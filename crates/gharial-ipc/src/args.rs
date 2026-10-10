//! Shared token cursor for the key/value grammars (window matchers,
//! placements, rules, spawn flags).
//!
//! Every key is accepted in four spellings — `key value`, `key=value`,
//! `--key value`, `--key=value` — so a shell script can use whichever
//! reads best. Boolean keys take an optional `on|off|toggle` value.
//!
//! [`Style::Flags`] is the stricter form used by `spawn`, where the
//! first non-flag token starts the command line: keys must carry the
//! `--` prefix and booleans only take a value inline (`--float=off`),
//! so `spawn --float on` can never swallow a program called `on`.

use crate::value::BoolValue;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Words,
    Flags,
}

/// A token split into its key and optional inline (`=`) value.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Key<'a> {
    pub name: &'a str,
    pub inline: Option<&'a str>,
}

pub(crate) struct Cursor<'t, 'a> {
    tokens: &'t [&'a str],
    pos: usize,
    style: Style,
}

impl<'t, 'a> Cursor<'t, 'a> {
    pub fn new(tokens: &'t [&'a str], style: Style) -> Self {
        Self {
            tokens,
            pos: 0,
            style,
        }
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn rest(&self) -> &'t [&'a str] {
        &self.tokens[self.pos.min(self.tokens.len())..]
    }

    /// The key at the cursor, without consuming it. `None` at the end,
    /// or (in flag style) when the token isn't a `--flag`.
    pub fn peek_key(&self) -> Option<Key<'a>> {
        let tok = *self.tokens.get(self.pos)?;
        let body = match (self.style, tok.strip_prefix("--")) {
            (_, Some(body)) if !body.is_empty() => body,
            (Style::Words, None) => tok,
            _ => return None,
        };
        Some(match body.split_once('=') {
            Some((name, value)) => Key {
                name,
                inline: Some(value),
            },
            None => Key {
                name: body,
                inline: None,
            },
        })
    }

    /// Consume the key token the last `peek_key` returned.
    pub fn advance(&mut self) {
        self.pos += 1;
    }

    /// The value for `key`: its inline value, or the next token.
    pub fn value(&mut self, key: Key<'a>) -> Result<&'a str, String> {
        if let Some(v) = key.inline {
            return Ok(v);
        }
        let v = self
            .tokens
            .get(self.pos)
            .copied()
            .ok_or_else(|| format!("{}: missing value", key.name))?;
        self.pos += 1;
        Ok(v)
    }

    /// Optional boolean for `key`: inline value, or (word style only) a
    /// following `on|off|toggle`. Defaults to `On`.
    pub fn bool_value(&mut self, key: Key<'a>) -> Result<BoolValue, String> {
        if let Some(v) = key.inline {
            return BoolValue::parse(v).map_err(|e| format!("{}: {e}", key.name));
        }
        if self.style == Style::Words {
            if let Some(Ok(b)) = self.tokens.get(self.pos).map(|t| BoolValue::parse(t)) {
                self.pos += 1;
                return Ok(b);
            }
        }
        Ok(BoolValue::On)
    }
}

/// Encode `key` for the given style: `--key` / `key`.
pub(crate) fn key_token(style: Style, key: &str) -> String {
    match style {
        Style::Words => key.to_string(),
        Style::Flags => format!("--{key}"),
    }
}

/// Encode a key/value pair so it re-parses identically in either style.
pub(crate) fn push_pair(out: &mut Vec<String>, style: Style, key: &str, value: &str) {
    match style {
        Style::Words => {
            out.push(key.to_string());
            out.push(value.to_string());
        }
        Style::Flags => out.push(format!("--{key}={value}")),
    }
}

/// Encode a boolean key: bare for `On`, `key=off|toggle` otherwise.
pub(crate) fn push_bool(out: &mut Vec<String>, style: Style, key: &str, value: BoolValue) {
    match value {
        BoolValue::On => out.push(key_token(style, key)),
        other => out.push(format!("{}={}", key_token(style, key), other.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_accept_all_four_spellings() {
        for toks in [
            &["output", "DP-2"][..],
            &["output=DP-2"][..],
            &["--output", "DP-2"][..],
            &["--output=DP-2"][..],
        ] {
            let mut c = Cursor::new(toks, Style::Words);
            let key = c.peek_key().unwrap();
            assert_eq!(key.name, "output");
            c.advance();
            assert_eq!(c.value(key).unwrap(), "DP-2");
            assert!(c.is_done());
        }
    }

    #[test]
    fn flags_stop_at_the_first_bare_token() {
        let c = Cursor::new(&["rio", "--float"], Style::Flags);
        assert!(c.peek_key().is_none());
        let c = Cursor::new(&["--", "rio"], Style::Flags);
        assert!(c.peek_key().is_none(), "`--` alone ends the flags");
    }

    #[test]
    fn bool_lookahead_only_in_word_style() {
        let mut c = Cursor::new(&["float", "off"], Style::Words);
        let k = c.peek_key().unwrap();
        c.advance();
        assert_eq!(c.bool_value(k).unwrap(), BoolValue::Off);
        assert!(c.is_done());

        let mut c = Cursor::new(&["--float", "off"], Style::Flags);
        let k = c.peek_key().unwrap();
        c.advance();
        assert_eq!(c.bool_value(k).unwrap(), BoolValue::On);
        assert_eq!(c.rest(), &["off"]);

        let mut c = Cursor::new(&["--float=toggle"], Style::Flags);
        let k = c.peek_key().unwrap();
        c.advance();
        assert_eq!(c.bool_value(k).unwrap(), BoolValue::Toggle);
    }
}
