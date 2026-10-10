use super::*;
use crate::output::{OutputTarget, Presentation};
use crate::placement::Placement;
use crate::tags::TagSet;
use crate::value::BoolValue;
use crate::window::{WindowOp, WindowSelector};

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
        Action::Spawn {
            cmd,
            args,
            placement,
        } => {
            assert_eq!(cmd, "rio");
            assert_eq!(args, vec!["-e".to_string(), "nvim".to_string()]);
            assert!(placement.is_empty());
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
        Action::Spawn {
            cmd,
            args,
            placement,
        } => {
            assert_eq!(cmd, "rio");
            assert_eq!(args, vec!["-e", "nvim", "foo.txt"]);
            assert!(placement.is_empty());
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
fn parse_action_output_focus_warp() {
    assert!(matches!(
        Action::parse(&["output", "focus-warp", "on"]).unwrap(),
        Action::SetOutputFocusWarp(BoolValue::On)
    ));
    assert!(matches!(
        Action::parse(&["output", "focus-warp", "off"]).unwrap(),
        Action::SetOutputFocusWarp(BoolValue::Off)
    ));
    assert!(matches!(
        Action::parse(&["output", "focus-warp", "toggle"]).unwrap(),
        Action::SetOutputFocusWarp(BoolValue::Toggle)
    ));

    let missing = Action::parse(&["output", "focus-warp"]).unwrap_err();
    assert!(missing.contains("on|off|toggle"));
    let invalid = Action::parse(&["output", "focus-warp", "maybe"]).unwrap_err();
    assert!(invalid.contains("maybe"));
    assert!(invalid.contains("on|off|toggle"));
}

#[test]
fn every_output_variant_round_trips() {
    let cases = vec![
        Action::focus_output(Direction::Next),
        Action::focus_output(Direction::Left),
        Action::focus_output_named("DP-2"),
        Action::send_to_output(Direction::Prev),
        Action::send_to_output_named("2"),
        Action::set_output_focus_warp(BoolValue::On),
        Action::set_output_focus_warp(BoolValue::Off),
        Action::set_output_focus_warp(BoolValue::Toggle),
    ];
    for action in cases {
        let tokens = action.to_tokens();
        let back = parse_tokens(&tokens);
        assert_eq!(back.to_tokens(), tokens, "round-trip drift for {action:?}");
    }
}

// ─────────────────────────────────────────────────────────────────────
// 0.4 vocabulary: placement-aware spawn, sticky, output view /
// presentation, and per-window commands.

#[test]
fn spawn_with_placement_flags() {
    let a = Action::parse(&[
        "spawn",
        "--output",
        "DP-2",
        "--float",
        "--size=1280x720",
        "--",
        "game",
        "--fast",
    ])
    .unwrap();
    assert_eq!(
        a,
        Action::spawn_placed(
            Placement::new()
                .on_output("DP-2")
                .floating(true)
                .size(1280, 720),
            "game",
            ["--fast"],
        )
    );
    assert_eq!(parse_tokens(&a.to_tokens()), a);
}

#[test]
fn spawn_flags_never_swallow_the_command() {
    // A bare word ends the flags, even one that is a placement key.
    let a = Action::parse(&["spawn", "--float", "lock", "now"]).unwrap();
    match a {
        Action::Spawn {
            cmd,
            args,
            placement,
        } => {
            assert_eq!(cmd, "lock");
            assert_eq!(args, ["now"]);
            assert_eq!(placement.floating, Some(BoolValue::On));
        }
        other => panic!("{other:?}"),
    }
    assert!(Action::parse(&["spawn", "--frobnicate", "x"]).is_err());
    assert!(Action::parse(&["spawn", "--float"]).is_err());
    assert!(Action::parse(&["spawn", "--"]).is_err());
}

#[test]
fn spawn_of_a_dash_command_round_trips() {
    let a = Action::spawn("-weird", ["x"]);
    assert_eq!(parse_tokens(&a.to_tokens()), a);
}

#[test]
fn new_variants_round_trip() {
    let cases = vec![
        Action::ToggleSticky,
        Action::view_output_tags("DP-2", TagSet::single(9).unwrap()),
        Action::view_output_tags("2", TagSet::parse("1,3").unwrap()),
        Action::set_presentation("DP-2", Presentation::Async),
        Action::set_presentation("HDMI-A-1", Presentation::Auto),
        Action::view_output_tags("DP-2", TagSet::single(9).unwrap()).until_exit(4242),
        Action::set_presentation("DP-2", Presentation::Async).until_exit(7),
        Action::window(WindowSelector::Focused, WindowOp::Close),
        Action::window(
            WindowSelector::Id(7),
            WindowOp::Place(Placement::new().sticky(true).on_output("next")),
        ),
        Action::window(
            WindowSelector::parse(&["app-id=wattbench-*"]).unwrap(),
            WindowOp::Place(Placement::new().size(1280, 720).position(0, 0)),
        ),
    ];
    for action in cases {
        let tokens = action.to_tokens();
        assert_eq!(parse_tokens(&tokens), action, "{tokens:?}");
    }
}

#[test]
fn output_view_and_presentation_validate() {
    assert!(Action::parse(&["output", "view", "DP-2"]).is_err());
    assert!(Action::parse(&["output", "view", "DP-2", "0"]).is_err());
    assert!(Action::parse(&["output", "presentation", "DP-2", "fast"]).is_err());
    assert_eq!(
        Action::parse(&["output", "view", "next", "all"]).unwrap(),
        Action::ViewOutputTags {
            output: OutputTarget::Direction(Direction::Next),
            tags: TagSet::ALL,
            until_exit: None,
        }
    );
    assert_eq!(
        Action::parse(&["output", "view", "DP-2", "9", "--until-exit=12"]).unwrap(),
        Action::view_output_tags("DP-2", TagSet::single(9).unwrap()).until_exit(12)
    );
    assert!(Action::parse(&["output", "view", "DP-2", "9", "until-exit", "x"]).is_err());
    assert!(Action::parse(&["output", "view", "DP-2", "9", "until-exit"]).is_err());
}

#[test]
fn window_action_parses_selector_then_op() {
    assert_eq!(
        Action::parse(&["window", "focused", "sticky", "toggle"]).unwrap(),
        Action::window(
            WindowSelector::Focused,
            WindowOp::Place(Placement {
                sticky: Some(BoolValue::Toggle),
                ..Placement::default()
            })
        )
    );
    assert!(Action::parse(&["window"]).is_err());
    assert!(Action::parse(&["window", "focused"]).is_err());
    assert!(Action::parse(&["window", "list"]).is_err());
}

#[test]
fn set_form_parses_like_the_bare_key() {
    assert_eq!(
        Action::parse(&["set", "main-ratio", "-0.05"]).unwrap(),
        Action::parse(&["main-ratio", "-0.05"]).unwrap()
    );
    assert!(Action::parse(&["set", "nope", "1"]).is_err());
    assert!(Action::parse(&["set", "gaps"]).is_err());
    assert!(Action::parse(&["set"]).is_err());
}
