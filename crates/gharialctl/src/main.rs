//! gharialctl: control tool for the gharial window manager.
//!
//! `main` only parses the argv slice into (socket, command, args),
//! handles the few client-side verbs, and forwards everything else to
//! the daemon. Helpers live in sibling modules.
//!
//! Exit status: 0 success, 1 the daemon refused the request, 2 usage
//! error, 3 the daemon could not be reached.

mod args;
mod usage;
mod wait;

use std::process::ExitCode;
use std::time::Duration;

use gharial_ipc::{send_with_timeout, Command, Request, Response};

use crate::args::{parse_timeout, split_socket_flag};
use crate::usage::usage;
use crate::wait::wait_for_daemon;

/// Reply timeout for ordinary requests (the daemon applies a command in
/// one manage sequence; this only matters if it is wedged).
const REPLY_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let (socket_override, rest) = match split_socket_flag(raw) {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("gharialctl: {msg}");
            return ExitCode::from(2);
        }
    };
    let Some((command, cmd_args)) = rest.split_first() else {
        usage(false);
        return ExitCode::from(2);
    };

    match command.as_str() {
        "-h" | "--help" | "help" => {
            usage(true);
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" => {
            println!("gharialctl {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let path = socket_override.unwrap_or_else(gharial_ipc::socket_path);

    let req = match (command.as_str(), cmd_args) {
        // `wait window …` blocks on a window; plain `wait` on the daemon.
        ("wait", [sub, rest @ ..]) if sub == "window" => Request::new("wait-window", rest.to_vec()),
        ("wait", rest) => {
            let timeout = match rest.first() {
                None => Duration::from_secs(2),
                Some(raw) => match parse_timeout(Some(raw)) {
                    Some(t) => t,
                    None => {
                        eprintln!(
                            "gharialctl: wait: invalid timeout {raw:?} (e.g. 2000, 500ms, 5s)"
                        );
                        return ExitCode::from(2);
                    }
                },
            };
            return wait_for_daemon(&path, timeout);
        }
        _ => Request::new(command.clone(), cmd_args.to_vec()),
    };

    // Validate locally so typos fail without a round trip, and learn how
    // long a `wait-window` may legitimately take.
    let reply_timeout = match Command::parse(&req) {
        Ok(Command::WaitWindow { timeout, .. }) => timeout + Duration::from_secs(5),
        Ok(_) => REPLY_TIMEOUT,
        Err(msg) => {
            eprintln!("gharialctl: {msg}");
            return ExitCode::from(2);
        }
    };

    match send_with_timeout(&path, &req, Some(reply_timeout)) {
        Ok(Response::Ok(body)) => {
            if !body.is_empty() {
                println!("{body}");
            }
            ExitCode::SUCCESS
        }
        Ok(Response::Err(body)) => {
            eprintln!("gharialctl: {body}");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!(
                "gharialctl: cannot reach gharial at {}: {e}\n\
                 (is the daemon running? try: pgrep -a gharial)",
                path.display()
            );
            ExitCode::from(3)
        }
    }
}
