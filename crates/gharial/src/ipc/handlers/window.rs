//! Window-management IPC verbs: `close`, `focus`, `swap`, `spawn`.
//!
//! These don't touch layout params; they cross into the wayland thread
//! by pushing an [`Action`] onto the channel that `wm::run` installed
//! on `Shared`. Replies are immediate: an `ok` only confirms the action
//! was queued, not that it has been applied.

use gharial_ipc::Response;

use crate::action::{Action, Direction};
use crate::state::Shared;

/// Returned bool is always `false` for these verbs — they don't change
/// layout params, so they shouldn't trip the dirty-flag notifier (the
/// channel send already woke the wayland thread).
pub fn close(shared: &Shared) -> (Response, bool) {
    send(shared, Action::Close, "close queued")
}

pub fn toggle_float(shared: &Shared) -> (Response, bool) {
    send(shared, Action::ToggleFloat, "toggle-float queued")
}

pub fn focus(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let dir = match args.first() {
        Some(&s) => match Direction::parse(s) {
            Ok(d) => d,
            Err(e) => return (Response::err(e), false),
        },
        None => return (Response::err("focus: expected next|prev"), false),
    };
    send(shared, Action::FocusDirection(dir), "focus queued")
}

pub fn swap(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let dir = match args.first() {
        Some(&s) => match Direction::parse(s) {
            Ok(d) => d,
            Err(e) => return (Response::err(e), false),
        },
        None => return (Response::err("swap: expected next|prev"), false),
    };
    send(shared, Action::SwapDirection(dir), "swap queued")
}

pub fn spawn(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let Some((&cmd, rest)) = args.split_first() else {
        return (Response::err("spawn: missing command"), false);
    };
    let args: Vec<String> = rest.iter().map(|s| (*s).to_string()).collect();
    send(
        shared,
        Action::Spawn { cmd: cmd.to_string(), args },
        "spawn queued",
    )
}

fn send(shared: &Shared, action: Action, ok_msg: &str) -> (Response, bool) {
    match shared.send_action(action) {
        Ok(()) => (Response::ok(ok_msg), false),
        Err(e) => (Response::err(e), false),
    }
}
