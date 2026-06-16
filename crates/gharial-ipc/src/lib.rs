//! IPC protocol and typed control vocabulary shared between the gharial
//! daemon, `gharialctl`, and Rust-based configs.
//!
//! Wire format: a single newline-terminated request line, a single
//! newline-terminated response line. Tokens are whitespace-separated, with
//! double-quoted strings supporting `\"` and `\\` escapes.
//!
//! On top of the raw wire types this crate owns the *vocabulary* every
//! client speaks — [`Action`], [`Color`], [`Orientation`], [`BoolValue`],
//! the keysym table, and the [`Client`] handle. The daemon re-exports
//! these (so there is exactly one definition of the wire grammar), and
//! because this crate has no Wayland dependencies a config binary can
//! depend on it alone and still build in well under a second.

pub mod action;
pub mod client;
pub mod color;
pub mod keysyms;
pub mod orientation;
pub mod value;

pub use action::{Action, BindingSpec, Direction};
pub use client::{Client, Error};
pub use color::Color;
pub use orientation::Orientation;
pub use value::BoolValue;

/// Outcome of a [`Client`] method. Re-exported at the crate root so
/// `gharial_ipc::Result` reads naturally in a config `main`.
pub use client::Result;

use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub const SOCKET_ENV: &str = "GHARIAL_SOCKET";
pub const SOCKET_BASENAME: &str = "gharial";

/// Resolves the IPC socket path.
///
/// Precedence:
///   1. `$GHARIAL_SOCKET` if set.
///   2. `$XDG_RUNTIME_DIR/gharial-<WAYLAND_DISPLAY>.sock` if both set.
///   3. `$XDG_RUNTIME_DIR/gharial.sock`.
///   4. `/tmp/gharial-<uid>.sock`.
pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os(SOCKET_ENV) {
        return PathBuf::from(p);
    }
    let basename = match std::env::var_os("WAYLAND_DISPLAY") {
        Some(d) if !d.is_empty() => {
            let mut s = OsString::from(SOCKET_BASENAME);
            s.push("-");
            s.push(&d);
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
        out.push_str(&self.command);
        for arg in &self.args {
            out.push(' ');
            push_token(&mut out, arg);
        }
        out.push('\n');
        out
    }

    /// Parse a line (without trailing newline) into a Request.
    pub fn parse(line: &str) -> std::result::Result<Self, ParseError> {
        let mut tokens = tokenize(line)?;
        if tokens.is_empty() {
            return Err(ParseError::Empty);
        }
        let command = tokens.remove(0);
        Ok(Self {
            command,
            args: tokens,
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

    /// Encode to a single line including the trailing `\n`. Embedded
    /// newlines in the body are replaced with `\n` literal characters
    /// (`\\n`) so the response always fits on one line.
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
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    c => out.push(c),
                }
            }
        }
        out.push('\n');
        out
    }

    /// Parse a line (without trailing newline) into a Response.
    pub fn parse(line: &str) -> std::result::Result<Self, ParseError> {
        let line = line.trim_end_matches(['\r', '\n']);
        let (tag, rest) = match line.find(' ') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, ""),
        };
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
        if c == '\\' {
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
        } else {
            out.push(c);
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
            .any(|c| c == ' ' || c == '\t' || c == '"' || c == '\\' || c == '\n');
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
            c => out.push(c),
        }
    }
    out.push('"');
}

fn tokenize(line: &str) -> std::result::Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut cur = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
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
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                cur.push(c);
                chars.next();
            }
        }
        out.push(cur);
    }
    Ok(out)
}

/// Send a single request to the daemon at `path` and read one response line.
pub fn send_one(path: &std::path::Path, req: &Request) -> io::Result<Response> {
    let stream = UnixStream::connect(path)?;
    let mut writer = stream.try_clone()?;
    writer.write_all(req.encode().as_bytes())?;
    writer.flush()?;
    drop(writer);
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no response"));
    }
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
        let parsed = Request::parse(encoded.trim_end()).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn request_roundtrip_quoted() {
        let r = Request::new(
            "bind",
            vec!["super+q".into(), "spawn rio -e nvim foo".into()],
        );
        let encoded = r.encode();
        let parsed = Request::parse(encoded.trim_end()).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn response_roundtrip() {
        let r = Response::ok("main-ratio=0.5;main-count=1");
        let encoded = r.encode();
        let parsed = Response::parse(&encoded).unwrap();
        assert_eq!(parsed, r);
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
        // `\"`, `\\`, `\n`, `\t` are escapes inside quoted strings.
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
        // Whitespace-only and empty both come back as Empty.
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
        // Bodies with embedded backslashes/quotes/control bytes must
        // come back unchanged via encode → parse.
        let bodies = [
            "plain text",
            "with \"quotes\" inside",
            "back\\slash\\here",
            "tab\there",
            "newline\nhere",
        ];
        for body in bodies {
            let r = Response::ok(body);
            let s = r.encode();
            let p = Response::parse(&s).unwrap_or_else(|e| panic!("{body:?}: {e}"));
            assert_eq!(p, r);
        }
    }

    #[test]
    fn request_encode_preserves_unicode() {
        let r = Request::new("spawn", vec!["tofi-drun".into(), "résumé".into()]);
        let encoded = r.encode();
        let parsed = Request::parse(encoded.trim_end()).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn empty_string_argument_is_quoted_through_a_round_trip() {
        let r = Request::new("set", vec!["key".into(), "".into()]);
        let encoded = r.encode();
        // The empty arg requires quoting so the parser sees it.
        assert!(encoded.contains("\"\""));
        let parsed = Request::parse(encoded.trim_end()).unwrap();
        assert_eq!(parsed, r);
    }
}
