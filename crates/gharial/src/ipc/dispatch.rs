//! Per-connection request handling: read one line, parse it into a
//! [`Command`], answer it, write one line back.
//!
//! `ping` and `version` are answered right here, so `gharialctl wait`
//! works even while the wayland thread is busy; everything else goes to
//! the wayland thread through the [`Bridge`].

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use gharial_ipc::wire::read_line_bounded;
use gharial_ipc::{Command, Request, Response};

use crate::bridge::Bridge;

/// How long a client may take to send its request line.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the wayland thread gets to apply a command. Generous: a
/// manage sequence can be held up by slow clients, but a healthy
/// session answers in milliseconds.
const APPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Extra time past a `wait-window` deadline before giving up on the
/// wayland thread's own timeout reply.
const WAIT_SLACK: Duration = Duration::from_secs(2);

pub(super) fn handle_client(stream: UnixStream, bridge: &Bridge) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let Some(line) = read_line_bounded(&stream)? else {
        return Ok(());
    };
    let response = match Request::parse(line.trim_end_matches(['\r', '\n'])) {
        Ok(req) => answer(&req, bridge),
        Err(e) => Response::err(e.to_string()),
    };
    let mut writer = stream;
    writer.write_all(response.encode().as_bytes())?;
    writer.flush()
}

pub(super) fn answer(req: &Request, bridge: &Bridge) -> Response {
    let command = match Command::parse(req) {
        Ok(command) => command,
        Err(e) => return Response::err(e),
    };
    match command {
        Command::Ping => Response::ok("pong"),
        Command::Version => Response::ok(env!("CARGO_PKG_VERSION")),
        Command::WaitWindow { timeout, .. } => bridge.request(command, timeout + WAIT_SLACK),
        command => bridge.request(command, APPLY_TIMEOUT),
    }
}
