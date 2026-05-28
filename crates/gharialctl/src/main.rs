//! gharialctl: control tool for the gharial layout daemon.
//!
//! Responsibilities of `main` are deliberately narrow: parse the argv
//! slice into (socket, command, args), pattern-match on `command`, and
//! delegate everything else. Helpers live in sibling modules.

mod args;
mod usage;
mod wait;

use std::process::ExitCode;
use std::time::Duration;

use gharial_ipc::{send_one, Request, Response};

use crate::args::{parse_timeout, split_socket_flag};
use crate::usage::usage;
use crate::wait::wait_for_daemon;

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() {
        usage(false);
        return ExitCode::from(2);
    }

    let (socket_override, rest) = match split_socket_flag(raw) {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("gharialctl: {msg}");
            return ExitCode::from(2);
        }
    };

    let (command, cmd_args) = match rest.split_first() {
        Some((c, a)) => (c.clone(), a.to_vec()),
        None => {
            usage(false);
            return ExitCode::from(2);
        }
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

    if command == "wait" {
        let timeout =
            parse_timeout(cmd_args.first().map(String::as_str)).unwrap_or(Duration::from_secs(2));
        return wait_for_daemon(&path, timeout);
    }

    let req = Request::new(command, cmd_args);
    match send_one(&path, &req) {
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
            ExitCode::from(1)
        }
    }
}
