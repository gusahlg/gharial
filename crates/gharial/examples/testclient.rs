//! Deterministic Wayland client for gharial end-to-end tests.
//!
//! Opens one or more xdg toplevels, honours every configure exactly
//! (falling back to `--size` only for a 0 dimension), fills each
//! buffer with a solid colour, and logs one line per event so a shell
//! harness can grep it:
//!
//! ```text
//! ready <pid>
//! configure <idx> <width> <height> <states|->
//! bounds <idx> <width> <height>
//! commit <idx> <width> <height>
//! close <idx>
//! ```
//!
//! ```sh
//! WAYLAND_DISPLAY=wayland-2 cargo run --example testclient -- \
//!     --app-id foo --title bar --count 2 --exit-after 3
//! ```

use std::error::Error;
use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::time::{Duration, Instant};

use wayland_client::backend::WaylandError;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const USAGE: &str = "usage: testclient [--app-id ID] [--title T] [--color RRGGBB] \
[--size WxH] [--min WxH] [--max WxH] [--fullscreen] [--exit-after SECS] [--count N]";

/// xdg_toplevel.state names, indexed by `value - 1`; anything past
/// the end is printed as a raw number.
const STATE_NAMES: [&str; 9] = [
    "maximized",
    "fullscreen",
    "resizing",
    "activated",
    "tiled_left",
    "tiled_right",
    "tiled_top",
    "tiled_bottom",
    "suspended",
];

// Every event is logged with println!: std's stdout is line-buffered
// even into a pipe, so each line reaches the harness immediately.

struct Opts {
    app_id: String,
    title: String,
    color: u32,
    size: (i32, i32),
    min: Option<(i32, i32)>,
    max: Option<(i32, i32)>,
    fullscreen: bool,
    exit_after: Option<Duration>,
    count: usize,
}

fn parse_size(s: &str) -> Result<(i32, i32), String> {
    let bad = || format!("bad size {s:?}, want WxH");
    let (w, h) = s.split_once('x').ok_or_else(bad)?;
    Ok((w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?))
}

fn parse_args() -> Result<Opts, String> {
    let mut opts = Opts {
        app_id: "testclient".into(),
        title: "testclient".into(),
        color: 0x3366CC,
        size: (640, 480),
        min: None,
        max: None,
        fullscreen: false,
        exit_after: None,
        count: 1,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--app-id" => opts.app_id = value()?,
            "--title" => opts.title = value()?,
            "--color" => {
                let hex = value()?;
                let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16);
                opts.color = rgb.map_err(|_| format!("bad color {hex:?}"))? & 0xFF_FFFF;
            }
            "--size" => opts.size = parse_size(&value()?)?,
            "--min" => opts.min = Some(parse_size(&value()?)?),
            "--max" => opts.max = Some(parse_size(&value()?)?),
            "--fullscreen" => opts.fullscreen = true,
            "--exit-after" => {
                let v = value()?;
                let bad = || format!("bad seconds {v:?}");
                let secs: f64 = v.parse().map_err(|_| bad())?;
                opts.exit_after = Some(Duration::try_from_secs_f64(secs).map_err(|_| bad())?);
            }
            "--count" => {
                let v = value()?;
                opts.count = v.parse().map_err(|_| format!("bad count {v:?}"))?;
            }
            _ => return Err(format!("unknown option {flag}\n{USAGE}")),
        }
    }
    Ok(opts)
}

/// Comma-separated state names from a raw configure states array.
fn state_list(raw: &[u8]) -> String {
    let names: Vec<String> = (0..raw.len() / 4)
        .map(|i| u32::from_ne_bytes([raw[4 * i], raw[4 * i + 1], raw[4 * i + 2], raw[4 * i + 3]]))
        .map(|v| match STATE_NAMES.get((v as usize).wrapping_sub(1)) {
            Some(name) => name.to_string(),
            None => v.to_string(),
        })
        .collect();
    match names.is_empty() {
        true => "-".into(),
        false => names.join(","),
    }
}

struct Window {
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    /// Size from the latest xdg_toplevel.configure, applied on the
    /// following xdg_surface.configure.
    pending: (i32, i32),
}

struct App {
    opts: Opts,
    shm: wl_shm::WlShm,
    /// Indexed by toplevel idx; `None` once the compositor closed it.
    windows: Vec<Option<Window>>,
}

impl App {
    /// Paint and commit a solid buffer for window `idx` at its
    /// configured size, substituting `--size` for 0 dimensions.
    fn draw(&self, idx: usize, qh: &QueueHandle<Self>) -> io::Result<()> {
        let Some(win) = self.windows.get(idx).and_then(Option::as_ref) else {
            return Ok(());
        };
        let pick = |v: i32, fallback: i32| if v > 0 { v } else { fallback };
        let w = pick(win.pending.0, self.opts.size.0);
        let h = pick(win.pending.1, self.opts.size.1);
        let len = (w * 4) as usize * h as usize;

        // SAFETY: the name is NUL-terminated; a non-negative return is
        // a fresh fd that File takes ownership of.
        let fd = unsafe { libc::memfd_create(c"gharial-testclient".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.set_len(len as u64)?;
        let pixel = (0xFF00_0000 | self.opts.color).to_le_bytes();
        file.write_all(&pixel.repeat(len / 4))?;

        // The pool can go right away (the buffer keeps the memory);
        // the buffer itself is destroyed on wl_buffer.release.
        let pool = self.shm.create_pool(file.as_fd(), len as i32, qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, qh, ());
        pool.destroy();

        win.surface.attach(Some(&buffer), 0, 0);
        win.surface.damage_buffer(0, 0, w, h);
        win.surface.commit();
        println!("commit {idx} {w} {h}");
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(App: wl_compositor::WlCompositor);
delegate_noop!(App: wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_surface::WlSurface);

impl Dispatch<wl_buffer::WlBuffer, ()> for App {
    fn event(
        _: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            buffer.destroy();
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for App {
    fn event(
        app: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        idx: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            if let Err(err) = app.draw(*idx, qh) {
                eprintln!("testclient: draw {idx}: {err}");
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for App {
    fn event(
        app: &mut Self,
        toplevel: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let (idx, win) = (*idx, app.windows.get_mut(*idx));
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                println!("configure {idx} {width} {height} {}", state_list(&states));
                if let Some(Some(win)) = win {
                    win.pending = (width, height);
                }
            }
            xdg_toplevel::Event::ConfigureBounds { width, height } => {
                println!("bounds {idx} {width} {height}");
            }
            xdg_toplevel::Event::Close => {
                println!("close {idx}");
                toplevel.destroy();
                if let Some(win) = win.and_then(Option::take) {
                    win.xdg_surface.destroy();
                    win.surface.destroy();
                }
            }
            _ => {} // wm_capabilities: nothing to adapt
        }
    }
}

/// Wait up to `timeout` (forever if `None`) for the socket to become
/// readable, then read whatever arrived into the queue.
fn read_events(queue: &EventQueue<App>, timeout: Option<Duration>) -> Result<(), WaylandError> {
    let Some(guard) = queue.prepare_read() else {
        return Ok(()); // events already queued; the caller dispatches them
    };
    let ms = timeout.map_or(-1, |t| t.as_millis().clamp(1, i32::MAX as u128) as i32);
    let fd = guard.connection_fd().as_raw_fd();
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd for the duration of the call.
    if unsafe { libc::poll(&mut pfd, 1, ms) } <= 0 {
        return Ok(()); // timeout or EINTR; dropping the guard cancels the read
    }
    match guard.read() {
        Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => Ok(()),
        other => other.map(drop),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = parse_args().unwrap_or_else(|msg| {
        eprintln!("{msg}");
        std::process::exit(2);
    });

    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<App>(&conn)?;
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 4..=6, ())?;
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())?;
    // Capped at 6 so every state the compositor may send has a name.
    let wm_base: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=6, ())?;
    println!("ready {}", std::process::id());

    let deadline = opts.exit_after.map(|d| Instant::now() + d);
    let mut app = App {
        windows: Vec::with_capacity(opts.count),
        shm,
        opts,
    };
    for idx in 0..app.opts.count {
        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, idx);
        let toplevel = xdg_surface.get_toplevel(&qh, idx);
        toplevel.set_app_id(app.opts.app_id.clone());
        toplevel.set_title(match app.opts.count {
            1 => app.opts.title.clone(),
            _ => format!("{}-{idx}", app.opts.title),
        });
        if let Some((w, h)) = app.opts.min {
            toplevel.set_min_size(w, h);
        }
        if let Some((w, h)) = app.opts.max {
            toplevel.set_max_size(w, h);
        }
        if app.opts.fullscreen {
            toplevel.set_fullscreen(None);
        }
        surface.commit(); // no buffer yet: this asks for the first configure
        app.windows.push(Some(Window {
            surface,
            xdg_surface,
            pending: (0, 0),
        }));
    }

    while app.windows.iter().any(Option::is_some) {
        queue.dispatch_pending(&mut app)?;
        conn.flush()?;
        let timeout = match deadline {
            Some(d) if Instant::now() >= d => break,
            Some(d) => Some(d.saturating_duration_since(Instant::now())),
            None => None,
        };
        read_events(&queue, timeout)?;
    }
    conn.flush()?;
    Ok(())
}
