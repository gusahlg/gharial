//! End-to-end IPC tests: real Unix socket, real `gharial_ipc::send_one`.
//! Also covers the public `gharial::Client` against the same server.

use super::Server;
use crate::layout::Params;
use crate::state::Shared;
use gharial::{Action, BoolValue, Client, Color, Direction, Orientation};
use gharial_ipc::{send_one, Request, Response};
use std::path::PathBuf;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn temp_socket(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("gharial-test-{}-{}.sock", std::process::id(), name));
    p
}

#[test]
fn ping_returns_pong() {
    let path = temp_socket("ping");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();
    let resp = send_one(&path, &Request::new("ping", vec![])).unwrap();
    assert_eq!(resp, Response::ok("pong"));
}

#[test]
fn set_then_get_roundtrip() {
    let path = temp_socket("setget");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();

    let resp = send_one(
        &path,
        &Request::new("set", vec!["gaps".into(), "12".into()]),
    )
    .unwrap();
    assert!(resp.is_ok(), "set failed: {resp:?}");
    let resp = send_one(&path, &Request::new("get", vec!["gaps".into()])).unwrap();
    assert_eq!(resp, Response::ok("12"));
}

#[test]
fn unknown_command_errors() {
    let path = temp_socket("unknown");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();
    let resp = send_one(&path, &Request::new("frobnicate", vec![])).unwrap();
    assert!(!resp.is_ok(), "expected error, got {resp:?}");
}

#[test]
fn status_lists_all_keys() {
    let path = temp_socket("status");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();
    let resp = send_one(&path, &Request::new("status", vec![])).unwrap();
    let body = match resp {
        Response::Ok(b) => b,
        other => panic!("expected ok, got {other:?}"),
    };
    for key in [
        "main-ratio",
        "main-count",
        "gaps",
        "outer-padding",
        "orientation",
        "smart-gaps",
    ] {
        assert!(body.contains(key), "missing {key} in {body}");
    }
}

#[test]
fn shorthand_layout_key_routes_to_apply() {
    // The dispatcher's `is_layout_key`-driven shorthand must accept the
    // exact same vocabulary as `set`. We exercise both paths and assert
    // they produce identical state.
    let path = temp_socket("shorthand");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();

    let r1 = send_one(
        &path,
        &Request::new("set", vec!["gaps".into(), "11".into()]),
    )
    .unwrap();
    assert!(r1.is_ok());
    let r2 = send_one(&path, &Request::new("gaps", vec!["+1".into()])).unwrap();
    assert!(r2.is_ok());
    let r3 = send_one(&path, &Request::new("get", vec!["gaps".into()])).unwrap();
    assert_eq!(r3, Response::ok("12"));
}

#[test]
fn focus_without_args_returns_explicit_error_message() {
    let path = temp_socket("focus-err");
    let shared = Shared::new(Params::default());
    let _server = Server::start_at(path.clone(), shared, None).unwrap();

    let resp = send_one(&path, &Request::new("focus", vec![])).unwrap();
    let body = match resp {
        Response::Err(b) => b,
        other => panic!("expected err, got {other:?}"),
    };
    assert!(body.contains("focus"));
    assert!(body.contains("next"));
    assert!(body.contains("left"));
    assert!(body.contains("down"));
}

// ─────────────────────────────────────────────────────────────────────
// `gharial::Client` integration tests — exercise the public Rust API
// against the same real server. They confirm that every typed method
// produces the wire request the daemon already knows how to handle.

fn spawn_server(name: &str) -> (Client, Server, Shared) {
    let path = temp_socket(name);
    let shared = Shared::new(Params::default());
    let server = Server::start_at(path.clone(), shared.clone(), None).unwrap();
    (Client::with_socket(path), server, shared)
}

#[test]
fn client_ping_succeeds() {
    let (client, _server, _shared) = spawn_server("client-ping");
    client.ping().expect("ping");
}

#[test]
fn client_version_returns_a_non_empty_string() {
    let (client, _server, _shared) = spawn_server("client-version");
    let v = client.version().expect("version");
    assert!(!v.is_empty());
}

#[test]
fn client_wait_until_ready_succeeds_when_daemon_is_up() {
    let (client, _server, _shared) = spawn_server("client-wait-ready");
    client
        .wait_until_ready(Duration::from_secs(1))
        .expect("ready");
}

#[test]
fn client_wait_until_ready_times_out_against_an_empty_socket() {
    let path = temp_socket("client-wait-timeout");
    // No server bound — the connect attempts in `ping` will keep failing.
    let client = Client::with_socket(path);
    let err = client
        .wait_until_ready(Duration::from_millis(150))
        .unwrap_err();
    assert!(matches!(err, gharial::Error::Timeout { .. }));
}

#[test]
fn client_set_gaps_round_trips_through_get() {
    let (client, _server, _shared) = spawn_server("client-gaps");
    client.set_gaps(11).expect("set");
    assert_eq!(client.get("gaps").unwrap(), "11");
    client.adjust_gaps(4).expect("adjust");
    assert_eq!(client.get("gaps").unwrap(), "15");
    client.adjust_gaps(-100).expect("saturating");
    assert_eq!(client.get("gaps").unwrap(), "0");
}

#[test]
fn client_set_main_ratio_round_trips() {
    let (client, _server, _shared) = spawn_server("client-ratio");
    client.set_main_ratio(0.6).expect("set");
    let v: f32 = client.get("main-ratio").unwrap().parse().unwrap();
    assert!((v - 0.6).abs() < 1e-3);
    client.adjust_main_ratio(0.05).expect("adjust");
    let v: f32 = client.get("main-ratio").unwrap().parse().unwrap();
    assert!((v - 0.65).abs() < 1e-3);
}

#[test]
fn client_set_orientation_round_trips() {
    let (client, _server, _shared) = spawn_server("client-orientation");
    client.set_orientation(Orientation::Right).expect("set");
    assert_eq!(client.get("orientation").unwrap(), "right");
    client.set_orientation(Orientation::Bottom).expect("set");
    assert_eq!(client.get("orientation").unwrap(), "bottom");
}

#[test]
fn client_set_smart_gaps_handles_bool_and_toggle() {
    let (client, _server, _shared) = spawn_server("client-smart-gaps");
    client.set_smart_gaps(false).expect("set off");
    assert_eq!(client.get("smart-gaps").unwrap(), "false");
    client.set_smart_gaps(BoolValue::Toggle).expect("toggle");
    assert_eq!(client.get("smart-gaps").unwrap(), "true");
}

#[test]
fn client_set_border_color_round_trips() {
    let (client, _server, _shared) = spawn_server("client-border");
    let c = Color::rgba(0xC8, 0x32, 0x4B, 0xFF);
    client.set_border_color_focused(c).expect("focused");
    assert_eq!(client.get("border-color-focused").unwrap(), "0xC8324BFF");
    // Tuple conversion works too.
    client
        .set_border_color_unfocused((0x00, 0xC8, 0x96, 0xFF))
        .expect("unfocused");
    assert_eq!(client.get("border-color-unfocused").unwrap(), "0x00C896FF");
}

#[test]
fn client_status_lists_all_keys() {
    let (client, _server, _shared) = spawn_server("client-status");
    let line = client.status().unwrap();
    for key in [
        "main-ratio",
        "main-count",
        "gaps",
        "outer-padding",
        "orientation",
        "smart-gaps",
        "border-width",
        "border-color-focused",
        "border-color-unfocused",
    ] {
        assert!(line.contains(key), "missing {key} in {line}");
    }
}

#[test]
fn client_get_unknown_key_returns_daemon_error() {
    let (client, _server, _shared) = spawn_server("client-bogus");
    let err = client.get("not-a-key").unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_focus_swap_queue_through_action_channel() {
    // No wayland thread is wired up, so the daemon returns an error
    // about the action channel — but the Client must still produce a
    // request that REACHES the dispatcher (i.e. doesn't fail
    // client-side).
    let (client, _server, _shared) = spawn_server("client-focus");
    let err = client.focus(Direction::Next).unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
    let err = client.swap(Direction::Left).unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
    let err = client.close().unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_tag_methods_route_to_tag_verb() {
    let (client, _server, _shared) = spawn_server("client-tag");
    // Same as above: no wayland thread = daemon error, but every
    // method must reach the dispatcher.
    for r in [
        client.tag_focus(1),
        client.tag_toggle(2),
        client.tag_move(3),
        client.tag_window_toggle(4),
    ] {
        assert!(matches!(r.unwrap_err(), gharial::Error::Daemon(_)));
    }
}

#[test]
fn client_output_methods_route_to_output_verb() {
    let (client, _server, _shared) = spawn_server("client-output");
    // No wayland thread = daemon error from the action channel, but
    // every method must survive parsing and reach the dispatcher.
    for r in [
        client.focus_output(Direction::Next),
        client.focus_output_named("DP-2"),
        client.send_to_output(Direction::Right),
        client.set_output_focus_warp(false),
    ] {
        let err = r.unwrap_err();
        assert!(matches!(err, gharial::Error::Daemon(msg) if msg.contains("wayland")));
    }
}

#[test]
fn output_list_answers_without_a_wayland_thread() {
    // The mirror starts empty; the verb must still answer ok.
    let (client, _server, _shared) = spawn_server("output-list");
    assert_eq!(client.list_outputs().unwrap(), "no outputs");
}

#[test]
fn output_list_reflects_the_shared_mirror() {
    use crate::state::{OutputInfo, OutputsInfo};
    let (client, _server, shared) = spawn_server("output-mirror");
    shared.set_outputs_info(OutputsInfo {
        outputs: vec![
            OutputInfo {
                name: "DP-1".into(),
                position: (0, 0),
                dimensions: (1920, 1080),
                active_tags: 0x1,
                focused: true,
            },
            OutputInfo {
                name: "DP-2".into(),
                position: (1920, 0),
                dimensions: (2560, 1440),
                active_tags: 0x2,
                focused: false,
            },
        ],
    });
    let listed = client.list_outputs().unwrap();
    assert!(listed.contains("DP-1 1920x1080+0+0 tags=0x00000001 focused"));
    assert!(listed.contains("DP-2 2560x1440+1920+0 tags=0x00000002"));
    assert!(!listed.contains("link"));
}

#[test]
fn output_verb_rejects_malformed_arguments() {
    let (client, _server, _shared) = spawn_server("output-bad");
    let path = client.socket().to_path_buf();
    for args in [
        vec![],
        vec!["focus".to_string()],
        vec!["focus-warp".to_string()],
        vec!["focus-warp".to_string(), "maybe".to_string()],
        vec!["link".to_string(), "DP-1:right".to_string()],
    ] {
        let resp = send_one(&path, &Request::new("output", args.clone())).unwrap();
        assert!(!resp.is_ok(), "expected error for output {args:?}");
    }
}

#[test]
fn client_spawn_routes_to_spawn_verb_with_correct_args() {
    let (client, _server, _shared) = spawn_server("client-spawn");
    let err = client.spawn("rio", &["-e", "nvim"]).unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
    // Empty args list compiles (the AsRef bound + slice form):
    let err = client.spawn("waybar", &[] as &[&str]).unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_bind_packages_chord_and_action_tokens() {
    let (client, _server, _shared) = spawn_server("client-bind");
    // The wayland thread isn't running, so the IPC server can't deliver
    // the Bind through the action channel; the call still reaches the
    // dispatcher (which would chord-parse + validate the action), and
    // the chord/action parse succeeds, so we see a "wayland thread not
    // ready" Daemon error — proof the request packing worked.
    let err = client.bind("Super+Q", Action::Close).unwrap_err();
    match err {
        gharial::Error::Daemon(msg) => assert!(
            msg.contains("not ready") || msg.contains("not accepting"),
            "unexpected daemon error: {msg}"
        ),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn client_bind_rejects_a_bad_chord_without_routing_to_action_channel() {
    let (client, _server, _shared) = spawn_server("client-bind-bad-chord");
    let err = client.bind("Hyper+NotARealKey", Action::Close).unwrap_err();
    match err {
        gharial::Error::Daemon(msg) => {
            assert!(msg.contains("modifier") || msg.contains("keysym") || msg.contains("Hyper"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn client_bind_in_mode_includes_mode_flag() {
    let (client, _server, _shared) = spawn_server("client-bind-mode");
    let err = client
        .bind_in_mode("resize", "Escape", Action::ExitMode)
        .unwrap_err();
    // Same expected error class as the default-mode bind.
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_unbind_round_trips() {
    let (client, _server, _shared) = spawn_server("client-unbind");
    // Even without anything bound, unbind reaches the dispatcher.
    let err = client.unbind("Super+Q").unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
    let err = client.unbind_in_mode("resize", "Escape").unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_raw_escape_hatch_works() {
    let (client, _server, _shared) = spawn_server("client-raw");
    // `raw` accepts any verb the dispatcher knows. Ping is the simplest.
    let body = client.raw("ping", [] as [&str; 0]).unwrap();
    assert_eq!(body, "pong");
    // Unknown verb propagates as Daemon error.
    let err = client.raw("frobnicate", ["x"]).unwrap_err();
    assert!(matches!(err, gharial::Error::Daemon(_)));
}

#[test]
fn client_set_is_an_alias_for_layout_apply() {
    let (client, _server, _shared) = spawn_server("client-set");
    client.set("gaps", "7").expect("set");
    assert_eq!(client.get("gaps").unwrap(), "7");
}

#[test]
fn client_execute_dispatches_action_via_its_canonical_verb() {
    let (client, _server, _shared) = spawn_server("client-execute");
    // Execute via the generic Action path — same wire shape as the
    // typed method. We compare effect via get.
    client.execute(Action::set_gaps(13)).expect("execute set");
    assert_eq!(client.get("gaps").unwrap(), "13");

    // Adjust via execute too.
    client
        .execute(Action::adjust_gaps(3))
        .expect("execute adjust");
    assert_eq!(client.get("gaps").unwrap(), "16");
}

#[test]
fn client_execute_rejects_bind_unbind_actions_client_side() {
    let (client, _server, _shared) = spawn_server("client-execute-bind");
    let bind = Action::Bind {
        spec: gharial::BindingSpec {
            modifiers: 0,
            keysym: 0,
        },
        action: Box::new(Action::Close),
        mode: "default".into(),
    };
    let err = client.execute(bind).unwrap_err();
    assert!(
        matches!(err, gharial::Error::Daemon(msg) if msg.contains("Bind/Unbind") || msg.contains("no tokens"))
    );
}

#[test]
fn ipc_set_marks_state_dirty_and_notifies() {
    let path = temp_socket("dirty");
    let shared = Shared::new(Params::default());
    shared.take_dirty(); // start clean
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_cb = count.clone();
    let notifier: super::Notifier = Arc::new(move || {
        count_for_cb.fetch_add(1, Ordering::SeqCst);
    });
    let _server = Server::start_at(path.clone(), shared.clone(), Some(notifier)).unwrap();

    // No-op set: no notify expected.
    let resp = send_one(&path, &Request::new("set", vec!["gaps".into(), "8".into()])).unwrap();
    assert!(resp.is_ok());

    // Real change: notify expected, dirty flag set.
    let resp = send_one(
        &path,
        &Request::new("set", vec!["gaps".into(), "20".into()]),
    )
    .unwrap();
    assert!(resp.is_ok());
    assert!(
        shared.take_dirty(),
        "ipc-driven set must set the dirty flag"
    );

    // Wait briefly for the notify to be delivered from the IPC thread.
    let deadline = Instant::now() + Duration::from_millis(500);
    while count.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "notifier should fire exactly once for a real change"
    );
}
