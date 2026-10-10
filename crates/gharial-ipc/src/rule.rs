//! Window rules: a matcher plus the placement applied when a matching
//! window first appears.
//!
//! Grammar (`rule add …`):
//!
//! ```text
//! MATCHER... PLACEMENT... [oneshot]
//! ```
//!
//! e.g. `app-id=wattbench-* output DP-2 tags 9 float size 1280x720 nofocus lock`.
//! Every matching rule applies, in the order they were added; a later
//! rule's fields override an earlier one's. A `oneshot` rule removes
//! itself after its first match. Lifetime (`until-exit PID`) is an IPC
//! option of `rule add`, not part of the rule itself; a rule with a
//! `pid=` matcher always ends when that process exits.

use crate::args::{key_token, Cursor, Style};
use crate::matcher::Matcher;
use crate::placement::Placement;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    pub matcher: Matcher,
    pub placement: Placement,
    pub oneshot: bool,
}

impl Rule {
    pub fn new(matcher: Matcher, placement: Placement) -> Self {
        Self {
            matcher,
            placement,
            oneshot: false,
        }
    }

    pub fn oneshot(mut self) -> Self {
        self.oneshot = true;
        self
    }

    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let mut c = Cursor::new(tokens, Style::Words);
        let mut rule = Self::default();
        while !c.is_done() {
            if rule.matcher.parse_item(&mut c)? || rule.placement.parse_item(&mut c)? {
                continue;
            }
            match c.peek_key() {
                Some(k) if k.name == "oneshot" && k.inline.is_none() => {
                    c.advance();
                    rule.oneshot = true;
                }
                _ => return Err(format!("rule: unknown key {}", c.rest()[0])),
            }
        }
        if rule.matcher.is_empty() {
            return Err("rule: needs at least one matcher (app-id=, title=, pid=)".into());
        }
        if rule.placement.is_empty() {
            return Err("rule: needs at least one placement key (float, output, tags, …)".into());
        }
        Ok(rule)
    }

    /// Canonical tokens: matchers, placement, flags. Round-trips with
    /// [`Rule::parse`].
    pub fn to_tokens(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.matcher.push_tokens(&mut out, Style::Words, true);
        self.placement.push_tokens(&mut out, Style::Words);
        if self.oneshot {
            out.push(key_token(Style::Words, "oneshot"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::TagSet;

    #[test]
    fn parses_matchers_and_placement_in_any_order() {
        let r = Rule::parse(&["float", "app-id=foo", "--output", "DP-2", "oneshot"]).unwrap();
        assert_eq!(r.matcher, Matcher::app_id("foo"));
        assert_eq!(
            r.placement,
            Placement::new().floating(true).on_output("DP-2")
        );
        assert!(r.oneshot);
    }

    #[test]
    fn requires_both_halves() {
        assert!(Rule::parse(&["app-id=foo"]).is_err());
        assert!(Rule::parse(&["float"]).is_err());
        assert!(Rule::parse(&["app-id=foo", "float", "bogus"]).is_err());
    }

    #[test]
    fn round_trips() {
        let r = Rule::new(
            Matcher {
                app_id: Some("wattbench-*".into()),
                title: None,
                pid: Some(4242),
            },
            Placement::new()
                .on_output("DP-2")
                .tags(TagSet::single(9).unwrap())
                .fullscreen(true)
                .focus(false)
                .locked(true),
        )
        .oneshot();
        let toks = r.to_tokens();
        let refs: Vec<&str> = toks.iter().map(String::as_str).collect();
        assert_eq!(Rule::parse(&refs).unwrap(), r, "{toks:?}");
    }
}
