//! Action — the single value that flows from IPC to the wayland thread.
//!
//! Lives at the crate root so both `state` (the IPC-side producer) and
//! `wm` (the wayland-side consumer) can name it without depending on
//! each other's internals.

use crate::keysyms::{parse_keysym, parse_modifier};

#[derive(Clone, Debug)]
pub enum Action {
    /// Fork+exec a command, detached.
    Spawn { cmd: String, args: Vec<String> },
    /// Close the focused window (server-side close request).
    Close,
    /// Shift keyboard focus along the stack order.
    FocusDirection(Direction),
    /// Swap the focused window with its neighbor in the stack and re-layout.
    SwapDirection(Direction),
    /// Toggle the focused window between tiled and floating. Floating
    /// windows keep their own size and are skipped by the tiling layout.
    ToggleFloat,
    /// Adjust a layout parameter (mirrors the gharialctl `set` grammar).
    Layout { key: String, args: Vec<String> },
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
    Unbind { spec: BindingSpec, mode: String },

    // Tags
    FocusTag(u8),
    ToggleTag(u8),
    MoveToTag(u8),
    ToggleWindowTag(u8),
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
                let dir = rest.first().copied().ok_or("focus: expected next|prev")?;
                Direction::parse(dir).map(Self::FocusDirection)
            }
            "swap" => {
                let dir = rest.first().copied().ok_or("swap: expected next|prev")?;
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
            "mode" => {
                let target = rest.first().copied().ok_or("mode: expected <name|exit>")?;
                if target == "exit" {
                    Ok(Self::ExitMode)
                } else {
                    Ok(Self::EnterMode(target.to_string()))
                }
            }
            "tag" => parse_tag_action(rest),
            // Anything else is treated as a layout-param command — the
            // same set of keys `gharialctl set` accepts.
            other if is_layout_key(other) => {
                Ok(Self::Layout {
                    key: other.to_string(),
                    args: rest.iter().map(|s| (*s).to_string()).collect(),
                })
            }
            other => Err(format!("unknown action: {other}")),
        }
    }
}

fn is_layout_key(s: &str) -> bool {
    matches!(s,
        "main-ratio" | "main-count" | "gaps" | "outer-padding" | "orientation"
        | "smart-gaps" | "border-width" | "border-color-focused"
        | "border-color-unfocused"
    )
}

fn parse_tag_action(tokens: &[&str]) -> Result<Action, String> {
    let (&sub, rest) = tokens.split_first()
        .ok_or("tag: expected focus|toggle|move|window-toggle")?;
    let n: u8 = rest.first()
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
        assert!(matches!(Action::parse(&["mode", "resize"]).unwrap(), Action::EnterMode(s) if s == "resize"));
        assert!(matches!(Action::parse(&["mode", "exit"]).unwrap(), Action::ExitMode));
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
        assert!(matches!(Action::parse(&["tag", "focus", "3"]).unwrap(), Action::FocusTag(3)));
        assert!(matches!(Action::parse(&["tag", "toggle", "10"]).unwrap(), Action::ToggleTag(10)));
        assert!(matches!(Action::parse(&["tag", "move", "1"]).unwrap(), Action::MoveToTag(1)));
        assert!(matches!(Action::parse(&["tag", "window-toggle", "2"]).unwrap(), Action::ToggleWindowTag(2)));
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
        assert!(matches!(Action::parse(&["toggle-float"]).unwrap(), Action::ToggleFloat));
    }

    #[test]
    fn parse_action_close_rejects_extra_args_silently() {
        // Close takes no args. Extra args are tolerated rather than
        // erroring — keybindings that pass trailing whitespace would
        // otherwise mysteriously fail.
        assert!(matches!(Action::parse(&["close", "ignored"]).unwrap(), Action::Close));
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
}
