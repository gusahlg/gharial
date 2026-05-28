//! Pure command-grammar logic: take a `Params` plus a parsed command,
//! mutate `Params`, or return a structured error message.
//!
//! No locks, no I/O. Tests for the grammar live alongside `Shared`
//! because they exercise the locked entry point.

use crate::layout::{Orientation, Params};

use super::{premultiply_straight, BorderConfig};

pub(super) fn apply_command(p: &mut Params, cmd: &str, args: &[&str]) -> Result<(), String> {
    match cmd {
        "main-ratio" => apply_float(&mut p.main_ratio, require_one(cmd, args)?),
        "main-count" => apply_u32(&mut p.main_count, require_one(cmd, args)?),
        "gaps" => apply_u32(&mut p.gaps, require_one(cmd, args)?),
        "outer-padding" => apply_u32(&mut p.outer_padding, require_one(cmd, args)?),
        "orientation" => {
            let v = require_one(cmd, args)?;
            p.orientation = v
                .parse::<Orientation>()
                .map_err(|_| format!("invalid orientation: {v} (left|right|top|bottom)"))?;
            Ok(())
        }
        "smart-gaps" => apply_bool(&mut p.smart_gaps, require_one(cmd, args)?),
        _ => Err(format!("unknown command: {cmd}")),
    }
}

pub(super) fn summarize(p: &Params, cmd: &str) -> String {
    match cmd {
        "main-ratio" => format!("main-ratio={:.4}", p.main_ratio),
        "main-count" => format!("main-count={}", p.main_count),
        "gaps" => format!("gaps={}", p.gaps),
        "outer-padding" => format!("outer-padding={}", p.outer_padding),
        "orientation" => format!("orientation={}", p.orientation.as_str()),
        "smart-gaps" => format!("smart-gaps={}", p.smart_gaps),
        _ => String::new(),
    }
}

pub(super) fn apply_border_command(
    b: &mut BorderConfig,
    cmd: &str,
    args: &[&str],
) -> Result<(), String> {
    match cmd {
        "border-width" => apply_u32(&mut b.width, require_one(cmd, args)?),
        "border-color-focused" => {
            b.focused = parse_color(require_one(cmd, args)?)?;
            Ok(())
        }
        "border-color-unfocused" => {
            b.unfocused = parse_color(require_one(cmd, args)?)?;
            Ok(())
        }
        _ => Err(format!("unknown border command: {cmd}")),
    }
}

pub(super) fn summarize_border(b: &BorderConfig, cmd: &str) -> String {
    match cmd {
        "border-width" => format!("border-width={}", b.width),
        "border-color-focused" => {
            format!("border-color-focused={}", super::format_color(&b.focused))
        }
        "border-color-unfocused" => format!(
            "border-color-unfocused={}",
            super::format_color(&b.unfocused)
        ),
        _ => String::new(),
    }
}

/// Parse a colour spelled `0xRRGGBBAA` or `#RRGGBBAA` into the
/// pre-multiplied RGBA form. Each pair is one byte; the order is
/// red-green-blue-alpha (matching river-classic's `border-color-*`).
fn parse_color(raw: &str) -> Result<super::BorderColor, String> {
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .or_else(|| raw.strip_prefix('#'))
        .unwrap_or(raw);
    if hex.len() != 8 {
        return Err(format!(
            "invalid color {raw}: expected 8 hex digits (RRGGBBAA)"
        ));
    }
    let parse = |i: usize| -> Result<u8, String> {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|_| format!("invalid color {raw}: non-hex byte at position {i}"))
    };
    let r = parse(0)?;
    let g = parse(2)?;
    let b = parse(4)?;
    let a = parse(6)?;
    Ok(premultiply_straight(r, g, b, a))
}

fn require_one<'a>(cmd: &str, args: &'a [&'a str]) -> Result<&'a str, String> {
    args.first()
        .copied()
        .ok_or_else(|| format!("{cmd}: missing value"))
}

enum Op {
    Set,
    Add,
    Sub,
}

fn parse_op(s: &str) -> (Op, &str) {
    if let Some(r) = s.strip_prefix('+') {
        (Op::Add, r)
    } else if let Some(r) = s.strip_prefix('-') {
        (Op::Sub, r)
    } else {
        (Op::Set, s)
    }
}

fn apply_float(field: &mut f32, raw: &str) -> Result<(), String> {
    let (op, rest) = parse_op(raw);
    let v: f32 = rest.parse().map_err(|_| format!("invalid number: {raw}"))?;
    match op {
        Op::Set => *field = v,
        Op::Add => *field += v,
        Op::Sub => *field -= v,
    }
    Ok(())
}

fn apply_u32(field: &mut u32, raw: &str) -> Result<(), String> {
    let (op, rest) = parse_op(raw);
    let v: u32 = rest
        .parse()
        .map_err(|_| format!("invalid integer: {raw}"))?;
    match op {
        Op::Set => *field = v,
        Op::Add => *field = field.saturating_add(v),
        Op::Sub => *field = field.saturating_sub(v),
    }
    Ok(())
}

fn apply_bool(field: &mut bool, raw: &str) -> Result<(), String> {
    *field = match raw {
        "on" | "true" | "yes" | "1" => true,
        "off" | "false" | "no" | "0" => false,
        "toggle" => !*field,
        _ => return Err(format!("invalid boolean: {raw} (on|off|toggle)")),
    };
    Ok(())
}
