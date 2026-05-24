//! Per-connection request handling. Reads exactly one line, parses it
//! into an IPC `Request`, and writes back exactly one `Response` line.
//!
//! The dispatch table is intentionally flat — adding a new verb means
//! adding a new arm here and a function in [`super::handlers`].

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use gharial_ipc::{Request, Response};

use crate::state::Shared;

use super::handlers::{bind, layout, misc, mode, tag, window};
use super::Notifier;

pub(super) fn handle_client(
    stream: UnixStream,
    shared: &Shared,
    notifier: Option<&Notifier>,
) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(());
    }
    let (response, changed) = match Request::parse(line.trim_end_matches(['\r', '\n'])) {
        Ok(req) => dispatch(req, shared),
        Err(e) => (Response::err(e.to_string()), false),
    };
    if changed {
        if let Some(n) = notifier {
            n();
        }
    }
    let mut writer = stream;
    writer.write_all(response.encode().as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn dispatch(req: Request, shared: &Shared) -> (Response, bool) {
    let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
    match req.command.as_str() {
        // Layout params — also accepted from river user_command paths.
        "main-ratio" | "main-count" | "gaps" | "outer-padding" | "orientation"
        | "smart-gaps" | "border-width" | "border-color-focused"
        | "border-color-unfocused" => layout::apply(shared, &req.command, &args),
        "set" => layout::set(shared, &args),
        "get" => layout::get(shared, &args),
        "status" => layout::status(shared),

        // Window-management verbs that route through the action channel.
        "close" => window::close(shared),
        "toggle-float" => window::toggle_float(shared),
        "focus" => window::focus(shared, &args),
        "swap" => window::swap(shared, &args),
        "spawn" => window::spawn(shared, &args),

        // Bindings & modes.
        "bind" => bind::bind(shared, &args),
        "unbind" => bind::unbind(shared, &args),
        "mode" => mode::mode(shared, &args),

        // Tags.
        "tag" => tag::tag(shared, &args),

        // Diagnostics.
        "ping" => misc::ping(),
        "version" => misc::version(),

        cmd => (Response::err(format!("unknown command: {cmd}")), false),
    }
}
