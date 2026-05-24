//! `tag` IPC verb — change which tags are active or which tags a window
//! belongs to.
//!
//! Grammar:
//!   tag focus <N>             show only tag N (1..32)
//!   tag toggle <N>            add/remove tag N from the active set
//!   tag move <N>              send focused window to tag N
//!   tag window-toggle <N>     add/remove tag N from focused window

use gharial_ipc::Response;

use crate::action::Action;
use crate::state::Shared;

pub fn tag(shared: &Shared, args: &[&str]) -> (Response, bool) {
    // Reuse the same parser as bound `tag ...` actions so the two
    // surfaces never drift.
    let action = match Action::parse(&prepend_tag(args)) {
        Ok(a) => a,
        Err(e) => return (Response::err(e), false),
    };
    match shared.send_action(action) {
        Ok(()) => (Response::ok("tag queued"), false),
        Err(e) => (Response::err(e), false),
    }
}

fn prepend_tag<'a>(args: &'a [&'a str]) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push("tag");
    out.extend_from_slice(args);
    out
}
