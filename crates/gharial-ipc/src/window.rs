//! Window selectors and per-window commands.
//!
//! Selector spellings:
//!
//! - `focused` — the keyboard-focused window
//! - `all` — every window
//! - `7` — gharial's window id (see `window list`; never reused while the
//!   daemon runs)
//! - `app-id=PATTERN`, `title=PATTERN`, `pid=PID` — a [`Matcher`]; several
//!   tokens combine (`app-id=foo title=bar`)

use std::fmt;

use crate::args::{Cursor, Style};
use crate::matcher::Matcher;
use crate::placement::Placement;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowSelector {
    Focused,
    All,
    Id(u64),
    Match(Matcher),
}

impl WindowSelector {
    /// Consume a selector from the front of `tokens`, returning it and
    /// the remaining tokens. Matcher selectors use the one-token
    /// `key=value` spelling so they can't be confused with the command
    /// words that follow.
    pub fn parse_prefix<'t, 'a>(tokens: &'t [&'a str]) -> Result<(Self, &'t [&'a str]), String> {
        let (&first, rest) = tokens
            .split_first()
            .ok_or("expected a window selector (focused|all|ID|app-id=…|title=…|pid=…)")?;
        match first {
            "focused" => return Ok((Self::Focused, rest)),
            "all" => return Ok((Self::All, rest)),
            _ => {}
        }
        if let Ok(id) = first.parse::<u64>() {
            return Ok((Self::Id(id), rest));
        }
        let joined = tokens
            .iter()
            .take_while(|t| t.contains('=') && !t.starts_with("--"))
            .count();
        let mut c = Cursor::new(&tokens[..joined], Style::Words);
        let mut m = Matcher::default();
        while !c.is_done() {
            if !m.parse_item(&mut c)? {
                break;
            }
        }
        let consumed = joined - c.rest().len();
        if m.is_empty() {
            return Err(format!(
                "invalid window selector {first:?} (expected focused|all|ID|app-id=…|title=…|pid=…)"
            ));
        }
        Ok((Self::Match(m), &tokens[consumed..]))
    }

    /// Parse a token list that is exactly one selector.
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let (sel, rest) = Self::parse_prefix(tokens)?;
        match rest.first() {
            None => Ok(sel),
            Some(extra) => Err(format!("unexpected token after selector: {extra}")),
        }
    }

    pub fn to_tokens(&self) -> Vec<String> {
        match self {
            Self::Focused => vec!["focused".into()],
            Self::All => vec!["all".into()],
            Self::Id(id) => vec![id.to_string()],
            Self::Match(m) => {
                let mut out = Vec::new();
                m.push_tokens(&mut out, Style::Words, true);
                out
            }
        }
    }
}

impl fmt::Display for WindowSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_tokens().join(" "))
    }
}

/// What a `window` command does to the selected windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowOp {
    /// Ask the windows to close.
    Close,
    /// Apply a placement (float, move to an output, set tags, focus, …).
    Place(Placement),
}

impl WindowOp {
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        match tokens {
            [] => Err("window: expected close or placement keys (float, output, tags, …)".into()),
            ["close"] => Ok(Self::Close),
            _ => {
                let p = Placement::parse(tokens)?;
                Ok(Self::Place(p))
            }
        }
    }

    pub fn to_tokens(&self) -> Vec<String> {
        match self {
            Self::Close => vec!["close".into()],
            Self::Place(p) => p.to_tokens(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::BoolValue;

    #[test]
    fn simple_selectors() {
        assert_eq!(
            WindowSelector::parse(&["focused"]).unwrap(),
            WindowSelector::Focused
        );
        assert_eq!(
            WindowSelector::parse(&["all"]).unwrap(),
            WindowSelector::All
        );
        assert_eq!(
            WindowSelector::parse(&["42"]).unwrap(),
            WindowSelector::Id(42)
        );
    }

    #[test]
    fn matcher_selectors_stop_at_command_words() {
        let (sel, rest) =
            WindowSelector::parse_prefix(&["app-id=wattbench-*", "title=x", "float", "size=1x1"])
                .unwrap();
        assert_eq!(
            sel,
            WindowSelector::Match(Matcher {
                app_id: Some("wattbench-*".into()),
                title: Some("x".into()),
                pid: None,
            })
        );
        // `size=1x1` is a placement key, not a matcher key.
        assert_eq!(rest, &["float", "size=1x1"]);
    }

    #[test]
    fn bad_selectors_error() {
        assert!(WindowSelector::parse(&[]).is_err());
        assert!(WindowSelector::parse(&["float"]).is_err());
        assert!(WindowSelector::parse(&["size=1x1"]).is_err());
        assert!(WindowSelector::parse(&["focused", "extra"]).is_err());
    }

    #[test]
    fn selectors_round_trip() {
        for sel in [
            WindowSelector::Focused,
            WindowSelector::All,
            WindowSelector::Id(7),
            WindowSelector::Match(Matcher::pid(99)),
            WindowSelector::Match(Matcher {
                app_id: Some("a b".into()),
                title: Some("t".into()),
                pid: Some(3),
            }),
        ] {
            let toks = sel.to_tokens();
            let refs: Vec<&str> = toks.iter().map(String::as_str).collect();
            assert_eq!(WindowSelector::parse(&refs).unwrap(), sel, "{toks:?}");
        }
    }

    #[test]
    fn ops_parse() {
        assert_eq!(WindowOp::parse(&["close"]).unwrap(), WindowOp::Close);
        match WindowOp::parse(&["sticky", "toggle"]).unwrap() {
            WindowOp::Place(p) => assert_eq!(p.sticky, Some(BoolValue::Toggle)),
            other => panic!("{other:?}"),
        }
        assert!(WindowOp::parse(&[]).is_err());
        assert!(WindowOp::parse(&["close", "now"]).is_err());
    }
}
