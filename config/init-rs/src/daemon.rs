//! Running and supervising the gharial daemon.
//!
//! River ends the session when its init exits, so the init must outlive
//! a gharial crash. River lets a restarted window manager adopt every
//! open window, so the supervisor restarts gharial (and re-applies the
//! policy) instead of taking the whole desktop down — unless it is
//! crash-looping, or River itself is gone.

use std::collections::VecDeque;
use std::env;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// More restarts than this within [`CRASH_WINDOW`] means something is
/// persistently wrong; give up rather than spin.
const MAX_RESTARTS: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(60);
const RESTART_DELAY: Duration = Duration::from_millis(500);

pub struct Daemon(Option<Child>);

impl Daemon {
    pub fn spawn() -> Result<Self, String> {
        let daemon = env::var_os("GHARIAL_DAEMON").unwrap_or_else(|| "gharial".into());
        Command::new(daemon)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map(|child| Self(Some(child)))
            .map_err(|error| format!("failed to start gharial: {error}"))
    }

    pub fn wait(&mut self) -> Result<ExitStatus, String> {
        match self.0.take() {
            Some(mut child) => child
                .wait()
                .map_err(|error| format!("failed waiting for gharial: {error}")),
            None => Err("gharial is not running".into()),
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // Only reached on an init error path; don't leave a daemon behind.
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Decides whether a gharial exit should be followed by a restart.
#[derive(Default)]
pub struct Supervisor {
    recent: VecDeque<Instant>,
}

impl Supervisor {
    /// `Ok(())` to restart, `Err(reason)` to end the session.
    pub fn should_restart(&mut self, status: ExitStatus) -> Result<(), String> {
        if status.success() {
            return Err("gharial exited normally".into());
        }
        if !compositor_alive() {
            return Err(format!("gharial exited ({status}) and River is gone"));
        }
        let now = Instant::now();
        self.recent.push_back(now);
        while self
            .recent
            .front()
            .is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW)
        {
            self.recent.pop_front();
        }
        if self.recent.len() > MAX_RESTARTS {
            return Err(format!(
                "gharial crashed {} times within {:?}; giving up",
                self.recent.len(),
                CRASH_WINDOW
            ));
        }
        eprintln!("gharial-init-rs: gharial exited ({status}); restarting");
        thread::sleep(RESTART_DELAY);
        Ok(())
    }
}

/// Whether River's Wayland socket still accepts connections.
fn compositor_alive() -> bool {
    let Some(display) = env::var_os("WAYLAND_DISPLAY") else {
        return false;
    };
    let mut path = PathBuf::from(&display);
    if path.is_relative() {
        match env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) => path = PathBuf::from(dir).join(display),
            None => return false,
        }
    }
    UnixStream::connect(path).is_ok()
}
