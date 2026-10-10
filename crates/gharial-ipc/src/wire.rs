//! Wire format: one newline-terminated request line in, one
//! newline-terminated response line out, one exchange per connection.
//!
//! Request tokens are whitespace-separated; a token containing
//! whitespace, quotes, or backslashes is double-quoted with `\"`, `\\`,
//! `\n`, and `\t` escapes. Response bodies escape `\\`, `\n`, and `\r`
//! so a multi-line body (a `window list`, say) still travels as one line.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SOCKET_ENV: &str = "GHARIAL_SOCKET";
pub const SOCKET_BASENAME: &str = "gharial";

/// Longest request or response line either side will read. Generous for
/// any real command (a `window list` of a few hundred windows fits),
/// small enough that a runaway peer can't make the other side buffer
/// unbounded input.
pub const MAX_LINE: u64 = 4 * 1024 * 1024;

/// Resolves the IPC socket path.
///
/// Precedence:
///   1. `$GHARIAL_SOCKET` if set.
///   2. `$XDG_RUNTIME_DIR/gharial-<WAYLAND_DISPLAY>.sock` if both set.
///   3. `$XDG_RUNTIME_DIR/gharial.sock`.
///   4. `/tmp/gharial-<user>.sock`.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os(SOCKET_ENV) {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let basename = match std::env::var_os("WAYLAND_DISPLAY") {
        Some(d) if !d.is_empty() => {
            // A WAYLAND_DISPLAY may be an absolute socket path; only its
            // file name is meaningful inside the runtime dir.
            let display = Path::new(&d).file_name().map(OsString::from).unwrap_or(d);
            let mut s = OsString::from(SOCKET_BASENAME);
            s.push("-");
            s.push(&display);
            s.push(".sock");
            s
        }
        _ => OsString::from(format!("{SOCKET_BASENAME}.sock")),
    };
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let mut p = PathBuf::from(dir);
        p.push(basename);
        return p;
    }
    // XDG_RUNTIME_DIR is always set when a Wayland session is running,
    // so this fallback is only relevant for non-Wayland clients.
    let user = std::env::var("USER").unwrap_or_else(|_| "default".into());
    PathBuf::from(format!("/tmp/{SOCKET_BASENAME}-{user}.sock"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub command: String,
    pub args: Vec<String>,
}

impl Request {
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
        }
    }

    /// Encode the request to a single line including the trailing `\n`.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        push_token(&mut out, &self.command);
        for arg in &self.args {
            out.push(' ');
            push_token(&mut out, arg);
        }
        out.push('\n');
        out
    }

    /// Parse a line (without trailing newline) into a Request.
    pub fn parse(line: &str) -> Result<Self, ParseError> {
        let mut tokens = tokenize(line)?.into_iter();
        let command = tokens.next().ok_or(ParseError::Empty)?;
        Ok(Self {
            command,
            args: tokens.collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Ok(String),
    Err(String),
}

impl Response {
    pub fn ok(body: impl Into<String>) -> Self {
        Self::Ok(body.into())
    }
    pub fn err(body: impl Into<String>) -> Self {
        Self::Err(body.into())
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn body(&self) -> &str {
        match self {
            Self::Ok(s) | Self::Err(s) => s,
        }
    }

    /// Encode to a single line including the trailing `\n`. Backslashes,
    /// newlines and carriage returns in the body are escaped so the
    /// response always fits on one line and round-trips exactly.
    pub fn encode(&self) -> String {
        let (tag, body) = match self {
            Self::Ok(b) => ("ok", b),
            Self::Err(b) => ("err", b),
        };
        let mut out = String::with_capacity(tag.len() + body.len() + 2);
        out.push_str(tag);
        if !body.is_empty() {
            out.push(' ');
            for ch in body.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    c => out.push(c),
                }
            }
        }
        out.push('\n');
        out
    }

    /// Parse a line (with or without trailing newline) into a Response.
    pub fn parse(line: &str) -> Result<Self, ParseError> {
        let line = line.trim_end_matches(['\r', '\n']);
        let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
        let body = unescape_body(rest);
        match tag {
            "ok" => Ok(Self::Ok(body)),
            "err" => Ok(Self::Err(body)),
            _ => Err(ParseError::BadTag(tag.to_string())),
        }
    }
}

fn unescape_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnterminatedQuote,
    BadTag(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty request"),
            Self::UnterminatedQuote => write!(f, "unterminated quoted string"),
            Self::BadTag(t) => write!(f, "unknown response tag: {t}"),
        }
    }
}

impl std::error::Error for ParseError {}

fn push_token(out: &mut String, tok: &str) {
    let needs_quote = tok.is_empty()
        || tok
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\');
    if !needs_quote {
        out.push_str(tok);
        return;
    }
    out.push('"');
    for c in tok.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn tokenize(line: &str) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut cur = String::new();
        if chars.next_if_eq(&'"').is_some() {
            loop {
                match chars.next() {
                    None => return Err(ParseError::UnterminatedQuote),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some('"') => cur.push('"'),
                        Some('\\') => cur.push('\\'),
                        Some('n') => cur.push('\n'),
                        Some('t') => cur.push('\t'),
                        Some(other) => {
                            cur.push('\\');
                            cur.push(other);
                        }
                        None => return Err(ParseError::UnterminatedQuote),
                    },
                    Some(c) => cur.push(c),
                }
            }
        } else {
            while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                cur.push(c);
            }
        }
        out.push(cur);
    }
    Ok(out)
}

/// Read one `\n`-terminated line of at most [`MAX_LINE`] bytes.
/// Returns `Ok(None)` on a clean EOF before any byte arrived.
pub fn read_line_bounded<R: Read>(reader: R) -> io::Result<Option<String>> {
    let mut reader = BufReader::new(reader.take(MAX_LINE));
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if !line.ends_with('\n') && n as u64 >= MAX_LINE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    Ok(Some(line))
}

/// Default time [`send_one`] waits for the daemon's answer. Ordinary
/// commands answer within milliseconds; the bound only matters when
/// the daemon is wedged, where hanging forever helps nobody.
pub const DEFAULT_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Send a single request to the daemon at `path` and read one response
/// line, waiting at most [`DEFAULT_REPLY_TIMEOUT`].
pub fn send_one(path: &Path, req: &Request) -> io::Result<Response> {
    send_with_timeout(path, req, Some(DEFAULT_REPLY_TIMEOUT))
}

/// [`send_one`] with an explicit reply timeout (`None` waits forever).
/// Long-running verbs such as `wait-window` pass their own deadline plus
/// some slack.
pub fn send_with_timeout(
    path: &Path,
    req: &Request,
    timeout: Option<Duration>,
) -> io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(req.encode().as_bytes())?;
    stream.flush()?;
    let line = read_line_bounded(&stream)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no response"))?;
    Response::parse(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_simple() {
        let r = Request::new("set", vec!["main-ratio".into(), "0.55".into()]);
        let encoded = r.encode();
        assert_eq!(encoded, "set main-ratio 0.55\n");
        assert_eq!(Request::parse(encoded.trim_end()).unwrap(), r);
    }

    #[test]
    fn request_roundtrip_quoted() {
        let r = Request::new(
            "bind",
            vec!["super+q".into(), "spawn rio -e nvim foo".into()],
        );
        let encoded = r.encode();
        assert_eq!(Request::parse(encoded.trim_end()).unwrap(), r);
    }

    #[test]
    fn request_round_trips_tabs_and_newlines() {
        let r = Request::new("rule", vec!["title=a\tb".into(), "c\nd".into()]);
        let encoded = r.encode();
        assert_eq!(encoded.matches('\n').count(), 1, "{encoded:?}");
        assert_eq!(Request::parse(encoded.trim_end()).unwrap(), r);
    }

    #[test]
    fn response_roundtrip() {
        let r = Response::ok("main-ratio=0.5;main-count=1");
        assert_eq!(Response::parse(&r.encode()).unwrap(), r);
    }

    #[test]
    fn response_multiline_body_is_escaped() {
        let r = Response::ok("a\nb");
        let s = r.encode();
        assert!(s.starts_with("ok a\\nb"));
        assert_eq!(Response::parse(&s).unwrap(), r);
    }

    #[test]
    fn tokenize_handles_internal_backslash_escapes() {
        let r = Request::parse(r#"say "hello\tworld\nbye""#).unwrap();
        assert_eq!(r.command, "say");
        assert_eq!(r.args, vec!["hello\tworld\nbye".to_string()]);
    }

    #[test]
    fn tokenize_unterminated_quote_errors() {
        let err = Request::parse(r#"say "hello"#).unwrap_err();
        assert_eq!(err, ParseError::UnterminatedQuote);
    }

    #[test]
    fn tokenize_empty_input_errors() {
        assert_eq!(Request::parse("").unwrap_err(), ParseError::Empty);
        assert_eq!(Request::parse("   ").unwrap_err(), ParseError::Empty);
    }

    #[test]
    fn response_parse_rejects_bogus_tag() {
        let err = Response::parse("nope hello").unwrap_err();
        assert!(matches!(err, ParseError::BadTag(t) if t == "nope"));
    }

    #[test]
    fn response_round_trips_special_characters() {
        // Literal backslash sequences in a body (a window title holding
        // `\n`, say) must survive verbatim — not decode to a newline.
        let bodies = [
            "plain text",
            "with \"quotes\" inside",
            "back\\slash\\here",
            r"literal \n stays two chars",
            "tab\there",
            "newline\nhere",
            "trailing backslash\\",
        ];
        for body in bodies {
            let r = Response::ok(body);
            let p = Response::parse(&r.encode()).unwrap_or_else(|e| panic!("{body:?}: {e}"));
            assert_eq!(p, r);
        }
    }

    #[test]
    fn request_encode_preserves_unicode() {
        let r = Request::new("spawn", vec!["tofi-drun".into(), "résumé".into()]);
        assert_eq!(Request::parse(r.encode().trim_end()).unwrap(), r);
    }

    #[test]
    fn empty_string_argument_is_quoted_through_a_round_trip() {
        let r = Request::new("set", vec!["key".into(), "".into()]);
        let encoded = r.encode();
        assert!(encoded.contains("\"\""));
        assert_eq!(Request::parse(encoded.trim_end()).unwrap(), r);
    }

    #[test]
    fn bounded_reader_rejects_oversized_lines() {
        let big = vec![b'a'; MAX_LINE as usize + 10];
        assert!(read_line_bounded(&big[..]).is_err());
        assert_eq!(read_line_bounded(&b""[..]).unwrap(), None);
        assert_eq!(
            read_line_bounded(&b"ping\nrest"[..]).unwrap().as_deref(),
            Some("ping\n")
        );
    }
}
