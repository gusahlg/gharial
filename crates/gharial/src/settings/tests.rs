//! Tests for the `Settings` surface — exercises the command grammar
//! through `apply`/`get` so parsing, clamping, and change detection are
//! covered together.

use super::Settings;
use crate::layout::Params;

#[test]
fn set_main_ratio_absolute() {
    let mut s = Settings::default();
    let res = s.apply("main-ratio", &["0.7"]).unwrap();
    assert_eq!(s.params.main_ratio, 0.7);
    assert!(res.changed);
}

#[test]
fn relative_increment_and_decrement() {
    let mut s = Settings::with_params(Params {
        main_ratio: 0.5,
        ..Params::default()
    });
    let _ = s.apply("main-ratio", &["+0.1"]).unwrap();
    assert!((s.params.main_ratio - 0.6).abs() < 1e-5);
    let _ = s.apply("main-ratio", &["-0.2"]).unwrap();
    assert!((s.params.main_ratio - 0.4).abs() < 1e-5);
}

#[test]
fn ratio_is_clamped() {
    let mut s = Settings::default();
    let _ = s.apply("main-ratio", &["10"]).unwrap();
    assert!(s.params.main_ratio <= 0.95);
    let _ = s.apply("main-ratio", &["-100"]).unwrap();
    assert!(s.params.main_ratio >= 0.05);
}

#[test]
fn main_count_saturates_at_one() {
    let mut s = Settings::with_params(Params {
        main_count: 1,
        ..Params::default()
    });
    let _ = s.apply("main-count", &["-5"]).unwrap();
    assert_eq!(s.params.main_count, 1);
}

#[test]
fn smart_gaps_toggle() {
    let mut s = Settings::with_params(Params {
        smart_gaps: true,
        ..Params::default()
    });
    let _ = s.apply("smart-gaps", &["toggle"]).unwrap();
    assert!(!s.params.smart_gaps);
    let _ = s.apply("smart-gaps", &["toggle"]).unwrap();
    assert!(s.params.smart_gaps);
}

#[test]
fn unknown_command_is_rejected() {
    let mut s = Settings::default();
    assert!(s.apply("frobnicate", &["x"]).is_err());
}

#[test]
fn status_line_round_format() {
    let s = Settings::default();
    let line = s.status_line();
    for key in [
        "main-ratio=",
        "main-count=",
        "gaps=",
        "outer-padding=",
        "orientation=",
        "smart-gaps=",
    ] {
        assert!(line.contains(key), "missing {key} in {line}");
    }
}

#[test]
fn changed_only_reports_real_changes() {
    let mut s = Settings::with_params(Params {
        gaps: 8,
        ..Params::default()
    });
    let r = s.apply("gaps", &["8"]).unwrap(); // no-op
    assert!(!r.changed);
    let r = s.apply("gaps", &["12"]).unwrap();
    assert!(r.changed);
}

#[test]
fn border_width_roundtrip() {
    let mut s = Settings::default();
    let r = s.apply("border-width", &["6"]).unwrap();
    assert!(r.changed);
    assert_eq!(s.get("border-width").unwrap(), "6");
}

#[test]
fn border_color_premultiplied_at_zero_and_full_alpha() {
    let mut s = Settings::default();
    let _ = s.apply("border-color-focused", &["0x80808000"]).unwrap();
    // Alpha 0 must zero out the colour components after pre-multiplication;
    // alpha byte 0x00 is reported back literally.
    let printed = s.get("border-color-focused").unwrap();
    assert!(printed.ends_with("00"), "alpha byte expected 00: {printed}");

    let _ = s.apply("border-color-focused", &["0xFFFFFFFF"]).unwrap();
    // Round-trip the user-typed value through pre-mul + format.
    assert_eq!(s.get("border-color-focused").unwrap(), "0xFFFFFFFF");
}

#[test]
fn border_color_rejects_malformed_input() {
    let mut s = Settings::default();
    assert!(s.apply("border-color-focused", &["red"]).is_err());
    assert!(s.apply("border-color-focused", &["0xZZAA00FF"]).is_err());
    assert!(
        s.apply("border-color-focused", &["0xABCDEF"]).is_err(),
        "needs 8 digits"
    );
}

#[test]
fn status_line_includes_borders() {
    let s = Settings::default();
    let line = s.status_line();
    for key in [
        "border-width=",
        "border-color-focused=",
        "border-color-unfocused=",
    ] {
        assert!(line.contains(key), "missing {key} in {line}");
    }
}

#[test]
fn border_width_zero_clears_borders() {
    let mut s = Settings::default();
    let _ = s.apply("border-width", &["0"]).unwrap();
    assert_eq!(s.get("border-width").unwrap(), "0");
    assert_eq!(s.borders.width, 0);
}

#[test]
fn border_color_accepts_hash_prefix() {
    let mut s = Settings::default();
    let _ = s.apply("border-color-focused", &["#80808080"]).unwrap();
    // Reported back as 0x...
    assert!(s.get("border-color-focused").unwrap().starts_with("0x"));
}

#[test]
fn user_config_colors_round_trip() {
    // Sanity-check that the actual values shipped in the nix module
    // survive parse → premultiply → format unchanged.
    let mut s = Settings::default();
    let _ = s.apply("border-color-focused", &["0xC8324BFF"]).unwrap();
    assert_eq!(s.get("border-color-focused").unwrap(), "0xC8324BFF");
    let _ = s.apply("border-color-unfocused", &["0x00C896FF"]).unwrap();
    assert_eq!(s.get("border-color-unfocused").unwrap(), "0x00C896FF");
}

#[test]
fn every_layout_key_is_accepted_by_apply() {
    // LAYOUT_KEYS is the public surface; Settings::apply must
    // accept every entry so bindings that fire `Action::Layout { key, .. }`
    // never lose to a "unknown command" reply mid-session.
    use crate::action::LAYOUT_KEYS;
    let cases: &[(&str, &str)] = &[
        ("main-ratio", "0.6"),
        ("main-count", "2"),
        ("gaps", "4"),
        ("outer-padding", "4"),
        ("orientation", "right"),
        ("smart-gaps", "off"),
        ("border-width", "5"),
        ("border-color-focused", "0xC8324BFF"),
        ("border-color-unfocused", "0x00C896FF"),
    ];
    // Verify cases cover LAYOUT_KEYS in entirety so a new key doesn't
    // silently slip through.
    let case_keys: Vec<&str> = cases.iter().map(|(k, _)| *k).collect();
    let const_keys: Vec<&str> = LAYOUT_KEYS.to_vec();
    assert_eq!(case_keys, const_keys);

    let mut s = Settings::default();
    for (key, value) in cases {
        let _ = s
            .apply(key, &[value])
            .unwrap_or_else(|e| panic!("apply({key}, {value}) failed: {e}"));
        // get should round-trip without erroring on every key too.
        let _ = s
            .get(key)
            .unwrap_or_else(|e| panic!("get({key}) failed: {e}"));
    }
}

#[test]
fn apply_unknown_key_leaves_settings_untouched() {
    // Dirty must reflect *real* state change. An error path must leave
    // dirty alone so the wayland thread doesn't redo a no-op manage.
    let mut s = Settings::default();
    let before = s.clone();
    assert!(s.apply("not-a-real-key", &["whatever"]).is_err());
    assert_eq!(s, before, "errors must leave settings untouched");
}

#[test]
fn border_width_no_op_reports_unchanged() {
    let mut s = Settings::default();
    let r = s.apply("border-width", &["3"]).unwrap(); // matches default
    assert!(!r.changed);
}

#[test]
fn relative_add_below_zero_saturates_at_zero_for_u32_fields() {
    let mut s = Settings::with_params(Params {
        gaps: 4,
        ..Params::default()
    });
    let _ = s.apply("gaps", &["-100"]).unwrap();
    assert_eq!(s.params.gaps, 0);
}

#[test]
fn bad_value_leaves_settings_untouched() {
    let mut s = Settings::default();
    let before = s.clone();
    assert!(s.apply("gaps", &["eight"]).is_err());
    assert!(s.apply("border-color-focused", &["0xZZ"]).is_err());
    assert_eq!(s, before);
}

#[test]
fn pixel_settings_are_clamped_to_sane_ranges() {
    let mut s = Settings::default();
    let _ = s.apply("border-width", &["2147483648"]).unwrap();
    assert_eq!(s.borders.width, super::MAX_BORDER_WIDTH);
    let _ = s.apply("gaps", &["4000000000"]).unwrap();
    assert_eq!(s.params.gaps, crate::layout::MAX_SPACING);
    let _ = s.apply("outer-padding", &["99999"]).unwrap();
    assert_eq!(s.params.outer_padding, crate::layout::MAX_SPACING);
    let _ = s.apply("main-count", &["4000000000"]).unwrap();
    assert_eq!(s.params.main_count, 1000);
}
