//! Diagnostic verbs.

use gharial_ipc::Response;

pub fn ping() -> (Response, bool) {
    (Response::ok("pong"), false)
}

pub fn version() -> (Response, bool) {
    (Response::ok(env!("CARGO_PKG_VERSION")), false)
}
