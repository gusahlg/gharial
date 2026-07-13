//! Per-verb IPC handlers. Each module owns one verb family. The
//! dispatcher in [`super::dispatch`] just routes by verb and reports
//! back whether the response should trigger a notifier.

pub mod bind;
pub mod layout;
pub mod misc;
pub mod mode;
pub mod output;
pub mod tag;
pub mod window;

use gharial_ipc::Response;

use crate::action::Action;
use crate::state::Shared;

/// Forward an action to the wayland thread and translate the channel
/// outcome into an `(Ok|Err, false)` IPC reply. The `false` reflects
/// that action-channel verbs don't dirty layout params themselves — the
/// channel send already wakes the wayland thread.
pub(super) fn queue(shared: &Shared, action: Action, ok_msg: &str) -> (Response, bool) {
    match shared.send_action(action) {
        Ok(()) => (Response::ok(ok_msg), false),
        Err(e) => (Response::err(e), false),
    }
}
