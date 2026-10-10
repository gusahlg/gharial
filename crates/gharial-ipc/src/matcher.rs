//! Window matchers: which windows a rule, selector, or wait applies to.
//!
//! A matcher is a conjunction of up to three constraints:
//!
//! - `app-id=PATTERN` — glob over the window's app_id (`*` any run,
//!   `?` one character; no wildcard means an exact match)
//! - `title=PATTERN` — same, over the title
//! - `pid=PID` — the window's process is `PID`, a descendant of it, or
//!   in the session it leads (the daemon resolves this via `/proc`)

use crate::args::{push_pair, Cursor, Style};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matcher {
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub pid: Option<u32>,
}

impl Matcher {
    pub fn app_id(pattern: impl Into<String>) -> Self {
        Self {
            app_id: Some(pattern.into()),
            ..Self::default()
        }
    }

    pub fn title(pattern: impl Into<String>) -> Self {
        Self {
            title: Some(pattern.into()),
            ..Self::default()
        }
    }

    pub fn pid(pid: u32) -> Self {
        Self {
            pid: Some(pid),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.app_id.is_none() && self.title.is_none() && self.pid.is_none()
    }

    /// The text half of the match. A pattern against an unset field
    /// never matches. The `pid` constraint needs process ancestry and is
    /// checked by the daemon.
    pub fn matches_text(&self, app_id: Option<&str>, title: Option<&str>) -> bool {
        let field = |pattern: &Option<String>, value: Option<&str>| match pattern {
            None => true,
            Some(p) => value.is_some_and(|v| glob_match(p, v)),
        };
        field(&self.app_id, app_id) && field(&self.title, title)
    }

    /// Try to consume one matcher constraint at the cursor. Returns
    /// `Ok(false)` without consuming when the next token is not one.
    pub(crate) fn parse_item(&mut self, c: &mut Cursor<'_, '_>) -> Result<bool, String> {
        let Some(key) = c.peek_key() else {
            return Ok(false);
        };
        match key.name {
            "app-id" | "app_id" | "appid" => {
                c.advance();
                self.app_id = Some(c.value(key)?.to_string());
            }
            "title" => {
                c.advance();
                self.title = Some(c.value(key)?.to_string());
            }
            "pid" => {
                c.advance();
                let raw = c.value(key)?;
                let pid: u32 = raw
                    .parse()
                    .map_err(|_| format!("pid: invalid pid {raw:?}"))?;
                if pid == 0 {
                    return Err("pid: must be positive".into());
                }
                self.pid = Some(pid);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Parse a whole token list as a matcher (`app-id=foo title=bar`).
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let mut c = Cursor::new(tokens, Style::Words);
        let mut m = Self::default();
        while !c.is_done() {
            if !m.parse_item(&mut c)? {
                return Err(format!("unknown matcher: {}", c.rest()[0]));
            }
        }
        if m.is_empty() {
            return Err("empty matcher (expected app-id=, title=, or pid=)".into());
        }
        Ok(m)
    }

    /// Canonical `key=value` tokens. Round-trips with [`Matcher::parse`].
    pub fn to_tokens(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.push_tokens(&mut out, Style::Words, true);
        out
    }

    /// Push constraints in `style`. `joined` forces the one-token
    /// `key=value` spelling (selectors and listings want that).
    pub(crate) fn push_tokens(&self, out: &mut Vec<String>, style: Style, joined: bool) {
        let mut pair = |key: &str, value: &str| {
            if joined && style == Style::Words {
                out.push(format!("{key}={value}"));
            } else {
                push_pair(out, style, key, value);
            }
        };
        if let Some(p) = &self.app_id {
            pair("app-id", p);
        }
        if let Some(p) = &self.title {
            pair("title", p);
        }
        if let Some(pid) = self.pid {
            pair("pid", &pid.to_string());
        }
    }
}

/// Shell-style glob: `*` matches any run (including empty), `?` exactly
/// one character, everything else literally. Linear-time two-pointer
/// backtracking, so hostile titles can't blow up the matcher.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    // Position after the most recent `*`, and the text index it matched up to.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi + 1, ti));
                pi += 1;
            }
            Some('?') => {
                pi += 1;
                ti += 1;
            }
            Some(&c) if c == t[ti] => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    pi = sp;
                    ti = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_basics() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "foobar"));
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("*bar", "foobar"));
        assert!(glob_match("f*o*r", "foobar"));
        assert!(glob_match("wattbench-*", "wattbench-42"));
        assert!(!glob_match("wattbench-*", "wattbench"));
        assert!(glob_match("?oo", "zoo"));
        assert!(!glob_match("?oo", "oo"));
        assert!(glob_match("*", ""));
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
        assert!(glob_match("résumé*", "résumé.pdf"));
    }

    #[test]
    fn glob_handles_pathological_input_quickly() {
        let text = "a".repeat(10_000);
        let pattern = format!("{}b", "*a".repeat(50));
        assert!(!glob_match(&pattern, &text));
    }

    #[test]
    fn matcher_text_requires_every_constraint() {
        let m = Matcher {
            app_id: Some("foo*".into()),
            title: Some("bar".into()),
            pid: None,
        };
        assert!(m.matches_text(Some("foot"), Some("bar")));
        assert!(!m.matches_text(Some("foot"), Some("baz")));
        assert!(!m.matches_text(None, Some("bar")));
        assert!(Matcher::default().matches_text(None, None));
    }

    #[test]
    fn parse_accepts_word_and_flag_spellings() {
        let a = Matcher::parse(&["app-id=foo", "--title", "x y", "pid", "42"]).unwrap();
        assert_eq!(
            a,
            Matcher {
                app_id: Some("foo".into()),
                title: Some("x y".into()),
                pid: Some(42),
            }
        );
        assert!(Matcher::parse(&[]).is_err());
        assert!(Matcher::parse(&["float"]).is_err());
        assert!(Matcher::parse(&["pid=0"]).is_err());
        assert!(Matcher::parse(&["pid=abc"]).is_err());
    }

    #[test]
    fn tokens_round_trip() {
        let m = Matcher {
            app_id: Some("wattbench-*".into()),
            title: Some("a title".into()),
            pid: Some(7),
        };
        let toks = m.to_tokens();
        assert_eq!(toks, ["app-id=wattbench-*", "title=a title", "pid=7"]);
        let refs: Vec<&str> = toks.iter().map(String::as_str).collect();
        assert_eq!(Matcher::parse(&refs).unwrap(), m);
    }
}
