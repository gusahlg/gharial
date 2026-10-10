//! Tokens → [`Action`]. This is how bound actions get reified from
//! `gharialctl bind Super+Q <action ...>` and how the IPC verbs validate
//! before anything reaches the wayland thread.

use crate::args::{Cursor, Style};
use crate::output::{OutputTarget, Presentation};
use crate::placement::Placement;
use crate::tags::TagSet;
use crate::value::BoolValue;
use crate::window::{WindowOp, WindowSelector};

use super::{Action, Direction};

/// The keys the layout-param grammar accepts. Single source of truth —
/// `Action::parse`, the IPC dispatcher, and the daemon's settings store
/// all consult this list, so adding a key is one edit.
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

impl Action {
    /// Parse an action from a token slice — the same shape as a
    /// gharialctl invocation.
    pub fn parse(tokens: &[&str]) -> Result<Self, String> {
        let Some((&cmd, rest)) = tokens.split_first() else {
            return Err("empty action".into());
        };
        match cmd {
            "close" => Ok(Self::Close),
            "focus" => parse_direction("focus", rest).map(Self::FocusDirection),
            "swap" => parse_direction("swap", rest).map(Self::SwapDirection),
            "spawn" => parse_spawn(rest),
            "toggle-float" => Ok(Self::ToggleFloat),
            "toggle-fullscreen" | "fullscreen" => Ok(Self::ToggleFullscreen),
            "toggle-sticky" => Ok(Self::ToggleSticky),
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
            "window" => parse_window_action(rest),
            // `set KEY VALUE` is the same action as `KEY VALUE`, so
            // `bind Super+H set main-ratio -0.05` works as documented.
            "set" => match rest.split_first() {
                Some((key, args)) if is_layout_key(key) && !args.is_empty() => Ok(Self::Layout {
                    key: (*key).to_string(),
                    args: args.iter().map(|s| (*s).to_string()).collect(),
                }),
                Some((key, _)) if !is_layout_key(key) => Err(format!("unknown key: {key}")),
                Some((key, _)) => Err(format!("{key}: missing value")),
                None => Err("set: missing key".into()),
            },
            // Layout-param keys double as actions (`main-ratio +0.05`).
            other if is_layout_key(other) => Ok(Self::Layout {
                key: other.to_string(),
                args: rest.iter().map(|s| (*s).to_string()).collect(),
            }),
            other => Err(format!("unknown action: {other}")),
        }
    }
}

fn parse_direction(verb: &str, rest: &[&str]) -> Result<Direction, String> {
    let dir = rest
        .first()
        .copied()
        .ok_or_else(|| format!("{verb}: expected next|prev|left|right|up|down"))?;
    Direction::parse(dir)
}

/// `spawn [--placement-flag ...] [--] CMD [ARGS...]`. Flags must carry
/// their `--` so a command can never be mistaken for one.
fn parse_spawn(rest: &[&str]) -> Result<Action, String> {
    let mut c = Cursor::new(rest, Style::Flags);
    let mut placement = Placement::default();
    loop {
        if c.rest().first() == Some(&"--") {
            c.advance();
            break;
        }
        if !placement.parse_item(&mut c)? {
            if let Some(key) = c.peek_key() {
                return Err(format!("spawn: unknown option --{}", key.name));
            }
            break;
        }
    }
    let (&cmd, args) = c.rest().split_first().ok_or("spawn: missing command")?;
    Ok(Action::Spawn {
        cmd: cmd.to_string(),
        args: args.iter().map(|s| (*s).to_string()).collect(),
        placement,
    })
}

fn parse_output_action(tokens: &[&str]) -> Result<Action, String> {
    let (&sub, rest) = tokens
        .split_first()
        .ok_or("output: expected focus|send|view|presentation|focus-warp")?;
    let first = |what: &str| {
        rest.first()
            .copied()
            .ok_or_else(|| format!("output {sub}: expected {what}"))
    };
    match sub {
        "focus" => Ok(Action::FocusOutput(OutputTarget::parse(first(
            "next|prev|left|right|up|down|NAME",
        )?))),
        "send" | "move" => Ok(Action::SendToOutput(OutputTarget::parse(first(
            "next|prev|left|right|up|down|NAME",
        )?))),
        "focus-warp" => BoolValue::parse(first("on|off|toggle")?).map(Action::SetOutputFocusWarp),
        "view" => {
            let (until_exit, rest) = split_until_exit(rest)?;
            let [target, tags] = rest else {
                return Err("output view: expected <TARGET> <TAGS> [until-exit PID]".into());
            };
            Ok(Action::ViewOutputTags {
                output: OutputTarget::parse(target),
                tags: TagSet::parse(tags)?,
                until_exit,
            })
        }
        "presentation" => {
            let (until_exit, rest) = split_until_exit(rest)?;
            let [target, mode] = rest else {
                return Err(
                    "output presentation: expected <TARGET> <vsync|async|auto> [until-exit PID]"
                        .into(),
                );
            };
            Ok(Action::SetPresentation {
                output: OutputTarget::parse(target),
                mode: Presentation::parse(mode)?,
                until_exit,
            })
        }
        other => Err(format!("output: unknown subcommand {other}")),
    }
}

/// Strip a trailing `until-exit PID` (any spelling) off `rest`.
fn split_until_exit<'t, 'a>(rest: &'t [&'a str]) -> Result<(Option<u32>, &'t [&'a str]), String> {
    let pid_of = |raw: &str| -> Result<u32, String> {
        match raw.parse::<u32>() {
            Ok(pid) if pid > 0 => Ok(pid),
            _ => Err(format!("until-exit: invalid pid {raw:?}")),
        }
    };
    let is_key = |t: &str| matches!(t, "until-exit" | "--until-exit");
    if let [head @ .., key, pid] = rest {
        if is_key(key) {
            return Ok((Some(pid_of(pid)?), head));
        }
    }
    if let [head @ .., last] = rest {
        for prefix in ["until-exit=", "--until-exit="] {
            if let Some(pid) = last.strip_prefix(prefix) {
                return Ok((Some(pid_of(pid)?), head));
            }
        }
    }
    Ok((None, rest))
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

/// `window SELECTOR OP...`
fn parse_window_action(tokens: &[&str]) -> Result<Action, String> {
    let (selector, rest) = WindowSelector::parse_prefix(tokens)?;
    let op = WindowOp::parse(rest)?;
    Ok(Action::Window { selector, op })
}
