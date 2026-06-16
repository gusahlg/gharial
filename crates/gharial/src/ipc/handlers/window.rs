//! Window-management IPC verbs: `close`, `focus`, `swap`, `spawn`.
//!
//! These don't touch layout params; they cross into the wayland thread
//! by pushing an [`Action`] onto the channel that `wm::run` installed
//! on `Shared`. Replies are immediate: an `ok` only confirms the action
//! was queued, not that it has been applied.

use gharial_ipc::Response;

use crate::action::{Action, Direction};
use crate::state::Shared;

use super::queue;

/// Returned bool is always `false` for these verbs — they don't change
/// layout params, so they shouldn't trip the dirty-flag notifier (the
/// channel send already woke the wayland thread).
pub fn close(shared: &Shared) -> (Response, bool) {
    queue(shared, Action::Close, "close queued")
}

pub fn toggle_float(shared: &Shared) -> (Response, bool) {
    queue(shared, Action::ToggleFloat, "toggle-float queued")
}

pub fn toggle_fullscreen(shared: &Shared) -> (Response, bool) {
    queue(shared, Action::ToggleFullscreen, "toggle-fullscreen queued")
}

pub fn focus(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let dir = match args.first() {
        Some(&s) => match Direction::parse(s) {
            Ok(d) => d,
            Err(e) => return (Response::err(e), false),
        },
        None => {
            return (
                Response::err("focus: expected next|prev|left|right|up|down"),
                false,
            )
        }
    };
    queue(shared, Action::FocusDirection(dir), "focus queued")
}

pub fn swap(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let dir = match args.first() {
        Some(&s) => match Direction::parse(s) {
            Ok(d) => d,
            Err(e) => return (Response::err(e), false),
        },
        None => {
            return (
                Response::err("swap: expected next|prev|left|right|up|down"),
                false,
            )
        }
    };
    queue(shared, Action::SwapDirection(dir), "swap queued")
}

pub fn spawn(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let Some((&cmd, rest)) = args.split_first() else {
        return (Response::err("spawn: missing command"), false);
    };
    let args: Vec<String> = rest.iter().map(|s| (*s).to_string()).collect();
    queue(
        shared,
        Action::Spawn {
            cmd: cmd.to_string(),
            args,
        },
        "spawn queued",
    )
}
