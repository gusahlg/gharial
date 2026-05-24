//! Tests for the public `Shared` surface — exercises the command grammar
//! through the locked entry point so we cover both parsing and dirty-flag
//! propagation in one go.

use super::Shared;
use crate::layout::Params;

#[test]
fn set_main_ratio_absolute() {
    let s = Shared::new(Params::default());
    let res = s.apply("main-ratio", &["0.7"]).unwrap();
    assert_eq!(s.snapshot().main_ratio, 0.7);
    assert!(res.changed);
}

#[test]
fn relative_increment_and_decrement() {
    let s = Shared::new(Params { main_ratio: 0.5, ..Params::default() });
    s.apply("main-ratio", &["+0.1"]).unwrap();
    assert!((s.snapshot().main_ratio - 0.6).abs() < 1e-5);
    s.apply("main-ratio", &["-0.2"]).unwrap();
    assert!((s.snapshot().main_ratio - 0.4).abs() < 1e-5);
}

#[test]
fn ratio_is_clamped() {
    let s = Shared::new(Params::default());
    s.apply("main-ratio", &["10"]).unwrap();
    assert!(s.snapshot().main_ratio <= 0.95);
    s.apply("main-ratio", &["-100"]).unwrap();
    assert!(s.snapshot().main_ratio >= 0.05);
}

#[test]
fn main_count_saturates_at_one() {
    let s = Shared::new(Params { main_count: 1, ..Params::default() });
    s.apply("main-count", &["-5"]).unwrap();
    assert_eq!(s.snapshot().main_count, 1);
}

#[test]
fn smart_gaps_toggle() {
    let s = Shared::new(Params { smart_gaps: true, ..Params::default() });
    s.apply("smart-gaps", &["toggle"]).unwrap();
    assert!(!s.snapshot().smart_gaps);
    s.apply("smart-gaps", &["toggle"]).unwrap();
    assert!(s.snapshot().smart_gaps);
}

#[test]
fn unknown_command_is_rejected() {
    let s = Shared::new(Params::default());
    assert!(s.apply("frobnicate", &["x"]).is_err());
}

#[test]
fn status_line_round_format() {
    let s = Shared::new(Params::default());
    let line = s.status_line();
    for key in ["main-ratio=", "main-count=", "gaps=", "outer-padding=", "orientation=", "smart-gaps="] {
        assert!(line.contains(key), "missing {key} in {line}");
    }
}

#[test]
fn dirty_flag_only_trips_on_real_change() {
    let s = Shared::new(Params { gaps: 8, ..Params::default() });
    s.take_dirty(); // drain anything from construction
    let r = s.apply("gaps", &["8"]).unwrap(); // no-op
    assert!(!r.changed);
    assert!(!s.take_dirty());

    let r = s.apply("gaps", &["12"]).unwrap();
    assert!(r.changed);
    assert!(s.take_dirty());
    assert!(!s.take_dirty(), "dirty should latch off after take");
}

#[test]
fn send_action_errors_before_sender_is_installed() {
    use crate::action::{Action, Direction};
    let s = Shared::new(Params::default());
    let err = s.send_action(Action::FocusDirection(Direction::Next)).unwrap_err();
    assert!(err.contains("not ready"), "got {err}");
}

#[test]
fn border_width_roundtrip() {
    let s = Shared::new(Params::default());
    s.take_dirty();
    let r = s.apply("border-width", &["6"]).unwrap();
    assert!(r.changed);
    assert!(s.take_dirty());
    assert_eq!(s.get("border-width").unwrap(), "6");
}

#[test]
fn border_color_premultiplied_at_zero_and_full_alpha() {
    let s = Shared::new(Params::default());
    s.apply("border-color-focused", &["0x80808000"]).unwrap();
    // Alpha 0 must zero out the colour components after pre-multiplication;
    // alpha byte 0x00 is reported back literally.
    let printed = s.get("border-color-focused").unwrap();
    assert!(printed.ends_with("00"), "alpha byte expected 00: {printed}");

    s.apply("border-color-focused", &["0xFFFFFFFF"]).unwrap();
    // Round-trip the user-typed value through pre-mul + format.
    assert_eq!(s.get("border-color-focused").unwrap(), "0xFFFFFFFF");
}

#[test]
fn border_color_rejects_malformed_input() {
    let s = Shared::new(Params::default());
    assert!(s.apply("border-color-focused", &["red"]).is_err());
    assert!(s.apply("border-color-focused", &["0xZZAA00FF"]).is_err());
    assert!(s.apply("border-color-focused", &["0xABCDEF"]).is_err(), "needs 8 digits");
}

#[test]
fn status_line_includes_borders() {
    let s = Shared::new(Params::default());
    let line = s.status_line();
    for key in ["border-width=", "border-color-focused=", "border-color-unfocused="] {
        assert!(line.contains(key), "missing {key} in {line}");
    }
}

#[test]
fn border_width_zero_clears_borders() {
    let s = Shared::new(Params::default());
    s.apply("border-width", &["0"]).unwrap();
    assert_eq!(s.get("border-width").unwrap(), "0");
    assert_eq!(s.borders().width, 0);
}

#[test]
fn border_color_accepts_hash_prefix() {
    let s = Shared::new(Params::default());
    s.apply("border-color-focused", &["#80808080"]).unwrap();
    // Reported back as 0x...
    assert!(s.get("border-color-focused").unwrap().starts_with("0x"));
}

#[test]
fn user_config_colors_round_trip() {
    // Sanity-check that the actual values shipped in the nix module
    // survive parse → premultiply → format unchanged.
    let s = Shared::new(Params::default());
    s.apply("border-color-focused", &["0xC8324BFF"]).unwrap();
    assert_eq!(s.get("border-color-focused").unwrap(), "0xC8324BFF");
    s.apply("border-color-unfocused", &["0x00C896FF"]).unwrap();
    assert_eq!(s.get("border-color-unfocused").unwrap(), "0x00C896FF");
}
