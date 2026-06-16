//! Rust-based gharial init.
//!
//! A behavioural twin of `config/init` (the shell version) written as a
//! Cargo binary. It does the same four jobs:
//!
//!   1. Set the session environment and propagate it to dbus and systemd.
//!   2. Spawn the `gharial` daemon and wait until its socket answers.
//!   3. Drive the daemon over IPC to install layout params, bindings, and
//!      modes — the same calls a shell init makes through `gharialctl`.
//!   4. Ask the daemon to spawn autostart programs, then block on the
//!      daemon process so river doesn't tear down the session.
//!
//! This is one example of a Rust-shaped config; the only thing gharial
//! cares about is the IPC requests on the wire, so any language with a
//! Unix-socket client can play the same role.

mod desktop;
mod ipc;

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, process};

use gharial_ipc::{send_one, socket_path, Request};

fn main() {
    if let Err(e) = run() {
        eprintln!("gharial-init-rs: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    set_session_environment();
    propagate_environment();

    let socket = socket_path();
    let mut daemon = spawn_daemon()?;
    wait_for_daemon(&socket, Duration::from_secs(5))?;

    desktop::configure(&socket)?;

    // Block on the daemon so river keeps the session alive. When gharial
    // exits, this binary exits, and river tears the session down.
    let _ = daemon.wait();
    Ok(())
}

fn set_session_environment() {
    env::set_var("XDG_CURRENT_DESKTOP", "river");
    env::set_var("XDG_SESSION_TYPE", "wayland");
    env::set_var("FONT", desktop::FONT);
}

fn propagate_environment() {
    let vars = [
        "WAYLAND_DISPLAY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
    ];
    let _ = Command::new("dbus-update-activation-environment")
        .arg("--systemd")
        .args(vars)
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "import-environment"])
        .args(vars)
        .status();
}

fn spawn_daemon() -> Result<Child, String> {
    Command::new("gharial")
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start gharial: {e}"))
}

fn wait_for_daemon(socket: &Path, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let probe = Request::new("ping", Vec::new());
    loop {
        if let Ok(resp) = send_one(socket, &probe) {
            if resp.is_ok() {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "daemon at {} did not answer within {:?}",
                socket.display(),
                timeout
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
