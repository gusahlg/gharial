//! Action — the single value that flows from IPC to the wayland thread.
//!
//! Lives at the crate root so both `state` (the IPC-side producer) and
//! `wm` (the wayland-side consumer) can name it without depending on
//! each other's internals.

use crate::edge::{EdgeRef, OutputTarget};
use crate::keysyms::{parse_keysym, parse_modifier};

#[derive(Clone, Debug)]
pub enum Action {
    /// Fork+exec a command, detached.
    Spawn {
        cmd: String,
        args: Vec<String>,
    },
    /// Close the focused window (server-side close request).
    Close,
    /// Shift keyboard focus along the stack order.
    FocusDirection(Direction),
    /// Swap the focused window with its neighbor in the stack and re-layout.
    SwapDirection(Direction),
    /// Toggle the focused window between tiled and floating. Floating
    /// windows keep their own size and are skipped by the tiling layout.
    ToggleFloat,
    /// Toggle the focused window between fullscreen and its normal
    /// tiled/floating state. A fullscreen window covers its output and
    /// is excluded from the tiling layout.
    ToggleFullscreen,
    /// Adjust a layout parameter (mirrors the gharialctl `set` grammar).
    Layout {
        key: String,
        args: Vec<String>,
    },
    /// Switch the active binding mode.
    EnterMode(String),
    /// Return to the default binding mode.
    ExitMode,
    /// Install a new binding. The binding's action fires whenever the
    /// chord triggers and the binding's mode is active.
    Bind {
        spec: BindingSpec,
        action: Box<Action>,
        mode: String,
    },
    /// Remove a binding by (mode, chord).
    Unbind {
        spec: BindingSpec,
        mode: String,
    },

    // Tags
    FocusTag(u8),
    ToggleTag(u8),
    MoveToTag(u8),
    ToggleWindowTag(u8),

    // Outputs (screens)
    /// Switch the focused output. New windows, tag commands, and
    /// keyboard focus follow the focused output.
    FocusOutput(OutputTarget),
    /// Move the focused window to another output. The window adopts
    /// the target output's currently visible tags.
    SendToOutput(OutputTarget),
    /// Link two output edges so the pointer warps through them when it
    /// hits either side. Links are bidirectional.
    LinkOutputs {
        a: EdgeRef,
        b: EdgeRef,
    },
    /// Remove any pointer link touching the given output edge.
    UnlinkOutput(EdgeRef),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Cycle forward in the stack order (insertion order).
    Next,
    /// Cycle backward in the stack order.
    Prev,
    /// Spatially: move focus to the nearest window left of the focused.
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "next" | "n" => Ok(Self::Next),
            "prev" | "previous" | "p" => Ok(Self::Prev),
            "left" | "h" => Ok(Self::Left),
            "right" | "l" => Ok(Self::Right),
            "up" | "k" => Ok(Self::Up),
            "down" | "j" => Ok(Self::Down),
            other => Err(format!(
                "invalid direction: {other} (next|prev|left|right|up|down)"
            )),
        }
    }

    /// Canonical token form. Round-trips with [`Direction::parse`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Prev => "prev",
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// `true` for the four cardinal directions that depend on actual
    /// window geometry rather than the stack-insertion order.
    pub fn is_spatial(self) -> bool {
        matches!(self, Self::Left | Self::Right | Self::Up | Self::Down)
    }
}

/// An xkb chord — a keysym combined with a bitfield of modifier flags.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BindingSpec {
    pub modifiers: u32,
    pub keysym: u32,
}

impl BindingSpec {
    /// Parse a `Super+Shift+Q`-style chord. Modifier and key names are
    /// case-insensitive. Names live in [`crate::keysyms`].
    pub fn parse(chord: &str) -> Result<Self, String> {
        let parts: Vec<&str> = chord.split('+').filter(|p| !p.is_empty()).collect();
        let Some((key, mods)) = parts.split_last() else {
            return Err("empty chord".into());
        };
        let mut modifiers = 0u32;
        for m in mods {
            modifiers |= parse_modifier(m)?;
        }
        let keysym = parse_keysym(key).ok_or_else(|| format!("unknown keysym: {key}"))?;
        Ok(Self { modifiers, keysym })
    }

    /// `const` chord parse — the same grammar as [`BindingSpec::parse`],
    /// usable in a const context. This is what lets the `chord!` macro
    /// reject a bad chord at compile time. Segments are split on `+`;
    /// empties are skipped; the last non-empty segment is the key and the
    /// rest are modifiers.
    pub const fn parse_const(chord: &str) -> Result<Self, &'static str> {
        let b = chord.as_bytes();
        let mut modifiers = 0u32;
        // The most recent non-empty segment seen so far is a *pending*
        // key; when another segment follows, it gets promoted to a
        // modifier and the new one becomes the pending key.
        let mut have_seg = false;
        let mut seg_start = 0usize;
        let mut seg_end = 0usize;

        let mut cursor = 0usize;
        let mut i = 0usize;
        loop {
            let at_end = i == b.len();
            if at_end || b[i] == b'+' {
                if i > cursor {
                    // Non-empty segment [cursor, i).
                    if have_seg {
                        match crate::keysyms::modifier_bytes(b, seg_start, seg_end) {
                            Some(bit) => modifiers |= bit,
                            None => return Err("unknown modifier in chord"),
                        }
                    }
                    seg_start = cursor;
                    seg_end = i;
                    have_seg = true;
                }
                if at_end {
                    break;
                }
                cursor = i + 1;
            }
            i += 1;
        }

        if !have_seg {
            return Err("empty chord");
        }
        match crate::keysyms::keysym_bytes(b, seg_start, seg_end) {
            Some(keysym) => Ok(Self { modifiers, keysym }),
            None => Err("unknown keysym in chord"),
        }
    }

    /// Const chord parse that panics (a *compile* error in a const
    /// context) on a bad chord. Backs the `chord!` macro.
    pub const fn const_checked(chord: &str) -> Self {
        match Self::parse_const(chord) {
            Ok(spec) => spec,
            // Const panic messages must be literals, so collapse the
            // specific reason; the compile error still points at the
            // offending `chord!(...)` call site.
            Err(_) => panic!("chord!: invalid chord — unknown modifier, unknown key, or empty"),
        }
    }
}

impl Action {
    /// Parse an action from a token slice. Tokens are the same shape as
    /// a gharialctl invocation; this is how bound actions get reified
    /// from `gharialctl bind Super+Q <action ...>` calls and how the
    /// IPC `bind` arm validates ahead of forwarding.
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let Some((&cmd, rest)) = tokens.split_first() else {
            return Err("empty action".into());
        };
        match cmd {
            "close" => Ok(Self::Close),
            "focus" => {
                let dir = rest
                    .first()
                    .copied()
                    .ok_or("focus: expected next|prev|left|right|up|down")?;
                Direction::parse(dir).map(Self::FocusDirection)
            }
            "swap" => {
                let dir = rest
                    .first()
                    .copied()
                    .ok_or("swap: expected next|prev|left|right|up|down")?;
                Direction::parse(dir).map(Self::SwapDirection)
            }
            "spawn" => {
                let (&cmd, args) = rest.split_first().ok_or("spawn: missing command")?;
                Ok(Self::Spawn {
                    cmd: cmd.to_string(),
                    args: args.iter().map(|s| (*s).to_string()).collect(),
                })
            }
            "toggle-float" => Ok(Self::ToggleFloat),
            "toggle-fullscreen" | "fullscreen" => Ok(Self::ToggleFullscreen),
            "mode" => {
                let target = rest.first().copied().ok_or("mode: expected <name|exit>")?;
                if target == "exit" {
                    Ok(Self::ExitMode)
                } else {
                    Ok(Self::EnterMode(target.to_string()))
                }
            }
            "tag" => parse_tag_action(rest),
            "output" => parse_output_action(rest),
            // Anything else is treated as a layout-param command — the
            // same set of keys `gharialctl set` accepts.
            other if is_layout_key(other) => Ok(Self::Layout {
                key: other.to_string(),
                args: rest.iter().map(|s| (*s).to_string()).collect(),
            }),
            other => Err(format!("unknown action: {other}")),
        }
    }
}

/// The keys the layout-param grammar accepts. Single source of truth —
/// `Action::parse`, the IPC dispatcher, and `Shared::apply` all consult
/// this list (directly or transitively) so adding a new key is one edit.
pub const LAYOUT_KEYS: &[&str] = &[
    "main-ratio",
    "main-count",
    "gaps",
    "outer-padding",
    "orientation",
    "smart-gaps",
    "border-width",
    "border-color-focused",
    "border-color-unfocused",
];

pub fn is_layout_key(s: &str) -> bool {
    LAYOUT_KEYS.contains(&s)
}

// ─────────────────────────────────────────────────────────────────────
// Encoding side — produce the wire-token list for an Action so the
// public Rust API can ship the same vocabulary as gharialctl.

impl Action {
    /// Encode this action as the token list the daemon expects when it
    /// fires (matching what [`Action::parse`] would accept).
    ///
    /// Round-trips: `Action::parse(&action.to_tokens().iter().map(|s| s.as_str()).collect::<Vec<_>>())`
    /// recovers `action` for every variant the parser knows about.
    ///
    /// `Bind` and `Unbind` are special — they're produced by the IPC
    /// `bind`/`unbind` verb handlers and consumed by the wm thread, not
    /// by the action parser. Encoding either yields an empty token list;
    /// the public `Client` exposes them through dedicated methods.
    pub fn to_tokens(&self) -> Vec<String> {
        match self {
            Self::Close => vec!["close".into()],
            Self::ToggleFloat => vec!["toggle-float".into()],
            Self::ToggleFullscreen => vec!["toggle-fullscreen".into()],
            Self::FocusDirection(dir) => vec!["focus".into(), dir.as_str().into()],
            Self::SwapDirection(dir) => vec!["swap".into(), dir.as_str().into()],
            Self::Spawn { cmd, args } => {
                let mut out = Vec::with_capacity(args.len() + 2);
                out.push("spawn".into());
                out.push(cmd.clone());
                out.extend(args.iter().cloned());
                out
            }
            Self::Layout { key, args } => {
                let mut out = Vec::with_capacity(args.len() + 1);
                out.push(key.clone());
                out.extend(args.iter().cloned());
                out
            }
            Self::EnterMode(name) => vec!["mode".into(), name.clone()],
            Self::ExitMode => vec!["mode".into(), "exit".into()],
            Self::FocusTag(n) => vec!["tag".into(), "focus".into(), n.to_string()],
            Self::ToggleTag(n) => vec!["tag".into(), "toggle".into(), n.to_string()],
            Self::MoveToTag(n) => vec!["tag".into(), "move".into(), n.to_string()],
            Self::ToggleWindowTag(n) => {
                vec!["tag".into(), "window-toggle".into(), n.to_string()]
            }
            Self::FocusOutput(target) => {
                vec!["output".into(), "focus".into(), target.to_token()]
            }
            Self::SendToOutput(target) => {
                vec!["output".into(), "send".into(), target.to_token()]
            }
            Self::LinkOutputs { a, b } => {
                vec!["output".into(), "link".into(), a.to_token(), b.to_token()]
            }
            Self::UnlinkOutput(at) => {
                vec!["output".into(), "unlink".into(), at.to_token()]
            }
            // Bind/Unbind aren't bindable actions — the Client emits the
            // `bind`/`unbind` IPC verbs directly. Return empty so misuse
            // produces an obviously-broken request rather than silent
            // corruption.
            Self::Bind { .. } | Self::Unbind { .. } => Vec::new(),
        }
    }

    // ── Typed constructors for common cases. ─────────────────────────

    /// Build a `Spawn` action from a command + arg iterator. Strings
    /// and `&str` both accepted — `Action::spawn("rio", ["-e", "nvim"])`
    /// and `Action::spawn("rio", [] as [&str; 0])` both compile.
    pub fn spawn<C, I, S>(cmd: C, args: I) -> Self
    where
        C: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Spawn {
            cmd: cmd.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub fn focus(dir: Direction) -> Self {
        Self::FocusDirection(dir)
    }

    pub fn swap(dir: Direction) -> Self {
        Self::SwapDirection(dir)
    }

    pub fn enter_mode(name: impl Into<String>) -> Self {
        Self::EnterMode(name.into())
    }

    /// Switch the focused output by direction (`Direction::Next`, a
    /// cardinal, …).
    pub fn focus_output(dir: Direction) -> Self {
        Self::FocusOutput(OutputTarget::Direction(dir))
    }

    /// Switch the focused output by connector name (`"DP-1"`) or
    /// 1-based advertisement index (`"2"`).
    pub fn focus_output_named(name: impl Into<String>) -> Self {
        Self::FocusOutput(OutputTarget::Named(name.into()))
    }

    /// Move the focused window to another output by direction.
    pub fn send_to_output(dir: Direction) -> Self {
        Self::SendToOutput(OutputTarget::Direction(dir))
    }

    /// Move the focused window to a named/indexed output.
    pub fn send_to_output_named(name: impl Into<String>) -> Self {
        Self::SendToOutput(OutputTarget::Named(name.into()))
    }

    /// Link two output edges so the pointer warps through them (both
    /// directions).
    pub fn link_outputs(
        a_output: impl Into<String>,
        a_edge: crate::edge::Edge,
        b_output: impl Into<String>,
        b_edge: crate::edge::Edge,
    ) -> Self {
        Self::LinkOutputs {
            a: EdgeRef::new(a_output, a_edge),
            b: EdgeRef::new(b_output, b_edge),
        }
    }

    /// Remove any pointer link touching the given output edge.
    pub fn unlink_output(output: impl Into<String>, edge: crate::edge::Edge) -> Self {
        Self::UnlinkOutput(EdgeRef::new(output, edge))
    }

    // Layout-param constructors. Each pins the wire form so the daemon
    // sees the value the user typed — no precision drift across the
    // round-trip.

    pub fn set_main_ratio(value: f32) -> Self {
        Self::layout("main-ratio", [format_f32(value)])
    }
    pub fn adjust_main_ratio(delta: f32) -> Self {
        Self::layout("main-ratio", [format_delta_f32(delta)])
    }
    pub fn set_main_count(value: u32) -> Self {
        Self::layout("main-count", [value.to_string()])
    }
    pub fn adjust_main_count(delta: i32) -> Self {
        Self::layout("main-count", [format_delta_i32(delta)])
    }
    pub fn set_gaps(value: u32) -> Self {
        Self::layout("gaps", [value.to_string()])
    }
    pub fn adjust_gaps(delta: i32) -> Self {
        Self::layout("gaps", [format_delta_i32(delta)])
    }
    pub fn set_outer_padding(value: u32) -> Self {
        Self::layout("outer-padding", [value.to_string()])
    }
    pub fn adjust_outer_padding(delta: i32) -> Self {
        Self::layout("outer-padding", [format_delta_i32(delta)])
    }
    pub fn set_orientation(o: crate::orientation::Orientation) -> Self {
        Self::layout("orientation", [o.as_str().to_string()])
    }
    pub fn set_smart_gaps(v: crate::value::BoolValue) -> Self {
        Self::layout("smart-gaps", [v.as_str().to_string()])
    }
    pub fn set_border_width(value: u32) -> Self {
        Self::layout("border-width", [value.to_string()])
    }
    pub fn set_border_color_focused(c: crate::color::Color) -> Self {
        Self::layout("border-color-focused", [c.to_hex_string()])
    }
    pub fn set_border_color_unfocused(c: crate::color::Color) -> Self {
        Self::layout("border-color-unfocused", [c.to_hex_string()])
    }

    fn layout<I>(key: &str, args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        Self::Layout {
            key: key.into(),
            args: args.into_iter().collect(),
        }
    }
}

fn format_f32(v: f32) -> String {
    // `%.4f`-equivalent — matches the daemon's own summary formatting
    // so round-tripping a value through Action → parse → apply yields
    // the same recorded value.
    format!("{v:.4}")
}

fn format_delta_f32(delta: f32) -> String {
    // `{:+}` formats positives with a leading `+`; the daemon's parser
    // accepts both `+0.05` and `-0.05` as relative adjustments.
    format!("{delta:+.4}")
}

fn format_delta_i32(delta: i32) -> String {
    format!("{delta:+}")
}

fn parse_output_action(tokens: &[&str]) -> Result<Action, String> {
    let (&sub, rest) = tokens
        .split_first()
        .ok_or("output: expected focus|send|link|unlink")?;
    match sub {
        "focus" => {
            let target = rest
                .first()
                .copied()
                .ok_or("output focus: expected next|prev|left|right|up|down|NAME")?;
            Ok(Action::FocusOutput(OutputTarget::parse(target)))
        }
        "send" | "move" => {
            let target = rest
                .first()
                .copied()
                .ok_or("output send: expected next|prev|left|right|up|down|NAME")?;
            Ok(Action::SendToOutput(OutputTarget::parse(target)))
        }
        "link" => match rest {
            [a, b] => Ok(Action::LinkOutputs {
                a: EdgeRef::parse(a)?,
                b: EdgeRef::parse(b)?,
            }),
            _ => Err("output link: expected two OUTPUT:EDGE arguments \
                      (e.g. output link DP-1:right DP-2:left)"
                .into()),
        },
        "unlink" => {
            let at = rest
                .first()
                .copied()
                .ok_or("output unlink: expected OUTPUT:EDGE")?;
            Ok(Action::UnlinkOutput(EdgeRef::parse(at)?))
        }
        other => Err(format!("output: unknown subcommand {other}")),
    }
}

fn parse_tag_action(tokens: &[&str]) -> Result<Action, String> {
    let (&sub, rest) = tokens
        .split_first()
        .ok_or("tag: expected focus|toggle|move|window-toggle")?;
    let n: u8 = rest
        .first()
        .copied()
        .ok_or_else(|| format!("tag {sub}: missing tag number 1..32"))?
        .parse()
        .map_err(|_| format!("tag {sub}: invalid tag number"))?;
    if !(1..=32).contains(&n) {
        return Err(format!("tag {sub}: tag {n} out of range 1..32"));
    }
    Ok(match sub {
        "focus" => Action::FocusTag(n),
        "toggle" => Action::ToggleTag(n),
        "move" | "send" => Action::MoveToTag(n),
        "window-toggle" | "wtoggle" => Action::ToggleWindowTag(n),
        other => return Err(format!("tag: unknown subcommand {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chord_super_q() {
        let s = BindingSpec::parse("Super+Q").unwrap();
        assert_eq!(s.modifiers, 64);
        assert_eq!(s.keysym, b'q' as u32);
    }

    #[test]
    fn parse_chord_modifier_order_irrelevant() {
        let a = BindingSpec::parse("Super+Shift+Q").unwrap();
        let b = BindingSpec::parse("Shift+Super+q").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_chord_bare_key() {
        let s = BindingSpec::parse("Return").unwrap();
        assert_eq!(s.modifiers, 0);
        assert_eq!(s.keysym, 0xff0d);
    }

    #[test]
    fn parse_chord_empty_errors() {
        assert!(BindingSpec::parse("").is_err());
        assert!(BindingSpec::parse("+").is_err());
    }

    #[test]
    fn parse_chord_unknown_modifier_errors() {
        assert!(BindingSpec::parse("Hyper+Q").is_err());
    }

    #[test]
    fn parse_action_close() {
        assert!(matches!(Action::parse(&["close"]).unwrap(), Action::Close));
    }

    #[test]
    fn parse_action_focus_next() {
        assert!(matches!(
            Action::parse(&["focus", "next"]).unwrap(),
            Action::FocusDirection(Direction::Next)
        ));
    }

    #[test]
    fn parse_action_spawn() {
        match Action::parse(&["spawn", "rio", "-e", "nvim"]).unwrap() {
            Action::Spawn { cmd, args } => {
                assert_eq!(cmd, "rio");
                assert_eq!(args, vec!["-e".to_string(), "nvim".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_action_mode() {
        assert!(
            matches!(Action::parse(&["mode", "resize"]).unwrap(), Action::EnterMode(s) if s == "resize")
        );
        assert!(matches!(
            Action::parse(&["mode", "exit"]).unwrap(),
            Action::ExitMode
        ));
    }

    #[test]
    fn parse_action_layout_param() {
        match Action::parse(&["main-ratio", "+0.05"]).unwrap() {
            Action::Layout { key, args } => {
                assert_eq!(key, "main-ratio");
                assert_eq!(args, vec!["+0.05".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_action_tag_focus() {
        assert!(matches!(
            Action::parse(&["tag", "focus", "3"]).unwrap(),
            Action::FocusTag(3)
        ));
        assert!(matches!(
            Action::parse(&["tag", "toggle", "10"]).unwrap(),
            Action::ToggleTag(10)
        ));
        assert!(matches!(
            Action::parse(&["tag", "move", "1"]).unwrap(),
            Action::MoveToTag(1)
        ));
        assert!(matches!(
            Action::parse(&["tag", "window-toggle", "2"]).unwrap(),
            Action::ToggleWindowTag(2)
        ));
    }

    #[test]
    fn parse_action_tag_out_of_range_errors() {
        assert!(Action::parse(&["tag", "focus", "0"]).is_err());
        assert!(Action::parse(&["tag", "focus", "33"]).is_err());
        assert!(Action::parse(&["tag", "frobnicate", "5"]).is_err());
    }

    #[test]
    fn parse_action_unknown_errors() {
        assert!(Action::parse(&[]).is_err());
        assert!(Action::parse(&["fly", "high"]).is_err());
    }

    #[test]
    fn parse_action_toggle_float() {
        assert!(matches!(
            Action::parse(&["toggle-float"]).unwrap(),
            Action::ToggleFloat
        ));
    }

    #[test]
    fn parse_action_toggle_fullscreen() {
        // Both the canonical token and the `fullscreen` alias resolve to
        // the same action.
        assert!(matches!(
            Action::parse(&["toggle-fullscreen"]).unwrap(),
            Action::ToggleFullscreen
        ));
        assert!(matches!(
            Action::parse(&["fullscreen"]).unwrap(),
            Action::ToggleFullscreen
        ));
    }

    #[test]
    fn toggle_fullscreen_round_trips() {
        let a = Action::ToggleFullscreen;
        // Encodes to the canonical token, which parses back to itself.
        assert_eq!(a.to_tokens(), vec!["toggle-fullscreen".to_string()]);
        assert!(matches!(
            parse_tokens(&a.to_tokens()),
            Action::ToggleFullscreen
        ));
    }

    #[test]
    fn parse_action_close_rejects_extra_args_silently() {
        // Close takes no args. Extra args are tolerated rather than
        // erroring — keybindings that pass trailing whitespace would
        // otherwise mysteriously fail.
        assert!(matches!(
            Action::parse(&["close", "ignored"]).unwrap(),
            Action::Close
        ));
    }

    #[test]
    fn parse_action_border_keys_route_to_layout() {
        match Action::parse(&["border-width", "5"]).unwrap() {
            Action::Layout { key, args } => {
                assert_eq!(key, "border-width");
                assert_eq!(args, vec!["5".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            Action::parse(&["border-color-focused", "0xFF0000FF"]).unwrap(),
            Action::Layout { .. }
        ));
    }

    #[test]
    fn every_layout_key_routes_to_action_layout() {
        // LAYOUT_KEYS is the single source of truth — every entry must
        // parse as an Action::Layout, never as an "unknown action".
        for &key in LAYOUT_KEYS {
            let parsed = Action::parse(&[key, "0"]).unwrap_or_else(|e| {
                panic!("LAYOUT_KEYS entry {key:?} should parse as Action::Layout, got: {e}")
            });
            match parsed {
                Action::Layout { key: k, .. } => assert_eq!(k, key),
                other => panic!("LAYOUT_KEYS entry {key:?} parsed as {other:?}, not Layout"),
            }
        }
    }

    #[test]
    fn is_layout_key_matches_layout_keys_const() {
        for &key in LAYOUT_KEYS {
            assert!(is_layout_key(key), "expected {key:?} to be a layout key");
        }
        for not_key in ["close", "focus", "swap", "tag", "spawn", "mode", "bogus"] {
            assert!(
                !is_layout_key(not_key),
                "expected {not_key:?} not to be a layout key"
            );
        }
    }

    #[test]
    fn focus_error_message_lists_spatial_dirs() {
        // The error mentions every direction parser accepts so users
        // discover the spatial set without reading source.
        let err = Action::parse(&["focus"]).unwrap_err();
        for dir in ["next", "prev", "left", "right", "up", "down"] {
            assert!(err.contains(dir), "{err} missing {dir}");
        }
    }

    #[test]
    fn swap_error_message_lists_spatial_dirs() {
        let err = Action::parse(&["swap"]).unwrap_err();
        for dir in ["next", "prev", "left", "right", "up", "down"] {
            assert!(err.contains(dir), "{err} missing {dir}");
        }
    }

    #[test]
    fn direction_parse_accepts_every_spatial_alias() {
        // The is_spatial bit is part of the action vocabulary contract —
        // pin the set so future direction tweaks don't silently regress.
        assert!(matches!(Direction::parse("left"), Ok(Direction::Left)));
        assert!(matches!(Direction::parse("h"), Ok(Direction::Left)));
        assert!(matches!(Direction::parse("right"), Ok(Direction::Right)));
        assert!(matches!(Direction::parse("l"), Ok(Direction::Right)));
        assert!(matches!(Direction::parse("up"), Ok(Direction::Up)));
        assert!(matches!(Direction::parse("k"), Ok(Direction::Up)));
        assert!(matches!(Direction::parse("down"), Ok(Direction::Down)));
        assert!(matches!(Direction::parse("j"), Ok(Direction::Down)));
        assert!(Direction::parse("left").unwrap().is_spatial());
        assert!(!Direction::parse("next").unwrap().is_spatial());
    }

    // ─────────────────────────────────────────────────────────────────
    // Encoding round-trips. Every variant `Action::parse` accepts must
    // also encode back to the same token list.

    fn parse_tokens(tokens: &[String]) -> Action {
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        Action::parse(&refs).unwrap_or_else(|e| panic!("parse({tokens:?}) failed: {e}"))
    }

    #[test]
    fn close_round_trips() {
        let a = Action::Close;
        assert_eq!(a.to_tokens(), vec!["close".to_string()]);
        let back = parse_tokens(&a.to_tokens());
        assert!(matches!(back, Action::Close));
    }

    #[test]
    fn toggle_float_round_trips() {
        let a = Action::ToggleFloat;
        assert_eq!(a.to_tokens(), vec!["toggle-float".to_string()]);
        assert!(matches!(parse_tokens(&a.to_tokens()), Action::ToggleFloat));
    }

    #[test]
    fn every_direction_round_trips_through_focus_and_swap() {
        for dir in [
            Direction::Next,
            Direction::Prev,
            Direction::Left,
            Direction::Right,
            Direction::Up,
            Direction::Down,
        ] {
            let f = Action::focus(dir);
            let tokens = f.to_tokens();
            assert_eq!(tokens, vec!["focus".to_string(), dir.as_str().to_string()]);
            assert!(matches!(parse_tokens(&tokens), Action::FocusDirection(d) if d == dir));

            let s = Action::swap(dir);
            let tokens = s.to_tokens();
            assert_eq!(tokens, vec!["swap".to_string(), dir.as_str().to_string()]);
            assert!(matches!(parse_tokens(&tokens), Action::SwapDirection(d) if d == dir));
        }
    }

    #[test]
    fn spawn_round_trips_with_args() {
        let a = Action::spawn("rio", ["-e", "nvim", "foo.txt"]);
        let tokens = a.to_tokens();
        assert_eq!(
            tokens,
            vec![
                "spawn".to_string(),
                "rio".into(),
                "-e".into(),
                "nvim".into(),
                "foo.txt".into(),
            ]
        );
        match parse_tokens(&tokens) {
            Action::Spawn { cmd, args } => {
                assert_eq!(cmd, "rio");
                assert_eq!(args, vec!["-e", "nvim", "foo.txt"]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn modes_round_trip() {
        let enter = Action::enter_mode("resize");
        assert_eq!(enter.to_tokens(), vec!["mode".to_string(), "resize".into()]);
        assert!(matches!(parse_tokens(&enter.to_tokens()), Action::EnterMode(s) if s == "resize"));

        let exit = Action::ExitMode;
        assert_eq!(exit.to_tokens(), vec!["mode".to_string(), "exit".into()]);
        assert!(matches!(parse_tokens(&exit.to_tokens()), Action::ExitMode));
    }

    #[test]
    fn every_tag_variant_round_trips() {
        for (action, sub) in [
            (Action::FocusTag(3), "focus"),
            (Action::ToggleTag(3), "toggle"),
            (Action::MoveToTag(3), "move"),
            (Action::ToggleWindowTag(3), "window-toggle"),
        ] {
            let tokens = action.to_tokens();
            assert_eq!(
                tokens,
                vec!["tag".to_string(), sub.to_string(), "3".to_string()]
            );
            let back = parse_tokens(&tokens);
            // The variant must come back equivalent.
            assert_eq!(back.to_tokens(), tokens);
        }
    }

    #[test]
    fn every_layout_constructor_round_trips() {
        use crate::color::Color;
        use crate::orientation::Orientation;
        use crate::value::BoolValue;

        // (Action, expected tokens) — also exercised through parse.
        let cases: Vec<(Action, Vec<&str>)> = vec![
            (Action::set_main_ratio(0.55), vec!["main-ratio", "0.5500"]),
            (
                Action::adjust_main_ratio(0.05),
                vec!["main-ratio", "+0.0500"],
            ),
            (
                Action::adjust_main_ratio(-0.05),
                vec!["main-ratio", "-0.0500"],
            ),
            (Action::set_main_count(2), vec!["main-count", "2"]),
            (Action::adjust_main_count(1), vec!["main-count", "+1"]),
            (Action::adjust_main_count(-1), vec!["main-count", "-1"]),
            (Action::set_gaps(8), vec!["gaps", "8"]),
            (Action::adjust_gaps(2), vec!["gaps", "+2"]),
            (Action::adjust_gaps(-2), vec!["gaps", "-2"]),
            (Action::set_outer_padding(4), vec!["outer-padding", "4"]),
            (
                Action::adjust_outer_padding(-1),
                vec!["outer-padding", "-1"],
            ),
            (
                Action::set_orientation(Orientation::Left),
                vec!["orientation", "left"],
            ),
            (
                Action::set_orientation(Orientation::Bottom),
                vec!["orientation", "bottom"],
            ),
            (
                Action::set_smart_gaps(BoolValue::On),
                vec!["smart-gaps", "on"],
            ),
            (
                Action::set_smart_gaps(BoolValue::Toggle),
                vec!["smart-gaps", "toggle"],
            ),
            (Action::set_border_width(3), vec!["border-width", "3"]),
            (
                Action::set_border_color_focused(Color::rgba(0xC8, 0x32, 0x4B, 0xFF)),
                vec!["border-color-focused", "0xC8324BFF"],
            ),
            (
                Action::set_border_color_unfocused(Color::rgba(0x00, 0xC8, 0x96, 0xFF)),
                vec!["border-color-unfocused", "0x00C896FF"],
            ),
        ];
        for (action, expected) in cases {
            let tokens = action.to_tokens();
            let actual: Vec<&str> = tokens.iter().map(String::as_str).collect();
            assert_eq!(actual, expected, "encoding mismatch for {action:?}");
            // Must parse back as a Layout with matching key + args.
            let back = parse_tokens(&tokens);
            assert!(
                matches!(back, Action::Layout { .. }),
                "{action:?} did not parse as Layout"
            );
            // And re-encoding the parsed version must produce the same
            // tokens (true round-trip — no precision loss).
            assert_eq!(back.to_tokens(), tokens);
        }
    }

    #[test]
    fn parse_action_output_focus_and_send() {
        use crate::edge::OutputTarget;
        assert!(matches!(
            Action::parse(&["output", "focus", "next"]).unwrap(),
            Action::FocusOutput(OutputTarget::Direction(Direction::Next))
        ));
        assert!(matches!(
            Action::parse(&["output", "focus", "DP-2"]).unwrap(),
            Action::FocusOutput(OutputTarget::Named(name)) if name == "DP-2"
        ));
        assert!(matches!(
            Action::parse(&["output", "send", "right"]).unwrap(),
            Action::SendToOutput(OutputTarget::Direction(Direction::Right))
        ));
        assert!(Action::parse(&["output", "focus"]).is_err());
        assert!(Action::parse(&["output", "frobnicate", "1"]).is_err());
    }

    #[test]
    fn parse_action_output_link_and_unlink() {
        use crate::edge::Edge;
        match Action::parse(&["output", "link", "DP-1:right", "DP-2:left"]).unwrap() {
            Action::LinkOutputs { a, b } => {
                assert_eq!(a.output, "DP-1");
                assert_eq!(a.edge, Edge::Right);
                assert_eq!(b.output, "DP-2");
                assert_eq!(b.edge, Edge::Left);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            Action::parse(&["output", "unlink", "DP-1:right"]).unwrap(),
            Action::UnlinkOutput(at) if at.output == "DP-1"
        ));
        assert!(Action::parse(&["output", "link", "DP-1:right"]).is_err());
        assert!(Action::parse(&["output", "link", "DP-1:up", "DP-2:left"]).is_err());
        assert!(Action::parse(&["output", "unlink", "DP-1"]).is_err());
    }

    #[test]
    fn every_output_variant_round_trips() {
        use crate::edge::Edge;
        let cases = vec![
            Action::focus_output(Direction::Next),
            Action::focus_output(Direction::Left),
            Action::focus_output_named("DP-2"),
            Action::send_to_output(Direction::Prev),
            Action::send_to_output_named("2"),
            Action::link_outputs("DP-1", Edge::Right, "DP-2", Edge::Left),
            Action::unlink_output("DP-1", Edge::Right),
        ];
        for action in cases {
            let tokens = action.to_tokens();
            let back = parse_tokens(&tokens);
            assert_eq!(back.to_tokens(), tokens, "round-trip drift for {action:?}");
        }
    }

    #[test]
    fn bind_and_unbind_have_no_token_encoding() {
        // Bind / Unbind are emitted only by the IPC `bind`/`unbind`
        // verb handlers; they're not bindable. Their to_tokens() returns
        // empty so misuse fails loudly at the Client layer instead of
        // shipping a malformed request.
        let bind = Action::Bind {
            spec: BindingSpec {
                modifiers: 64,
                keysym: b'q' as u32,
            },
            action: Box::new(Action::Close),
            mode: "default".into(),
        };
        assert!(bind.to_tokens().is_empty());

        let unbind = Action::Unbind {
            spec: BindingSpec {
                modifiers: 0,
                keysym: 0xff1b,
            },
            mode: "default".into(),
        };
        assert!(unbind.to_tokens().is_empty());
    }
}
