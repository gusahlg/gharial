//! `Action::Spawn` — detached process launch, optionally with placement
//! for the windows it opens.
//!
//! The child gets a clean signal disposition (SIGPIPE → default) and its
//! own session (`setsid`), so signals aimed at gharial don't reach it and
//! — usefully — every process it starts stays in that session, which is
//! how a placed spawn recognises windows from deep descendants.
//!
//! Each child is reaped by a tiny thread blocked in `wait()` on exactly
//! that child. (A global `waitpid(-1)` reaper races std's own
//! `waitpid(pid)` after a failed exec — std then panics, and with
//! `panic = "abort"` that takes the window manager down.)

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;

use gharial_ipc::{Matcher, Placement, Rule};

use super::super::world::World;
use super::Outcome;

/// Launch `cmd`, returning its pid. A non-empty `placement` becomes a
/// oneshot rule matching the child's process tree; it lapses with the
/// child.
pub fn spawn(world: &mut World, cmd: &str, args: &[String], placement: &Placement) -> Outcome {
    let pid = run(cmd, args)?;
    if !placement.is_empty() {
        let rule = Rule::new(Matcher::pid(pid), placement.clone()).oneshot();
        world.rules.add(rule, None);
        super::super::ticker::arm(world);
    }
    Ok(pid.to_string())
}

fn run(cmd: &str, args: &[String]) -> Result<u32, String> {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Safety: pre_exec runs in the forked child before exec; signal()
    // and setsid() are async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::setsid();
            Ok(())
        });
    }
    let child = command
        .spawn()
        .map_err(|e| format!("spawn {cmd:?} failed: {e}"))?;
    let pid = child.id();
    reap_when_done(child);
    Ok(pid)
}

/// Collect the child's exit status when it ends, so it never lingers as
/// a zombie.
fn reap_when_done(mut child: Child) {
    let reaper = thread::Builder::new()
        .name("gharial-reap".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let _ = child.wait();
        });
    if let Err(e) = reaper {
        // The child still runs; it just stays a zombie after exiting.
        eprintln!("gharial: cannot start a reaper thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_exec_is_an_error_not_a_crash() {
        // A living child must not interfere with std's own wait after a
        // failed exec (the old global reaper did).
        let living = run("sleep", &["2".into()]).unwrap();
        assert!(living > 0);
        let err = run("gharial-definitely-not-a-command", &[]).unwrap_err();
        assert!(err.contains("failed"), "{err}");
    }

    #[test]
    fn exited_children_are_reaped() {
        let pid = run("true", &[]).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "child {pid} was not reaped"
            );
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
