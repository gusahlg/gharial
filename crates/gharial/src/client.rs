//! `Client` — the typed Rust handle for talking to a running gharial
//! daemon over its IPC socket. Each method composes a `Request` from
//! the [`Action`] / value vocabulary, calls
//! [`gharial_ipc::send_one`], and surfaces either the daemon's `ok`
//! body or its `err` body as a typed [`Error`].
//!
//! Every method opens a fresh Unix-socket connection (the daemon's
//! protocol is one-shot per connection). `Client` itself is cheap to
//! construct and to clone — it holds only a `PathBuf`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use gharial_ipc::{send_one, Request, Response};

use crate::action::Action;
use crate::color::Color;
use crate::orientation::Orientation;
use crate::value::BoolValue;

/// Outcome of any [`Client`] method.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors surfaced by [`Client`] methods.
#[derive(Debug)]
pub enum Error {
    /// Couldn't reach the daemon — socket missing, permission denied,
    /// or a write/read failed mid-conversation.
    Io(std::io::Error),
    /// The daemon answered with `err <message>`. Returned verbatim.
    Daemon(String),
    /// `wait_until_ready` ran out of time before the daemon answered.
    Timeout {
        socket: PathBuf,
        waited: Duration,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "ipc transport error: {e}"),
            Self::Daemon(msg) => write!(f, "daemon refused: {msg}"),
            Self::Timeout { socket, waited } => write!(
                f,
                "daemon at {} did not respond within {:?}",
                socket.display(),
                waited
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Handle on a running gharial daemon.
///
/// ```no_run
/// # fn main() -> gharial::Result<()> {
/// let g = gharial::Client::new();
/// g.set_gaps(8)?;
/// g.bind("Super+Q", gharial::Action::Close)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Default constructor — resolves the socket path the same way
    /// `gharialctl` does (`$GHARIAL_SOCKET`, then the XDG runtime
    /// path).
    pub fn new() -> Self {
        Self {
            socket: gharial_ipc::socket_path(),
        }
    }

    /// Point at an explicit socket — useful in tests, in containerised
    /// setups, or when running multiple daemons.
    pub fn with_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket: path.into(),
        }
    }

    /// The path the client is talking to.
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    // ── Diagnostics ───────────────────────────────────────────────────

    /// Verify the daemon is reachable. Returns `Ok(())` if the socket
    /// answers `ok pong`.
    pub fn ping(&self) -> Result<()> {
        self.send_expect_ok("ping", &[])?;
        Ok(())
    }

    /// The daemon's own reported version string.
    pub fn version(&self) -> Result<String> {
        self.send_expect_ok("version", &[])
    }

    /// All layout / border parameters as one `key=value;...` line.
    pub fn status(&self) -> Result<String> {
        self.send_expect_ok("status", &[])
    }

    /// Block until the daemon answers a ping or `timeout` elapses.
    ///
    /// Polls every ~50 ms. Use this at the top of an init script so
    /// `set` / `bind` calls don't race the daemon coming up.
    pub fn wait_until_ready(&self, timeout: Duration) -> Result<()> {
        let started = Instant::now();
        let deadline = started + timeout;
        loop {
            if self.ping().is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout {
                    socket: self.socket.clone(),
                    waited: started.elapsed(),
                });
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    // ── Layout parameters ─────────────────────────────────────────────

    pub fn set_main_ratio(&self, value: f32) -> Result<()> {
        self.execute(Action::set_main_ratio(value))
    }
    pub fn adjust_main_ratio(&self, delta: f32) -> Result<()> {
        self.execute(Action::adjust_main_ratio(delta))
    }
    pub fn set_main_count(&self, value: u32) -> Result<()> {
        self.execute(Action::set_main_count(value))
    }
    pub fn adjust_main_count(&self, delta: i32) -> Result<()> {
        self.execute(Action::adjust_main_count(delta))
    }
    pub fn set_gaps(&self, value: u32) -> Result<()> {
        self.execute(Action::set_gaps(value))
    }
    pub fn adjust_gaps(&self, delta: i32) -> Result<()> {
        self.execute(Action::adjust_gaps(delta))
    }
    pub fn set_outer_padding(&self, value: u32) -> Result<()> {
        self.execute(Action::set_outer_padding(value))
    }
    pub fn adjust_outer_padding(&self, delta: i32) -> Result<()> {
        self.execute(Action::adjust_outer_padding(delta))
    }
    pub fn set_orientation(&self, value: Orientation) -> Result<()> {
        self.execute(Action::set_orientation(value))
    }
    pub fn set_smart_gaps(&self, value: impl Into<BoolValue>) -> Result<()> {
        self.execute(Action::set_smart_gaps(value.into()))
    }

    // ── Borders ───────────────────────────────────────────────────────

    pub fn set_border_width(&self, value: u32) -> Result<()> {
        self.execute(Action::set_border_width(value))
    }
    pub fn set_border_color_focused(&self, value: impl Into<Color>) -> Result<()> {
        self.execute(Action::set_border_color_focused(value.into()))
    }
    pub fn set_border_color_unfocused(&self, value: impl Into<Color>) -> Result<()> {
        self.execute(Action::set_border_color_unfocused(value.into()))
    }

    // ── Generic get/set/raw ──────────────────────────────────────────

    /// Read a single parameter as the daemon prints it.
    pub fn get(&self, key: &str) -> Result<String> {
        self.send_expect_ok("get", &[key])
    }

    /// `set <key> <value>` — escape hatch for keys not yet covered by a
    /// typed constructor.
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.send_expect_ok("set", &[key, value])?;
        Ok(())
    }

    /// Send an arbitrary IPC verb — escape hatch for verbs the typed
    /// API hasn't grown a method for yet. Returns the daemon's `ok`
    /// body verbatim.
    pub fn raw<S>(&self, command: &str, args: impl IntoIterator<Item = S>) -> Result<String>
    where
        S: AsRef<str>,
    {
        let owned: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let arg_refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.send_expect_ok(command, &arg_refs)
    }

    // ── Window management ────────────────────────────────────────────

    pub fn close(&self) -> Result<()> {
        self.execute(Action::Close)
    }
    pub fn toggle_float(&self) -> Result<()> {
        self.execute(Action::ToggleFloat)
    }
    pub fn focus(&self, dir: crate::action::Direction) -> Result<()> {
        self.execute(Action::FocusDirection(dir))
    }
    pub fn swap(&self, dir: crate::action::Direction) -> Result<()> {
        self.execute(Action::SwapDirection(dir))
    }

    /// Fork-exec a detached child. Use the slice form to pass args:
    /// `client.spawn("rio", &["-e", "nvim"])?`.
    pub fn spawn<S>(&self, cmd: &str, args: &[S]) -> Result<()>
    where
        S: AsRef<str>,
    {
        let mut tokens = Vec::with_capacity(args.len() + 1);
        tokens.push(cmd);
        tokens.extend(args.iter().map(AsRef::as_ref));
        self.send_expect_ok("spawn", &tokens)?;
        Ok(())
    }

    // ── Tags ──────────────────────────────────────────────────────────

    pub fn tag_focus(&self, n: u8) -> Result<()> {
        self.execute(Action::FocusTag(n))
    }
    pub fn tag_toggle(&self, n: u8) -> Result<()> {
        self.execute(Action::ToggleTag(n))
    }
    pub fn tag_move(&self, n: u8) -> Result<()> {
        self.execute(Action::MoveToTag(n))
    }
    pub fn tag_window_toggle(&self, n: u8) -> Result<()> {
        self.execute(Action::ToggleWindowTag(n))
    }

    // ── Bindings & modes ─────────────────────────────────────────────

    /// Install a binding under the default mode.
    pub fn bind(&self, chord: &str, action: Action) -> Result<()> {
        self.bind_in_mode_inner(None, chord, action)
    }

    /// Install a binding under a named mode (the mode is auto-registered
    /// if not yet known).
    pub fn bind_in_mode(&self, mode: &str, chord: &str, action: Action) -> Result<()> {
        self.bind_in_mode_inner(Some(mode), chord, action)
    }

    /// Remove a binding from the default mode.
    pub fn unbind(&self, chord: &str) -> Result<()> {
        self.unbind_in_mode_inner(None, chord)
    }

    pub fn unbind_in_mode(&self, mode: &str, chord: &str) -> Result<()> {
        self.unbind_in_mode_inner(Some(mode), chord)
    }

    /// Switch the active binding mode.
    pub fn enter_mode(&self, name: &str) -> Result<()> {
        self.send_expect_ok("mode", &[name])?;
        Ok(())
    }

    /// Return to the default mode.
    pub fn exit_mode(&self) -> Result<()> {
        self.send_expect_ok("mode", &["exit"])?;
        Ok(())
    }

    // ── Action execution ─────────────────────────────────────────────

    /// Fire any [`Action`] directly — useful when scripting in a loop
    /// where the variant is computed dynamically.
    ///
    /// Equivalent to whatever specific method matches the variant —
    /// `execute(Action::Close)` == `close()`, etc.
    pub fn execute(&self, action: Action) -> Result<()> {
        let tokens = action.to_tokens();
        let Some((command, args)) = tokens.split_first() else {
            return Err(Error::Daemon(
                "internal: action produced no tokens (Bind/Unbind aren't bindable)".into(),
            ));
        };
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.send_expect_ok(command, &arg_refs)?;
        Ok(())
    }

    // ── private helpers ──────────────────────────────────────────────

    fn bind_in_mode_inner(&self, mode: Option<&str>, chord: &str, action: Action) -> Result<()> {
        let action_tokens = action.to_tokens();
        if action_tokens.is_empty() {
            // Bind / Unbind / unknown — refuse client-side rather than
            // sending an empty `bind` request.
            return Err(Error::Daemon("action has no token encoding".into()));
        }
        let mut owned: Vec<String> = Vec::with_capacity(action_tokens.len() + 3);
        if let Some(mode_name) = mode {
            owned.push("--mode".into());
            owned.push(mode_name.into());
        }
        owned.push(chord.into());
        owned.extend(action_tokens);
        let args: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.send_expect_ok("bind", &args)?;
        Ok(())
    }

    fn unbind_in_mode_inner(&self, mode: Option<&str>, chord: &str) -> Result<()> {
        let mut owned: Vec<&str> = Vec::with_capacity(3);
        if let Some(mode_name) = mode {
            owned.push("--mode");
            owned.push(mode_name);
        }
        owned.push(chord);
        self.send_expect_ok("unbind", &owned)?;
        Ok(())
    }

    fn send_expect_ok(&self, command: &str, args: &[&str]) -> Result<String> {
        let req = Request::new(
            command,
            args.iter().map(|s| (*s).to_string()).collect(),
        );
        match send_one(&self.socket, &req)? {
            Response::Ok(body) => Ok(body),
            Response::Err(msg) => Err(Error::Daemon(msg)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_useful() {
        let e = Error::Daemon("nope".into());
        assert!(format!("{e}").contains("nope"));
        let e = Error::Timeout {
            socket: PathBuf::from("/tmp/sock"),
            waited: Duration::from_millis(250),
        };
        let s = format!("{e}");
        assert!(s.contains("/tmp/sock"));
        assert!(s.contains("250"));
    }

    #[test]
    fn client_with_socket_stores_path_verbatim() {
        let c = Client::with_socket("/tmp/x.sock");
        assert_eq!(c.socket(), Path::new("/tmp/x.sock"));
    }

    #[test]
    fn default_client_uses_socket_path_resolver() {
        let c = Client::default();
        assert_eq!(c.socket(), gharial_ipc::socket_path().as_path());
    }
}
