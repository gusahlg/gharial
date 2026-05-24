//! `mode` IPC verb — switches the active binding mode.
//!
//! Grammar:
//!   mode <name>      enter the named mode
//!   mode exit        return to "default"

use gharial_ipc::Response;

use crate::action::Action;
use crate::state::Shared;

pub fn mode(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let target = match args.first() {
        Some(&t) => t,
        None => return (Response::err("mode: expected <name|exit>"), false),
    };
    let action = if target == "exit" {
        Action::ExitMode
    } else {
        Action::EnterMode(target.to_string())
    };
    match shared.send_action(action) {
        Ok(()) => (Response::ok("mode queued"), false),
        Err(e) => (Response::err(e), false),
    }
}
