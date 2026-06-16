//! Thin wrappers around `gharial_ipc::send_one` that mirror the
//! `gharialctl` verbs we use from a config: `set`, `bind`, and `spawn`.
//!
//! Each call sends one request, asserts the daemon answered `ok`, and
//! surfaces the failing line if it didn't — so a typo at startup fails
//! loudly instead of silently dropping a binding.

use std::path::Path;

use gharial_ipc::{send_one, Request, Response};

pub fn send(socket: &Path, command: &str, args: &[&str]) -> Result<(), String> {
    let args = args.iter().map(|s| s.to_string()).collect();
    let req = Request::new(command, args);
    match send_one(socket, &req) {
        Ok(Response::Ok(_)) => Ok(()),
        Ok(Response::Err(body)) => Err(format!("{}: {body}", req.encode().trim_end())),
        Err(e) => Err(format!("{}: {e}", req.encode().trim_end())),
    }
}

pub fn set(socket: &Path, key: &str, value: &[&str]) -> Result<(), String> {
    let mut args = Vec::with_capacity(value.len() + 1);
    args.push(key);
    args.extend_from_slice(value);
    send(socket, "set", &args)
}

pub fn bind(socket: &Path, chord: &str, action: &[&str]) -> Result<(), String> {
    let mut args = Vec::with_capacity(action.len() + 1);
    args.push(chord);
    args.extend_from_slice(action);
    send(socket, "bind", &args)
}

pub fn bind_in_mode(
    socket: &Path,
    mode: &str,
    chord: &str,
    action: &[&str],
) -> Result<(), String> {
    let mut args = Vec::with_capacity(action.len() + 3);
    args.push("--mode");
    args.push(mode);
    args.push(chord);
    args.extend_from_slice(action);
    send(socket, "bind", &args)
}

pub fn spawn(socket: &Path, argv: &[&str]) -> Result<(), String> {
    send(socket, "spawn", argv)
}
