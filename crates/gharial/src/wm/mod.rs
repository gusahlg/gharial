//! Window-management runtime built on `river-window-management-v1`,
//! `river-xkb-bindings-v1`, and `river-layer-shell-v1`. Public surface
//! is just [`run`]; the rest is the `World` state machine and the
//! per-interface dispatch tables.

/// Background thread that reaps every exited child process the daemon
/// (or its children) leaves behind. Started once at startup.
///
/// We *don't* use `SIGCHLD = SIG_IGN` for auto-reaping. While simpler,
/// SIG_IGN poisons the signal disposition for the whole process, which
/// in turn breaks std's `posix_spawn` / exec-pipe handshake (it relies
/// on default SIGCHLD behaviour to deliver the child's exec result).
/// A reaper thread keeps SIGCHLD at its default disposition for the
/// parent and reliably consumes zombies in the background.
fn start_child_reaper() {
    std::thread::Builder::new()
        .name("gharial-reaper".into())
        .spawn(|| loop {
            // waitpid(-1, ..., 0) blocks until any child changes state.
            // Returns -1 / ECHILD when no children exist; sleep then
            // retry. The thread costs ~one kernel object and runs for
            // the lifetime of the daemon.
            let mut status: libc::c_int = 0;
            // Safety: waitpid is async-signal-safe and operates on
            // shared process state; no Rust invariants violated.
            let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
            if pid < 0 {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        })
        .expect("spawn child-reaper thread");
}

mod actions;
mod bindings;
mod dispatch;
mod focus;
mod globals;
mod modes;
mod outputs;
mod render;
mod seats;
mod sequence;
mod spatial;
mod tags;
mod windows;
mod world;

use std::error::Error;
use std::sync::Arc;

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use crate::state::Shared;

pub use world::World;

/// Connect to wayland, bind river globals, run the manage/render loop
/// until something tells us to stop.
pub fn run(shared: Shared) -> Result<(), Box<dyn Error>> {
    start_child_reaper();

    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<World>(&conn)?;
    let qh = queue.handle();

    let bound = globals::bind_all(&globals, &qh)?;

    let mut event_loop: EventLoop<World> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn.clone(), queue).insert(loop_handle.clone())?;

    // IPC-to-wayland wake-up: layout-param dirty flag. When params
    // change through IPC, the compositor has no idea, so we ask for a
    // fresh manage sequence so the next render uses the new values.
    let (ping, ping_source) = calloop::ping::make_ping()?;
    loop_handle.insert_source(ping_source, |_, _, world: &mut World| {
        if world.shared.take_dirty() {
            world.mark_layout_dirty();
            world.globals.manager.manage_dirty();
        }
    })?;

    // IPC-to-wayland action channel: window-management commands (close,
    // focus, swap, spawn) flow through here. They land in
    // `world.pending_actions` and are drained at the next manage_start.
    let (action_tx, action_rx) = calloop::channel::channel::<crate::action::Action>();
    loop_handle.insert_source(action_rx, |event, _, world: &mut World| {
        if let calloop::channel::Event::Msg(action) = event {
            world.pending_actions.push_back(action);
            world.globals.manager.manage_dirty();
        }
    })?;
    shared.set_action_sender(action_tx);

    let notifier: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ping.ping());
    let ipc = crate::ipc::Server::start_with_notifier(shared.clone(), notifier)?;
    eprintln!("gharial: ipc socket at {}", ipc.socket_path.display());

    let mut world = World::new(shared, bound, qh.clone());

    while world.running() {
        event_loop.dispatch(None, &mut world)?;
    }
    drop(ipc);
    Ok(())
}
