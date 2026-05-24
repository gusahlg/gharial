//! Argument parsing helpers for gharialctl.
//!
//! These are pure functions — no I/O, no IPC. They turn the raw `argv`
//! slice into structured options that `main` can pattern-match against.

use std::path::PathBuf;
use std::time::Duration;

/// Extract `-s PATH` / `--socket PATH` from the front of the arg list.
///
/// Returns the optional socket override and the remaining args. Errors
/// only if `-s`/`--socket` is given without a following value.
pub fn split_socket_flag(args: Vec<String>) -> Result<(Option<PathBuf>, Vec<String>), String> {
    let mut iter = args.into_iter();
    let Some(first) = iter.next() else {
        return Ok((None, Vec::new()));
    };
    if first == "-s" || first == "--socket" {
        let path = iter.next().ok_or("--socket requires a path argument")?;
        Ok((Some(PathBuf::from(path)), iter.collect()))
    } else {
        let mut rest = Vec::with_capacity(iter.size_hint().0 + 1);
        rest.push(first);
        rest.extend(iter);
        Ok((None, rest))
    }
}

/// Parse a duration string like `2000`, `2000ms`, or `2s`. Bare numbers
/// are treated as milliseconds. Returns `None` if unparseable.
pub fn parse_timeout(arg: Option<&str>) -> Option<Duration> {
    let s = arg?;
    let (n, unit) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, "ms"),
    };
    let n: u64 = n.parse().ok()?;
    Some(match unit {
        "" | "ms" => Duration::from_millis(n),
        "s" => Duration::from_secs(n),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_socket_flag_passes_args_through() {
        let (sock, rest) = split_socket_flag(vec!["set".into(), "gaps".into()]).unwrap();
        assert_eq!(sock, None);
        assert_eq!(rest, vec!["set", "gaps"]);
    }

    #[test]
    fn short_and_long_socket_flag_consume_value() {
        let (sock, rest) =
            split_socket_flag(vec!["-s".into(), "/tmp/x.sock".into(), "ping".into()]).unwrap();
        assert_eq!(sock, Some(PathBuf::from("/tmp/x.sock")));
        assert_eq!(rest, vec!["ping"]);

        let (sock, rest) =
            split_socket_flag(vec!["--socket".into(), "/tmp/y.sock".into()]).unwrap();
        assert_eq!(sock, Some(PathBuf::from("/tmp/y.sock")));
        assert!(rest.is_empty());
    }

    #[test]
    fn socket_flag_without_value_errors() {
        let err = split_socket_flag(vec!["-s".into()]).unwrap_err();
        assert!(err.contains("--socket"));
    }

    #[test]
    fn timeout_defaults_to_milliseconds() {
        assert_eq!(parse_timeout(Some("250")), Some(Duration::from_millis(250)));
        assert_eq!(parse_timeout(Some("250ms")), Some(Duration::from_millis(250)));
        assert_eq!(parse_timeout(Some("3s")), Some(Duration::from_secs(3)));
        assert_eq!(parse_timeout(Some("3min")), None);
        assert_eq!(parse_timeout(None), None);
    }
}
