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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_op_strips_leading_signs() {
        match parse_op("+0.05") {
            (Op::Add, rest) => assert_eq!(rest, "0.05"),
            other => panic!("unexpected: {:?}", other.1),
        }
        match parse_op("-3") {
            (Op::Sub, rest) => assert_eq!(rest, "3"),
            other => panic!("unexpected: {:?}", other.1),
        }
        match parse_op("0.5") {
            (Op::Set, rest) => assert_eq!(rest, "0.5"),
            other => panic!("unexpected: {:?}", other.1),
        }
    }

    #[test]
    fn apply_bool_accepts_every_documented_alias() {
        for alias in ["on", "true", "yes", "1"] {
            let mut b = false;
            apply_bool(&mut b, alias).unwrap();
            assert!(b, "{alias} should set true");
        }
        for alias in ["off", "false", "no", "0"] {
            let mut b = true;
            apply_bool(&mut b, alias).unwrap();
            assert!(!b, "{alias} should set false");
        }
        let mut b = false;
        apply_bool(&mut b, "toggle").unwrap();
        assert!(b);
        apply_bool(&mut b, "toggle").unwrap();
        assert!(!b);
        assert!(apply_bool(&mut b, "maybe").is_err());
    }

    #[test]
    fn apply_u32_saturating_sub_floors_at_zero() {
        let mut v: u32 = 5;
        apply_u32(&mut v, "-100").unwrap();
        assert_eq!(v, 0, "underflow must saturate, not wrap");
    }

    #[test]
    fn apply_u32_saturating_add_caps_at_max() {
        let mut v: u32 = u32::MAX - 1;
        apply_u32(&mut v, "+5").unwrap();
        assert_eq!(v, u32::MAX, "overflow must saturate");
    }

    #[test]
    fn parse_color_accepts_all_three_prefixes() {
        // 0x..., 0X..., and # are all valid hex prefixes per docs;
        // pin this so a parser refactor doesn't silently break configs.
        for raw in ["0xC8324BFF", "0Xc8324bff", "#C8324BFF"] {
            let _ = parse_color(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
        }
    }

    #[test]
    fn parse_color_rejects_wrong_length() {
        assert!(parse_color("0xFF").is_err());
        assert!(parse_color("0xFFFFFFFFFF").is_err());
        assert!(parse_color("").is_err());
    }

    #[test]
    fn parse_color_rejects_non_hex_bytes() {
        assert!(parse_color("0xZZ334455").is_err());
        assert!(parse_color("#GG112233").is_err());
    }

    #[test]
    fn require_one_reports_the_command_name() {
        let err = require_one("gaps", &[]).unwrap_err();
        assert!(err.contains("gaps"));
    }
}
