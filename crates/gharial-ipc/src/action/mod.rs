//! Action — the single value that flows from IPC (and key bindings) to
//! the daemon's wayland thread.
//!
//! The grammar lives in [`parse`] (tokens → `Action`) and its inverse in
//! [`encode`] (`Action` → tokens); both are exercised as round-trips in
//! the tests, so the CLI, bindings, and the Rust `Client` can never
//! drift apart.

mod encode;
mod parse;

#[cfg(test)]
mod tests;

use crate::keysyms::{parse_keysym, parse_modifier};
use crate::output::{OutputTarget, Presentation};
use crate::placement::Placement;
use crate::tags::TagSet;
use crate::value::BoolValue;
use crate::window::{WindowOp, WindowSelector};

pub use parse::{is_layout_key, LAYOUT_KEYS};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Fork+exec a command, detached. A non-empty `placement` applies to
    /// the first window the spawned process tree opens.
    Spawn {
        cmd: String,
        args: Vec<String>,
        placement: Placement,
    },
    /// Close the focused window (server-side close request).
    Close,
    /// Shift keyboard focus along the stack order or spatially.
    FocusDirection(Direction),
    /// Swap the focused window with its neighbour and re-layout.
    SwapDirection(Direction),
    /// Toggle the focused window between tiled and floating.
    ToggleFloat,
    /// Toggle the focused window's fullscreen state.
    ToggleFullscreen,
    /// Toggle whether the focused window shows on every tag.
    ToggleSticky,
    /// Adjust a layout parameter (mirrors the gharialctl `set` grammar).
    Layout {
        key: String,
        args: Vec<String>,
    },
    /// Switch the active binding mode.
    EnterMode(String),
    /// Return to the default binding mode.
    ExitMode,

    // Tags (on the focused output / focused window)
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
    /// Enable, disable, or toggle automatic pointer warping when the
    /// focused output changes through an `output focus` action.
    SetOutputFocusWarp(BoolValue),
    /// Show `tags` on an output without changing which output (or
    /// window) has focus. With `until_exit`, the previous view comes
    /// back once that process exits (unless it was changed meanwhile).
    ViewOutputTags {
        output: OutputTarget,
        tags: TagSet,
        until_exit: Option<u32>,
    },
    /// Set an output's presentation mode (vsync, async, auto), restored
    /// like [`Action::ViewOutputTags`] when `until_exit` is given.
    SetPresentation {
        output: OutputTarget,
        mode: Presentation,
        until_exit: Option<u32>,
    },

    /// Act on the windows a selector picks.
    Window {
        selector: WindowSelector,
        op: WindowOp,
    },
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

// ── Typed constructors ───────────────────────────────────────────────

impl Action {
    /// Build a `Spawn` action from a command + arg iterator. Strings
    /// and `&str` both accepted — `Action::spawn("rio", ["-e", "nvim"])`
    /// and `Action::spawn("rio", [] as [&str; 0])` both compile.
    pub fn spawn<C, I, S>(cmd: C, args: I) -> Self
    where
        C: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::spawn_placed(Placement::default(), cmd, args)
    }

    /// `spawn` whose first window gets `placement`.
    pub fn spawn_placed<C, I, S>(placement: Placement, cmd: C, args: I) -> Self
    where
        C: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Spawn {
            cmd: cmd.into(),
            args: args.into_iter().map(Into::into).collect(),
            placement,
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

    /// Configure whether `output focus` warps the pointer to the newly
    /// focused output.
    pub fn set_output_focus_warp(value: BoolValue) -> Self {
        Self::SetOutputFocusWarp(value)
    }

    /// Show `tags` on an output, leaving output and keyboard focus alone.
    pub fn view_output_tags(output: impl Into<String>, tags: TagSet) -> Self {
        Self::ViewOutputTags {
            output: OutputTarget::parse(&output.into()),
            tags,
            until_exit: None,
        }
    }

    /// Set an output's presentation mode.
    pub fn set_presentation(output: impl Into<String>, mode: Presentation) -> Self {
        Self::SetPresentation {
            output: OutputTarget::parse(&output.into()),
            mode,
            until_exit: None,
        }
    }

    /// Tie a view/presentation change to a process: when `pid` exits the
    /// previous state is restored. No effect on other variants.
    pub fn until_exit(mut self, pid: u32) -> Self {
        match &mut self {
            Self::ViewOutputTags { until_exit, .. } | Self::SetPresentation { until_exit, .. } => {
                *until_exit = Some(pid);
            }
            _ => {}
        }
        self
    }

    /// Apply `op` to the windows `selector` picks.
    pub fn window(selector: WindowSelector, op: WindowOp) -> Self {
        Self::Window { selector, op }
    }

    // Layout-param constructors. Each pins the wire form so the daemon
    // sees the value the user typed — no precision drift across the
    // round-trip.

    pub fn set_main_ratio(value: f32) -> Self {
        Self::layout("main-ratio", [encode::format_f32(value)])
    }
    pub fn adjust_main_ratio(delta: f32) -> Self {
        Self::layout("main-ratio", [encode::format_delta_f32(delta)])
    }
    pub fn set_main_count(value: u32) -> Self {
        Self::layout("main-count", [value.to_string()])
    }
    pub fn adjust_main_count(delta: i32) -> Self {
        Self::layout("main-count", [format!("{delta:+}")])
    }
    pub fn set_gaps(value: u32) -> Self {
        Self::layout("gaps", [value.to_string()])
    }
    pub fn adjust_gaps(delta: i32) -> Self {
        Self::layout("gaps", [format!("{delta:+}")])
    }
    pub fn set_outer_padding(value: u32) -> Self {
        Self::layout("outer-padding", [value.to_string()])
    }
    pub fn adjust_outer_padding(delta: i32) -> Self {
        Self::layout("outer-padding", [format!("{delta:+}")])
    }
    pub fn set_orientation(o: crate::orientation::Orientation) -> Self {
        Self::layout("orientation", [o.as_str().to_string()])
    }
    pub fn set_smart_gaps(v: BoolValue) -> Self {
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
