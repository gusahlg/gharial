//! End-to-end IPC tests: real Unix socket, real `gharial_ipc::send_one`.

use super::Server;
use crate::layout::Params;
use crate::state::Shared;
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
