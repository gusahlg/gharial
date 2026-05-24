//! Layout-param IPC verbs: `main-ratio`, `gaps`, `set`, `get`, `status`.
//! These touch `Shared` directly (cheap mutex). A real change returns
//! `changed: true`, telling the IPC server to ping the wayland thread.

use gharial_ipc::Response;

use crate::state::Shared;

pub fn apply(shared: &Shared, key: &str, args: &[&str]) -> (Response, bool) {
    match shared.apply(key, args) {
        Ok(applied) => (Response::ok(applied.summary), applied.changed),
        Err(e) => (Response::err(e), false),
    }
}

pub fn set(shared: &Shared, args: &[&str]) -> (Response, bool) {
    match args.split_first() {
        Some((key, rest)) => apply(shared, key, rest),
        None => (Response::err("set: missing key"), false),
    }
}

pub fn get(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let resp = match args.first() {
        Some(&key) => match shared.get(key) {
            Ok(v) => Response::ok(v),
            Err(e) => Response::err(e),
        },
        None => Response::err("get: missing key"),
    };
    (resp, false)
}

pub fn status(shared: &Shared) -> (Response, bool) {
    (Response::ok(shared.status_line()), false)
}
