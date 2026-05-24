//! Minimal xkbcommon-compatible keysym table.
//!
//! River's xkb-bindings protocol takes a raw 32-bit keysym integer. Rather
//! than pull in xkbcommon (large C dep, FFI surface), we hand-roll the
//! mappings for the ~200 keys a daily-driver config touches and provide a
//! `0x1234` hex fallback for anything exotic. ASCII letters and digits
//! resolve from their char codes — no table entries needed.

/// Parse a key name into an xkbcommon keysym. Returns `None` if unknown.
pub fn parse_keysym(s: &str) -> Option<u32> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok();
    }
    // ASCII letters/digits map to their codepoint.
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        if c.is_ascii_alphanumeric() {
            return Some(c.to_ascii_lowercase() as u32);
        }
        // A handful of printable ASCII punctuation that users frequently
        // bind. xkbcommon's keysyms for these match their ASCII values.
        if matches!(c,
            ' ' | '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')'
            | '*' | '+' | ',' | '-' | '.' | '/' | ':' | ';' | '<' | '='
            | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '_' | '`'
            | '{' | '|' | '}' | '~'
        ) {
            return Some(c as u32);
        }
    }
    NAMED.iter().find_map(|(name, sym)| {
        if name.eq_ignore_ascii_case(s) {
            Some(*sym)
        } else {
            None
        }
    })
}

/// Hand-curated subset of xkbcommon keysyms. Names are matched case-
/// insensitively. Add to this list rather than reaching for xkbcommon.
const NAMED: &[(&str, u32)] = &[
    // Whitespace / editing
    ("space", 0x0020),
    ("return", 0xff0d),
    ("enter", 0xff0d),
    ("tab", 0xff09),
    ("backspace", 0xff08),
    ("bspc", 0xff08),
    ("delete", 0xffff),
    ("del", 0xffff),
    ("escape", 0xff1b),
    ("esc", 0xff1b),
    ("insert", 0xff63),
    ("home", 0xff50),
    ("end", 0xff57),
    ("page_up", 0xff55),
    ("pageup", 0xff55),
    ("pgup", 0xff55),
    ("page_down", 0xff56),
    ("pagedown", 0xff56),
    ("pgdn", 0xff56),
    ("menu", 0xff67),
    ("print", 0xff61),
    ("pause", 0xff13),
    // Arrows
    ("left", 0xff51),
    ("up", 0xff52),
    ("right", 0xff53),
    ("down", 0xff54),
    // F keys
    ("f1", 0xffbe), ("f2", 0xffbf), ("f3", 0xffc0), ("f4", 0xffc1),
    ("f5", 0xffc2), ("f6", 0xffc3), ("f7", 0xffc4), ("f8", 0xffc5),
    ("f9", 0xffc6), ("f10", 0xffc7), ("f11", 0xffc8), ("f12", 0xffc9),
    ("f13", 0xffca), ("f14", 0xffcb), ("f15", 0xffcc), ("f16", 0xffcd),
    ("f17", 0xffce), ("f18", 0xffcf), ("f19", 0xffd0), ("f20", 0xffd1),
    // Numpad
    ("kp_0", 0xffb0), ("kp_1", 0xffb1), ("kp_2", 0xffb2), ("kp_3", 0xffb3),
    ("kp_4", 0xffb4), ("kp_5", 0xffb5), ("kp_6", 0xffb6), ("kp_7", 0xffb7),
    ("kp_8", 0xffb8), ("kp_9", 0xffb9),
    ("kp_add", 0xffab), ("kp_subtract", 0xffad), ("kp_multiply", 0xffaa),
    ("kp_divide", 0xffaf), ("kp_enter", 0xff8d), ("kp_decimal", 0xffae),
    // XF86 media + brightness — handy for laptop config scripts.
    ("xf86audioraisevolume", 0x1008ff13),
    ("xf86audiolowervolume", 0x1008ff11),
    ("xf86audiomute", 0x1008ff12),
    ("xf86audiomicmute", 0x1008ffb2),
    ("xf86audioplay", 0x1008ff14),
    ("xf86audiopause", 0x1008ff31),
    ("xf86audionext", 0x1008ff17),
    ("xf86audioprev", 0x1008ff16),
    ("xf86audiostop", 0x1008ff15),
    ("xf86monbrightnessup", 0x1008ff02),
    ("xf86monbrightnessdown", 0x1008ff03),
    ("xf86kbdbrightnessup", 0x1008ff05),
    ("xf86kbdbrightnessdown", 0x1008ff06),
    ("xf86display", 0x1008ff59),
    ("xf86wlan", 0x1008ff95),
    ("xf86touchpadtoggle", 0x1008ffa9),
    ("xf86search", 0x1008ff1b),
    ("xf86mail", 0x1008ff19),
    ("xf86launch1", 0x1008ff41),
    ("xf86launch2", 0x1008ff42),
];

/// Parse a modifier name into its `river_seat_v1::modifiers` bit value.
pub fn parse_modifier(s: &str) -> Result<u32, String> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "shift" => 1,
        "ctrl" | "control" => 4,
        "alt" | "mod1" => 8,
        "mod3" => 32,
        "super" | "mod4" | "logo" | "win" | "meta" => 64,
        "mod5" => 128,
        other => return Err(format!("unknown modifier: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_letters_resolve() {
        assert_eq!(parse_keysym("a"), Some(0x61));
        assert_eq!(parse_keysym("Q"), Some(0x71));
        assert_eq!(parse_keysym("z"), Some(0x7a));
    }

    #[test]
    fn ascii_digits_resolve() {
        assert_eq!(parse_keysym("0"), Some(0x30));
        assert_eq!(parse_keysym("9"), Some(0x39));
    }

    #[test]
    fn named_keys_case_insensitive() {
        assert_eq!(parse_keysym("Return"), Some(0xff0d));
        assert_eq!(parse_keysym("enter"), Some(0xff0d));
        assert_eq!(parse_keysym("ESCAPE"), Some(0xff1b));
        assert_eq!(parse_keysym("Left"), Some(0xff51));
    }

    #[test]
    fn function_keys() {
        assert_eq!(parse_keysym("F1"), Some(0xffbe));
        assert_eq!(parse_keysym("f12"), Some(0xffc9));
    }

    #[test]
    fn xf86_media_keys() {
        assert_eq!(parse_keysym("XF86AudioRaiseVolume"), Some(0x1008ff13));
        assert_eq!(parse_keysym("xf86monbrightnessup"), Some(0x1008ff02));
    }

    #[test]
    fn hex_fallback() {
        assert_eq!(parse_keysym("0xff0d"), Some(0xff0d));
        assert_eq!(parse_keysym("0x1008ff13"), Some(0x1008ff13));
        assert_eq!(parse_keysym("0xnotahex"), None);
    }

    #[test]
    fn unknown_keysym_is_none() {
        assert_eq!(parse_keysym("PaperJam"), None);
    }

    #[test]
    fn modifier_aliases() {
        assert_eq!(parse_modifier("super").unwrap(), 64);
        assert_eq!(parse_modifier("Super").unwrap(), 64);
        assert_eq!(parse_modifier("mod4").unwrap(), 64);
        assert_eq!(parse_modifier("logo").unwrap(), 64);
        assert_eq!(parse_modifier("ctrl").unwrap(), 4);
        assert_eq!(parse_modifier("Control").unwrap(), 4);
        assert_eq!(parse_modifier("alt").unwrap(), 8);
        assert!(parse_modifier("hyper").is_err());
    }

    #[test]
    fn printable_punctuation() {
        assert_eq!(parse_keysym(","), Some(b',' as u32));
        assert_eq!(parse_keysym("/"), Some(b'/' as u32));
        assert_eq!(parse_keysym(";"), Some(b';' as u32));
    }
}
