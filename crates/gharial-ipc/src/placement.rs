//! Placement: where a window goes and how gharial treats it.
//!
//! One vocabulary serves three surfaces:
//!
//! - **rules** (`rule add app-id=foo float size 1280x720`) apply it when a
//!   matching window first appears;
//! - **spawn** (`spawn --output DP-2 --fullscreen -- game`) applies it to
//!   every window the spawned process tree opens;
//! - **window commands** (`window 7 output HDMI-A-1 sticky`) apply it to
//!   existing windows.
//!
//! Keys (any of `key value`, `key=value`, `--key value`, `--key=value`):
//!
//! | key                      | effect                                              |
//! |--------------------------|-----------------------------------------------------|
//! | `output NAME`            | move to an output (connector name, index, or dir)  |
//! | `tags TAGS`              | tag membership (`9`, `1,3`, `all`)                 |
//! | `float [B]` / `tile`     | floating vs tiled                                  |
//! | `fullscreen [B]`         | fullscreen on its output                           |
//! | `sticky [B]`             | visible on every tag of its output                 |
//! | `lock [B]`               | geometry frozen: never resized/moved by policy     |
//! | `border [B]`/`noborder`  | draw the border                                    |
//! | `focus [B]`/`nofocus`    | take keyboard focus (on map, or now)               |
//! | `size WxH`               | floating size (implies `float`)                    |
//! | `position X,Y`/`center`  | floating position relative to the output          |
//! | `geometry WxH+X+Y`       | size and position together                         |
//!
//! `[B]` is an optional `on|off|toggle` (default `on`).

use crate::args::{key_token, push_bool, push_pair, Cursor, Style};
use crate::output::OutputTarget;
use crate::tags::TagSet;
use crate::value::BoolValue;

/// Largest width/height/offset accepted. Wayland coordinates are `i32`;
/// this keeps arithmetic on them far from overflow.
pub const MAX_EXTENT: u32 = 1 << 16;

/// Where a floating window sits on its output.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FloatPos {
    /// Centred in the output's usable area.
    Center,
    /// Content top-left, relative to the output's top-left corner.
    At(i32, i32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    pub output: Option<OutputTarget>,
    pub tags: Option<TagSet>,
    pub floating: Option<BoolValue>,
    pub fullscreen: Option<BoolValue>,
    pub sticky: Option<BoolValue>,
    pub locked: Option<BoolValue>,
    pub border: Option<BoolValue>,
    pub focus: Option<BoolValue>,
    /// Content size of a floating window.
    pub size: Option<(u32, u32)>,
    pub position: Option<FloatPos>,
}

impl Placement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    // ── Builder helpers ───────────────────────────────────────────────

    pub fn on_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(OutputTarget::parse(&output.into()));
        self
    }
    pub fn tags(mut self, tags: TagSet) -> Self {
        self.tags = Some(tags);
        self
    }
    pub fn floating(mut self, on: bool) -> Self {
        self.floating = Some(on.into());
        self
    }
    pub fn fullscreen(mut self, on: bool) -> Self {
        self.fullscreen = Some(on.into());
        self
    }
    pub fn sticky(mut self, on: bool) -> Self {
        self.sticky = Some(on.into());
        self
    }
    pub fn locked(mut self, on: bool) -> Self {
        self.locked = Some(on.into());
        self
    }
    pub fn border(mut self, on: bool) -> Self {
        self.border = Some(on.into());
        self
    }
    pub fn focus(mut self, on: bool) -> Self {
        self.focus = Some(on.into());
        self
    }
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Some((width, height));
        self
    }
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some(FloatPos::At(x, y));
        self
    }
    pub fn centered(mut self) -> Self {
        self.position = Some(FloatPos::Center);
        self
    }

    /// Fold `other` over `self`: every field `other` sets wins. Rules
    /// matching the same window merge in insertion order this way.
    pub fn merge(&mut self, other: &Placement) {
        macro_rules! take {
            ($($field:ident),*) => {$(
                if other.$field.is_some() {
                    self.$field = other.$field.clone();
                }
            )*};
        }
        take!(output, tags, floating, fullscreen, sticky, locked, border, focus, size, position);
    }

    // ── Parsing ───────────────────────────────────────────────────────

    /// Try to consume one placement key at the cursor. Returns
    /// `Ok(false)` without consuming when the next token isn't one.
    pub(crate) fn parse_item(&mut self, c: &mut Cursor<'_, '_>) -> Result<bool, String> {
        let Some(key) = c.peek_key() else {
            return Ok(false);
        };
        match key.name {
            "output" => {
                c.advance();
                self.output = Some(OutputTarget::parse(c.value(key)?));
            }
            "tags" | "tag" => {
                c.advance();
                self.tags = Some(TagSet::parse(c.value(key)?)?);
            }
            "float" | "floating" => {
                c.advance();
                self.floating = Some(c.bool_value(key)?);
            }
            "tile" | "tiled" => {
                c.advance();
                self.floating = Some(BoolValue::Off);
            }
            "fullscreen" => {
                c.advance();
                self.fullscreen = Some(c.bool_value(key)?);
            }
            "sticky" => {
                c.advance();
                self.sticky = Some(c.bool_value(key)?);
            }
            "lock" | "locked" => {
                c.advance();
                self.locked = Some(c.bool_value(key)?);
            }
            "border" => {
                c.advance();
                self.border = Some(c.bool_value(key)?);
            }
            "noborder" => {
                c.advance();
                self.border = Some(BoolValue::Off);
            }
            "focus" => {
                c.advance();
                self.focus = Some(c.bool_value(key)?);
            }
            "nofocus" => {
                c.advance();
                self.focus = Some(BoolValue::Off);
            }
            "size" => {
                c.advance();
                self.size = Some(parse_size(c.value(key)?)?);
            }
            "position" | "pos" => {
                c.advance();
                self.position = Some(parse_position(c.value(key)?)?);
            }
            "center" | "centre" => {
                c.advance();
                self.position = Some(FloatPos::Center);
            }
            "geometry" | "geom" => {
                c.advance();
                let (size, pos) = parse_geometry(c.value(key)?)?;
                self.size = Some(size);
                self.position = Some(pos);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Parse a whole token list (word style) as a placement.
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let mut c = Cursor::new(tokens, Style::Words);
        let mut p = Self::default();
        while !c.is_done() {
            if !p.parse_item(&mut c)? {
                return Err(format!("unknown placement key: {}", c.rest()[0]));
            }
        }
        Ok(p)
    }

    /// Canonical word-style tokens. Round-trips with [`Placement::parse`].
    pub fn to_tokens(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.push_tokens(&mut out, Style::Words);
        out
    }

    /// Canonical `--flag` tokens, as `spawn` takes them.
    pub fn to_flags(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.push_tokens(&mut out, Style::Flags);
        out
    }

    pub(crate) fn push_tokens(&self, out: &mut Vec<String>, style: Style) {
        if let Some(o) = &self.output {
            push_pair(out, style, "output", &o.to_token());
        }
        if let Some(t) = self.tags {
            push_pair(out, style, "tags", &t.to_token());
        }
        let bools = [
            ("float", self.floating),
            ("fullscreen", self.fullscreen),
            ("sticky", self.sticky),
            ("lock", self.locked),
            ("border", self.border),
            ("focus", self.focus),
        ];
        for (key, value) in bools {
            if let Some(v) = value {
                push_bool(out, style, key, v);
            }
        }
        match (self.size, self.position) {
            (Some((w, h)), Some(FloatPos::At(x, y))) => {
                push_pair(out, style, "geometry", &format!("{w}x{h}{x:+}{y:+}"));
            }
            (size, pos) => {
                if let Some((w, h)) = size {
                    push_pair(out, style, "size", &format!("{w}x{h}"));
                }
                match pos {
                    Some(FloatPos::Center) => out.push(key_token(style, "center")),
                    Some(FloatPos::At(x, y)) => {
                        push_pair(out, style, "position", &format!("{x},{y}"));
                    }
                    None => {}
                }
            }
        }
    }
}

fn parse_extent(s: &str, what: &str) -> Result<u32, String> {
    let v: u32 = s.parse().map_err(|_| format!("invalid {what} {s:?}"))?;
    if v == 0 || v > MAX_EXTENT {
        return Err(format!("{what} {v} out of range 1..={MAX_EXTENT}"));
    }
    Ok(v)
}

fn parse_offset(s: &str) -> Result<i32, String> {
    let v: i32 = s
        .strip_prefix('+')
        .unwrap_or(s)
        .parse()
        .map_err(|_| format!("invalid offset {s:?}"))?;
    if v.unsigned_abs() > MAX_EXTENT {
        return Err(format!("offset {v} out of range ±{MAX_EXTENT}"));
    }
    Ok(v)
}

/// `WxH` (also `W,H`).
pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X', ','])
        .ok_or_else(|| format!("invalid size {s:?} (expected WIDTHxHEIGHT)"))?;
    Ok((parse_extent(w, "width")?, parse_extent(h, "height")?))
}

/// `X,Y` or `center`.
pub fn parse_position(s: &str) -> Result<FloatPos, String> {
    if matches!(s, "center" | "centre") {
        return Ok(FloatPos::Center);
    }
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("invalid position {s:?} (expected X,Y or center)"))?;
    Ok(FloatPos::At(parse_offset(x)?, parse_offset(y)?))
}

/// X11-style `WxH+X+Y`; each offset carries its own sign (`+10-20`).
pub fn parse_geometry(s: &str) -> Result<((u32, u32), FloatPos), String> {
    let bad = || format!("invalid geometry {s:?} (expected WIDTHxHEIGHT+X+Y)");
    let sign_at = |from: usize| s[from..].find(['+', '-']).map(|i| i + from);
    let first = sign_at(0).ok_or_else(bad)?;
    let second = sign_at(first + 1).ok_or_else(bad)?;
    let size = parse_size(&s[..first])?;
    let x = parse_offset(&s[first..second])?;
    let y = parse_offset(&s[second..])?;
    Ok((size, FloatPos::At(x, y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(tokens: &[&str]) -> Placement {
        Placement::parse(tokens).unwrap_or_else(|e| panic!("{tokens:?}: {e}"))
    }

    #[test]
    fn parses_the_bench_rule_shape() {
        let p = parse(&[
            "output", "DP-2", "tags", "9", "float", "size", "1280x720", "nofocus", "lock",
        ]);
        assert_eq!(p.output, Some(OutputTarget::Named("DP-2".into())));
        assert_eq!(p.tags, TagSet::single(9));
        assert_eq!(p.floating, Some(BoolValue::On));
        assert_eq!(p.size, Some((1280, 720)));
        assert_eq!(p.focus, Some(BoolValue::Off));
        assert_eq!(p.locked, Some(BoolValue::On));
    }

    #[test]
    fn bools_take_optional_values() {
        assert_eq!(parse(&["sticky"]).sticky, Some(BoolValue::On));
        assert_eq!(parse(&["sticky", "off"]).sticky, Some(BoolValue::Off));
        assert_eq!(parse(&["sticky=toggle"]).sticky, Some(BoolValue::Toggle));
        assert_eq!(parse(&["tile"]).floating, Some(BoolValue::Off));
        assert_eq!(parse(&["noborder"]).border, Some(BoolValue::Off));
    }

    #[test]
    fn geometry_forms() {
        assert_eq!(
            parse_geometry("1280x720+0+0").unwrap(),
            ((1280, 720), FloatPos::At(0, 0))
        );
        assert_eq!(
            parse_geometry("800x600+10-20").unwrap(),
            ((800, 600), FloatPos::At(10, -20))
        );
        assert_eq!(
            parse_geometry("800x600-10+20").unwrap(),
            ((800, 600), FloatPos::At(-10, 20))
        );
        for bad in ["800x600", "800x600+1", "x+1+1", "0x10+0+0", "10x10+a+0"] {
            assert!(parse_geometry(bad).is_err(), "{bad}");
        }
        assert_eq!(parse_size("1920,1080").unwrap(), (1920, 1080));
        assert!(parse_size("99999999x1").is_err());
        assert_eq!(parse_position("center").unwrap(), FloatPos::Center);
        assert_eq!(parse_position("-5,7").unwrap(), FloatPos::At(-5, 7));
    }

    #[test]
    fn unknown_keys_error() {
        assert!(Placement::parse(&["wobble"]).is_err());
        assert!(Placement::parse(&["size"]).is_err());
        assert!(Placement::parse(&["tags", "0"]).is_err());
    }

    #[test]
    fn word_and_flag_tokens_round_trip() {
        let cases = [
            Placement::new(),
            Placement::new()
                .on_output("DP-2")
                .tags(TagSet::single(9).unwrap()),
            Placement::new().floating(true).size(1280, 720).centered(),
            Placement::new().size(800, 600).position(-10, 20),
            Placement::new().fullscreen(true).focus(false).locked(true),
            Placement {
                sticky: Some(BoolValue::Toggle),
                border: Some(BoolValue::Off),
                position: Some(FloatPos::At(3, 4)),
                ..Placement::default()
            },
        ];
        for p in cases {
            let words = p.to_tokens();
            let refs: Vec<&str> = words.iter().map(String::as_str).collect();
            assert_eq!(Placement::parse(&refs).unwrap(), p, "words {words:?}");

            let flags = p.to_flags();
            let refs: Vec<&str> = flags.iter().map(String::as_str).collect();
            let mut c = Cursor::new(&refs, Style::Flags);
            let mut back = Placement::default();
            while back.parse_item(&mut c).unwrap() {}
            assert!(c.is_done(), "flags {flags:?} left {:?}", c.rest());
            assert_eq!(back, p, "flags {flags:?}");
        }
    }

    #[test]
    fn merge_lets_later_fields_win() {
        let mut base = Placement::new().on_output("DP-1").floating(true);
        base.merge(&Placement::new().on_output("DP-2").size(10, 10));
        assert_eq!(base.output, Some(OutputTarget::Named("DP-2".into())));
        assert_eq!(base.floating, Some(BoolValue::On));
        assert_eq!(base.size, Some((10, 10)));
    }
}
