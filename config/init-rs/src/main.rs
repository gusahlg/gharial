//! Rust-based gharial init.
//!
//! This is an executable River init: it starts gharial, waits for its IPC
//! socket, applies the typed desktop policy, and then waits on the daemon so
//! River keeps the session alive. Configuration errors tear the daemon down
//! before the init exits, avoiding an orphaned compositor client.

mod desktop;

use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, process};

use gharial_ipc::{socket_path, Client};

fn main() {
    if let Err(error) = run() {
        eprintln!("gharial-init-rs: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    set_session_environment();
    propagate_environment();

    let client = Client::with_socket(socket_path());
    let mut daemon = Daemon::spawn()?;
    wait_for_daemon(&client, Duration::from_secs(5))?;

    desktop::configure(&client)?;

    // River owns the session lifetime. Once gharial exits, let this init exit
    // too so River can tear the session down normally.
    daemon
        .wait()
        .map(|_| ())
        .map_err(|error| format!("failed waiting for gharial: {error}"))
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

struct Daemon(Option<Child>);

impl Daemon {
    fn spawn() -> Result<Self, String> {
        let daemon = env::var_os("GHARIAL_DAEMON").unwrap_or_else(|| "gharial".into());
        Command::new(daemon)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map(|child| Self(Some(child)))
            .map_err(|error| format!("failed to start gharial: {error}"))
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.0
            .take()
            .expect("daemon child is present until wait")
            .wait()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        // Configuration failure must not leave a daemon running after River
        // has rejected this init. Ignore races with a child that exited first.
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn wait_for_daemon(client: &Client, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if client.ping().is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "daemon at {} did not answer within {:?}",
                client.socket().display(),
                timeout
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
