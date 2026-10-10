//! Rust-based gharial init.
//!
//! An executable River init: it starts gharial, applies the typed desktop
//! policy, runs autostart programs once, and keeps gharial running for
//! the life of the session — restarting it (and re-applying the policy)
//! if it ever crashes, since River lets a new window manager adopt every
//! open window. See [`daemon`] for the restart policy.

mod daemon;
mod desktop;

use std::env;
use std::process::{self, Command};
use std::time::Duration;

use gharial_ipc::Client;

use crate::daemon::{Daemon, Supervisor};

fn main() {
    if let Err(error) = run() {
        eprintln!("gharial-init-rs: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    set_session_environment();
    propagate_environment();

    let client = Client::new();
    let mut supervisor = Supervisor::default();
    let mut autostarted = false;
    loop {
        let mut daemon = Daemon::spawn()?;
        match client.wait_until_ready(Duration::from_secs(10)) {
            Ok(()) => {
                // A policy mistake must not cost the session: report it
                // and keep the daemon (and every open window) running.
                if let Err(error) = desktop::configure(&client) {
                    eprintln!("gharial-init-rs: applying the desktop policy: {error}");
                }
                if !autostarted {
                    desktop::autostart(&client);
                    autostarted = true;
                }
            }
            Err(error) => eprintln!("gharial-init-rs: {error}"),
        }
        let status = daemon.wait()?;
        supervisor.should_restart(status)?;
    }
}

fn set_session_environment() {
    env::set_var("XDG_CURRENT_DESKTOP", "river");
    env::set_var("XDG_SESSION_TYPE", "wayland");
}

fn propagate_environment() {
    // FONT is intentionally included when supplied by the system config. It
    // is not hard-coded here because Nix store paths change on rebuild.
    let vars = [
        "WAYLAND_DISPLAY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "FONT",
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
