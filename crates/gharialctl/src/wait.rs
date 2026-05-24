//! Implementation of the `wait` subcommand: poll the daemon's socket
//! until it answers a ping, or fail after a deadline.

use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use gharial_ipc::{send_one, Request};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn wait_for_daemon(path: &Path, timeout: Duration) -> ExitCode {
    let deadline = Instant::now() + timeout;
    let probe = Request::new("ping", vec![]);
    loop {
        if send_one(path, &probe).map(|r| r.is_ok()).unwrap_or(false) {
            return ExitCode::SUCCESS;
        }
        if Instant::now() >= deadline {
            eprintln!(
                "gharialctl: daemon did not respond at {} within {:?}",
                path.display(),
                timeout
            );
            return ExitCode::from(1);
        }
        thread::sleep(POLL_INTERVAL);
    }
}
