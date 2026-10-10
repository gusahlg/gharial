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

use crate::action::{Action, BindingSpec, Direction};
use crate::color::Color;
use crate::command::{Command, ListFormat, DEFAULT_MODE};
use crate::info::{parse_outputs, parse_windows, OutputInfo, WindowInfo};
use crate::orientation::Orientation;
use crate::output::Presentation;
use crate::placement::Placement;
use crate::rule::Rule;
use crate::tags::TagSet;
use crate::value::BoolValue;
use crate::window::{WindowOp, WindowSelector};
use crate::wire::{send_with_timeout, Response, DEFAULT_REPLY_TIMEOUT};

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
    Timeout { socket: PathBuf, waited: Duration },
    /// The request could not be built (bad chord, malformed reply, …).
    Invalid(String),
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
            Self::Invalid(msg) => write!(f, "invalid request: {msg}"),
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
/// # fn main() -> gharial_ipc::Result<()> {
/// let g = gharial_ipc::Client::new();
/// g.set_gaps(8)?;
/// g.bind("Super+Q", gharial_ipc::Action::Close)?;
/// # Ok(())
/// # }
/// ```
///
/// State-changing calls return once the daemon has *applied* the change
/// (not merely queued it), so a script can act on the result right away.
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
            socket: crate::socket_path(),
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
        self.send(&Command::Ping).map(drop)
    }

    /// The daemon's own reported version string.
    pub fn version(&self) -> Result<String> {
        self.send(&Command::Version)
    }

    /// All layout / border parameters as one `key=value;...` line.
    pub fn status(&self) -> Result<String> {
        self.send(&Command::Status)
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
        self.send(&Command::Get(key.to_string()))
    }

    /// `set <key> <value>` — escape hatch for keys not yet covered by a
    /// typed constructor.
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.raw("set", [key, value]).map(drop)
    }

    /// Send an arbitrary IPC verb — escape hatch for verbs the typed
    /// API hasn't grown a method for yet. Returns the daemon's `ok`
    /// body verbatim.
    pub fn raw<S>(&self, command: &str, args: impl IntoIterator<Item = S>) -> Result<String>
    where
        S: AsRef<str>,
    {
        let req = crate::Request::new(
            command,
            args.into_iter().map(|s| s.as_ref().to_string()).collect(),
        );
        self.send_request(&req, DEFAULT_REPLY_TIMEOUT)
    }

    // ── Window management ────────────────────────────────────────────

    pub fn close(&self) -> Result<()> {
        self.execute(Action::Close)
    }
    pub fn toggle_float(&self) -> Result<()> {
        self.execute(Action::ToggleFloat)
    }
    pub fn toggle_fullscreen(&self) -> Result<()> {
        self.execute(Action::ToggleFullscreen)
    }
    pub fn toggle_sticky(&self) -> Result<()> {
        self.execute(Action::ToggleSticky)
    }
    pub fn focus(&self, dir: Direction) -> Result<()> {
        self.execute(Action::FocusDirection(dir))
    }
    pub fn swap(&self, dir: Direction) -> Result<()> {
        self.execute(Action::SwapDirection(dir))
    }

    /// Fork-exec a detached child and return its pid. Use the slice form
    /// to pass args: `client.spawn("rio", &["-e", "nvim"])?`.
    pub fn spawn<S>(&self, cmd: &str, args: &[S]) -> Result<u32>
    where
        S: AsRef<str>,
    {
        self.spawn_placed(Placement::default(), cmd, args)
    }

    /// Spawn a child whose first window (from it or any descendant) gets
    /// `placement`. Returns the child's pid.
    pub fn spawn_placed<S>(&self, placement: Placement, cmd: &str, args: &[S]) -> Result<u32>
    where
        S: AsRef<str>,
    {
        let action = Action::spawn_placed(placement, cmd, args.iter().map(AsRef::as_ref));
        let body = self.send(&Command::Action(action))?;
        parse_reply_number(&body)
    }

    /// Apply `op` to every window `selector` picks. Returns the ids of
    /// the windows affected (an empty match is an error).
    pub fn window(&self, selector: WindowSelector, op: WindowOp) -> Result<Vec<u64>> {
        let body = self.send(&Command::Action(Action::Window { selector, op }))?;
        body.lines()
            .map(|line| {
                line.trim()
                    .parse()
                    .map_err(|_| Error::Invalid(format!("unexpected window id {line:?}")))
            })
            .collect()
    }

    /// Apply a placement to the windows `selector` picks.
    pub fn place(&self, selector: WindowSelector, placement: Placement) -> Result<Vec<u64>> {
        self.window(selector, WindowOp::Place(placement))
    }

    /// Every window, or those `selector` picks.
    pub fn windows(&self, selector: Option<WindowSelector>) -> Result<Vec<WindowInfo>> {
        let body = self.send(&Command::ListWindows {
            selector,
            format: ListFormat::Tsv,
        })?;
        parse_windows(&body).map_err(Error::Invalid)
    }

    /// Block until a window matching `selector` is mapped at its final
    /// size, returning its id. `new_only` ignores windows already mapped
    /// when the wait begins.
    pub fn wait_window(
        &self,
        selector: WindowSelector,
        timeout: Duration,
        new_only: bool,
    ) -> Result<u64> {
        let cmd = Command::WaitWindow {
            selector,
            timeout,
            new_only,
        };
        let body = self.send_request(&cmd.to_request(), timeout + Duration::from_secs(5))?;
        parse_reply_number(&body)
    }

    // ── Rules ────────────────────────────────────────────────────────

    /// Install a window rule; returns its id. With `until_exit`, the
    /// rule disappears when that process exits.
    pub fn add_rule(&self, rule: Rule, until_exit: Option<u32>) -> Result<u64> {
        let body = self.send(&Command::AddRule { rule, until_exit })?;
        parse_reply_number(&body)
    }

    pub fn remove_rule(&self, id: u64) -> Result<()> {
        self.send(&Command::RemoveRule(id)).map(drop)
    }

    pub fn clear_rules(&self) -> Result<()> {
        self.send(&Command::ClearRules).map(drop)
    }

    /// Installed rules, one `ID<TAB>rule tokens` line each.
    pub fn rules(&self) -> Result<String> {
        self.send(&Command::ListRules)
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

    // ── Outputs (screens) ────────────────────────────────────────────

    /// Switch the focused output by direction.
    pub fn focus_output(&self, dir: Direction) -> Result<()> {
        self.execute(Action::focus_output(dir))
    }

    /// Switch the focused output by connector name or 1-based index.
    pub fn focus_output_named(&self, name: &str) -> Result<()> {
        self.execute(Action::focus_output_named(name))
    }

    /// Move the focused window to another output by direction.
    pub fn send_to_output(&self, dir: Direction) -> Result<()> {
        self.execute(Action::send_to_output(dir))
    }

    /// Move the focused window to a named/indexed output.
    pub fn send_to_output_named(&self, name: &str) -> Result<()> {
        self.execute(Action::send_to_output_named(name))
    }

    /// Configure whether `output focus` warps the pointer to the newly
    /// focused output.
    pub fn set_output_focus_warp(&self, value: impl Into<BoolValue>) -> Result<()> {
        self.execute(Action::set_output_focus_warp(value.into()))
    }

    /// Show `tags` on `output` without moving focus. With `until_exit`
    /// the previous view returns when that process exits.
    pub fn view_output_tags(
        &self,
        output: &str,
        tags: TagSet,
        until_exit: Option<u32>,
    ) -> Result<()> {
        let mut action = Action::view_output_tags(output, tags);
        if let Some(pid) = until_exit {
            action = action.until_exit(pid);
        }
        self.execute(action)
    }

    /// Set `output`'s presentation mode, optionally only while
    /// `until_exit` runs.
    pub fn set_presentation(
        &self,
        output: &str,
        mode: Presentation,
        until_exit: Option<u32>,
    ) -> Result<()> {
        let mut action = Action::set_presentation(output, mode);
        if let Some(pid) = until_exit {
            action = action.until_exit(pid);
        }
        self.execute(action)
    }

    /// Every output the daemon knows about.
    pub fn outputs(&self) -> Result<Vec<OutputInfo>> {
        let body = self.send(&Command::ListOutputs {
            format: ListFormat::Tsv,
        })?;
        parse_outputs(&body).map_err(Error::Invalid)
    }

    /// The `output list` text: one tab-separated line per output.
    pub fn list_outputs(&self) -> Result<String> {
        self.send(&Command::ListOutputs {
            format: ListFormat::Tsv,
        })
    }

    // ── Bindings & modes ─────────────────────────────────────────────

    /// Install a binding under the default mode.
    pub fn bind(&self, chord: &str, action: Action) -> Result<()> {
        self.bind_in_mode(DEFAULT_MODE, chord, action)
    }

    /// Install a binding under a named mode (the mode is auto-registered
    /// if not yet known).
    pub fn bind_in_mode(&self, mode: &str, chord: &str, action: Action) -> Result<()> {
        let spec = BindingSpec::parse(chord).map_err(Error::Invalid)?;
        self.send(&Command::Bind {
            mode: mode.to_string(),
            spec,
            action,
        })
        .map(drop)
    }

    /// Remove a binding from the default mode.
    pub fn unbind(&self, chord: &str) -> Result<()> {
        self.unbind_in_mode(DEFAULT_MODE, chord)
    }

    pub fn unbind_in_mode(&self, mode: &str, chord: &str) -> Result<()> {
        let spec = BindingSpec::parse(chord).map_err(Error::Invalid)?;
        self.send(&Command::Unbind {
            mode: mode.to_string(),
            spec,
        })
        .map(drop)
    }

    /// Switch the active binding mode.
    pub fn enter_mode(&self, name: &str) -> Result<()> {
        self.execute(Action::enter_mode(name))
    }

    /// Return to the default mode.
    pub fn exit_mode(&self) -> Result<()> {
        self.execute(Action::ExitMode)
    }

    // ── Action execution ─────────────────────────────────────────────

    /// Fire any [`Action`] directly — useful when scripting in a loop
    /// where the variant is computed dynamically.
    pub fn execute(&self, action: Action) -> Result<()> {
        self.send(&Command::Action(action)).map(drop)
    }

    /// Send any [`Command`] and return the daemon's `ok` body.
    pub fn send(&self, command: &Command) -> Result<String> {
        self.send_request(&command.to_request(), DEFAULT_REPLY_TIMEOUT)
    }

    fn send_request(&self, req: &crate::Request, timeout: Duration) -> Result<String> {
        match send_with_timeout(&self.socket, req, Some(timeout))? {
            Response::Ok(body) => Ok(body),
            Response::Err(msg) => Err(Error::Daemon(msg)),
        }
    }
}

/// Replies like `spawn`'s pid or `rule add`'s id lead with a number.
fn parse_reply_number<T: std::str::FromStr>(body: &str) -> Result<T> {
    body.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| Error::Invalid(format!("expected a number in reply, got {body:?}")))
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
        assert_eq!(c.socket(), crate::socket_path().as_path());
    }
}
