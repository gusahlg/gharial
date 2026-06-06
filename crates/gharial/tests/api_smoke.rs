//! External-style integration tests for the public Rust API.
//!
//! Unlike `src/ipc/tests.rs` these only see the `gharial` library's
//! public surface — they're compiled as a separate crate, so they
//! catch any item that accidentally relies on `pub(crate)` visibility.
//!
//! No daemon is started here; we just exercise the parts of the API
//! that don't touch a socket. The socket-driven path is covered by the
//! in-crate `ipc::tests` module that can construct a real `Server`.

use gharial::{Action, BindingSpec, BoolValue, Client, Color, Direction, Orientation};

#[test]
fn library_re_exports_resolve_at_the_crate_root() {
    let _: Action = Action::Close;
    let _: BindingSpec = BindingSpec::parse("Super+Q").unwrap();
    let _: Direction = Direction::Next;
    let _: Orientation = Orientation::Left;
    let _: BoolValue = BoolValue::Toggle;
    let _: Color = Color::rgb(0, 0, 0);
    let _: Client = Client::with_socket("/tmp/never-used.sock");
}

#[test]
fn client_socket_is_observable() {
    let c = Client::with_socket("/tmp/observed.sock");
    assert_eq!(c.socket().to_str(), Some("/tmp/observed.sock"));
}

#[test]
fn end_user_can_compose_a_full_init_script_in_rust() {
    // This is the "shape of life" test — it doesn't actually run
    // against a daemon, it just confirms that every method needed for
    // a realistic init compiles and produces the right Action shape.
    let mod_super = "Super";
    let chord = |k: &str| format!("{mod_super}+{k}");

    let bindings: Vec<(String, Action)> = vec![
        (chord("Q"), Action::Close),
        (chord("Return"), Action::spawn("rio", [] as [&str; 0])),
        (chord("T"), Action::spawn("qutebrowser", [] as [&str; 0])),
        (chord("L"), Action::focus(Direction::Next)),
        (chord("H"), Action::focus(Direction::Prev)),
        (chord("Shift+L"), Action::swap(Direction::Next)),
        (chord("Space"), Action::ToggleFloat),
        (chord("1"), Action::FocusTag(1)),
        (chord("Shift+1"), Action::MoveToTag(1)),
        (chord("Ctrl+1"), Action::ToggleTag(1)),
        (chord("R"), Action::enter_mode("resize")),
    ];
    // Every action must yield a non-empty token list (Bind/Unbind would
    // give empty; we don't put those in bindings).
    for (chord, action) in &bindings {
        let tokens = action.to_tokens();
        assert!(
            !tokens.is_empty(),
            "{chord}'s action {action:?} produced no tokens"
        );
        // And every binding chord must itself parse.
        BindingSpec::parse(chord).expect(chord);
    }

    // Layout commands.
    let layout: Vec<Action> = vec![
        Action::set_gaps(8),
        Action::set_outer_padding(8),
        Action::set_main_ratio(0.55),
        Action::adjust_main_ratio(0.05),
        Action::set_orientation(Orientation::Left),
        Action::set_smart_gaps(BoolValue::On),
        Action::set_border_width(3),
        Action::set_border_color_focused(Color::rgba(0xC8, 0x32, 0x4B, 0xFF)),
        Action::set_border_color_unfocused(Color::rgba(0x00, 0xC8, 0x96, 0xFF)),
    ];
    for a in &layout {
        let tokens = a.to_tokens();
        assert!(!tokens.is_empty(), "{a:?} produced no tokens");
        assert!(
            tokens.iter().all(|t| !t.is_empty()),
            "{a:?} produced empty token: {tokens:?}"
        );
    }
}

#[test]
fn color_from_u32_matches_wire_form() {
    // The library's u32 → Color conversion needs to use 0xRRGGBBAA
    // byte order; a regression would silently mis-translate user
    // configs.
    let c: Color = 0xC8324BFF_u32.into();
    assert_eq!(c, Color::rgba(0xC8, 0x32, 0x4B, 0xFF));
    assert_eq!(c.to_hex_string(), "0xC8324BFF");
}

#[test]
fn orientation_str_round_trips_through_action_layout() {
    for o in [
        Orientation::Left,
        Orientation::Right,
        Orientation::Top,
        Orientation::Bottom,
    ] {
        let a = Action::set_orientation(o);
        let tokens = a.to_tokens();
        assert_eq!(tokens[0], "orientation");
        assert_eq!(tokens[1], o.as_str());
    }
}
