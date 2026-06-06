//! `Action::Spawn` execution — the only action with no protocol effect.
//!
//! Detached process launch: we set up a clean signal disposition for
//! the child (SIGPIPE → default, `setsid()` to break the controlling
//! terminal association) and drop the `Child` handle so the daemon
//! doesn't have to track it. The background reaper thread that
//! `wm::run` starts consumes zombies via `waitpid(-1, …, 0)`.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

pub(in crate::wm) fn run(cmd: &str, args: &[String]) {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // setsid() detaches the child from gharial's controlling terminal
    // and process group so signals (SIGINT, SIGHUP, …) sent to us
    // don't propagate to it. SIGPIPE goes back to default just in
    // case some library on our side ever changes it.
    //
    // Safety: pre_exec runs in a single-threaded post-fork context.
    // signal/setsid are both async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::setsid();
            Ok(())
        });
    }
    match command.spawn() {
        // Detach: drop the Child handle. The background reaper thread
        // started in wm::run consumes the eventual zombie via waitpid.
        Ok(child) => drop(child),
        Err(e) => eprintln!("gharial: spawn {cmd:?} failed: {e}"),
    }
}
