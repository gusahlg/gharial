//! End-to-end IPC tests: a real Unix socket, real `gharial_ipc` clients,
//! and a stand-in for the wayland thread that answers settings commands
//! with the real `Settings` store (anything needing a compositor gets an
//! error, which still proves the request was parsed and routed).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use gharial::{Action, BoolValue, Client, Color, Direction, Orientation};
use gharial_ipc::{send_one, Command, Request, Response};

use super::Server;
use crate::bridge::{Bridge, Envelope};
use crate::settings::Settings;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp_socket(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    p.push(format!(
        "gharial-test-{}-{n}-{name}.sock",
        std::process::id()
    ));
    p
}

/// Answer envelopes the way the wayland thread would, minus a compositor.
fn fake_wm(settings: &mut Settings, Envelope { command, reply }: Envelope) {
    match command {
        Command::Status => reply.ok(settings.status_line()),
        Command::Get(key) => reply.result(settings.get(&key)),
        Command::Action(Action::Layout { key, args }) => {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            reply.result(settings.apply(&key, &args).map(|a| a.summary));
        }
        Command::WaitWindow { timeout, .. } => {
            // Hold the request like a real wait would, then time out.
            thread::spawn(move || {
                thread::sleep(timeout);
                reply.err("timed out");
            });
        }
        other => reply.err(format!("no compositor in tests: {other:?}")),
    }
}

fn spawn_server(name: &str) -> (Client, Server) {
    let (tx, rx) = calloop::channel::channel::<Envelope>();
    thread::spawn(move || {
        let mut event_loop: calloop::EventLoop<Settings> = calloop::EventLoop::try_new().unwrap();
        event_loop
            .handle()
            .insert_source(rx, |event, _, settings| {
                if let calloop::channel::Event::Msg(env) = event {
                    fake_wm(settings, env);
                }
            })
            .unwrap();
        let mut settings = Settings::default();
        loop {
            if event_loop.dispatch(None, &mut settings).is_err() {
                break;
            }
        }
    });
    let path = temp_socket(name);
    let server = Server::start_at(path.clone(), Bridge::new(tx)).unwrap();
    (Client::with_socket(path), server)
}

fn raw(client: &Client, line: &str) -> Response {
    send_one(client.socket(), &Request::parse(line).unwrap()).unwrap()
}

/// Send bytes as-is, for requests no well-behaved client would encode.
fn raw_line(client: &Client, line: &str) -> Response {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(client.socket()).unwrap();
    stream.write_all(line.as_bytes()).unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    Response::parse(&reply).unwrap()
}

#[test]
fn ping_and_version_need_no_wayland_thread() {
    let (tx, _rx) = calloop::channel::channel::<Envelope>();
    let path = temp_socket("ping");
    let _server = Server::start_at(path.clone(), Bridge::new(tx)).unwrap();
    assert_eq!(
        send_one(&path, &Request::new("ping", vec![])).unwrap(),
        Response::ok("pong")
    );
    let v = send_one(&path, &Request::new("version", vec![])).unwrap();
    assert_eq!(v, Response::ok(env!("CARGO_PKG_VERSION")));
}

#[test]
fn set_get_status_round_trip() {
    let (client, _server) = spawn_server("setget");
    assert!(raw(&client, "set gaps 12").is_ok());
    assert_eq!(raw(&client, "get gaps"), Response::ok("12"));
    assert!(raw(&client, "gaps +1").is_ok(), "layout keys work as verbs");
    assert_eq!(raw(&client, "get gaps"), Response::ok("13"));
    let status = client.status().unwrap();
    for key in gharial_ipc::action::LAYOUT_KEYS {
        assert!(status.contains(key), "missing {key} in {status}");
    }
}

#[test]
fn malformed_requests_get_specific_errors() {
    let (client, _server) = spawn_server("errors");
    let err = raw(&client, "frobnicate");
    assert!(err.body().contains("unknown command"), "{err:?}");
    let err = raw(&client, "focus");
    for dir in ["next", "left", "down"] {
        assert!(err.body().contains(dir), "{err:?}");
    }
    assert!(!raw(&client, "set nope 1").is_ok());
    assert!(!raw(&client, "output").is_ok());
    assert!(!raw(&client, "output focus-warp maybe").is_ok());
    assert!(!raw(&client, "rule add float").is_ok());
    assert!(!raw(&client, "window list bogus").is_ok());
    assert!(raw_line(&client, "spawn \"unterminated\n")
        .body()
        .contains("unterminated"));
    // A bad value is rejected by the settings store, not swallowed.
    assert!(!raw(&client, "set gaps lots").is_ok());
    assert_eq!(raw(&client, "get gaps"), Response::ok("8"));
}

#[test]
fn client_layout_methods_round_trip() {
    let (client, _server) = spawn_server("client-layout");
    client.set_gaps(11).unwrap();
    assert_eq!(client.get("gaps").unwrap(), "11");
    client.adjust_gaps(4).unwrap();
    assert_eq!(client.get("gaps").unwrap(), "15");
    client.adjust_gaps(-100).unwrap();
    assert_eq!(client.get("gaps").unwrap(), "0");

    client.set_main_ratio(0.6).unwrap();
    client.adjust_main_ratio(0.05).unwrap();
    let ratio: f32 = client.get("main-ratio").unwrap().parse().unwrap();
    assert!((ratio - 0.65).abs() < 1e-3);

    client.set_orientation(Orientation::Bottom).unwrap();
    assert_eq!(client.get("orientation").unwrap(), "bottom");
    client.set_smart_gaps(false).unwrap();
    client.set_smart_gaps(BoolValue::Toggle).unwrap();
    assert_eq!(client.get("smart-gaps").unwrap(), "true");

    client
        .set_border_color_focused(Color::rgba(0xC8, 0x32, 0x4B, 0xFF))
        .unwrap();
    assert_eq!(client.get("border-color-focused").unwrap(), "0xC8324BFF");
    client.set("border-width", "5").unwrap();
    assert_eq!(client.get("border-width").unwrap(), "5");
    client.execute(Action::set_gaps(13)).unwrap();
    assert_eq!(client.get("gaps").unwrap(), "13");
}

#[test]
fn compositor_commands_reach_the_wayland_thread() {
    let (client, _server) = spawn_server("routing");
    for result in [
        client.focus(Direction::Next),
        client.close(),
        client.tag_focus(3),
        client.focus_output_named("DP-2"),
        client.bind("Super+Q", Action::Close),
        client.unbind_in_mode("resize", "Escape"),
    ] {
        let err = result.unwrap_err();
        assert!(
            matches!(&err, gharial::Error::Daemon(msg) if msg.contains("no compositor")),
            "{err}"
        );
    }
}

#[test]
fn bad_chords_are_rejected_before_sending() {
    let (client, _server) = spawn_server("chords");
    assert!(matches!(
        client.bind("Hyper+Q", Action::Close).unwrap_err(),
        gharial::Error::Invalid(_)
    ));
    // The same chord over the raw wire fails in the daemon's parser.
    assert!(raw(&client, "bind Hyper+Q close")
        .body()
        .contains("modifier"));
}

#[test]
fn a_blocked_request_does_not_stall_other_clients() {
    let (client, _server) = spawn_server("concurrency");
    let waiter = client.clone();
    let handle = thread::spawn(move || {
        waiter.wait_window(
            gharial::WindowSelector::Match(gharial::Matcher::app_id("never")),
            Duration::from_secs(3),
            false,
        )
    });
    thread::sleep(Duration::from_millis(50));
    client.ping().unwrap();
    client.set_gaps(3).unwrap();
    // Both answered while the wait is still pending — no timing margin
    // to flake on a loaded build machine.
    assert!(
        !handle.is_finished(),
        "other requests must not queue behind a wait"
    );
    assert!(handle.join().unwrap().is_err());
}

#[test]
fn wait_until_ready_times_out_against_an_absent_daemon() {
    let client = Client::with_socket(temp_socket("absent"));
    let err = client
        .wait_until_ready(Duration::from_millis(150))
        .unwrap_err();
    assert!(matches!(err, gharial::Error::Timeout { .. }));
}

#[test]
fn a_second_daemon_refuses_a_live_socket_but_replaces_a_stale_one() {
    let (client, server) = spawn_server("exclusive");
    let (tx, _rx) = calloop::channel::channel::<Envelope>();
    let err = match Server::start_at(server.socket_path.clone(), Bridge::new(tx)) {
        Ok(_) => panic!("a second server must not take a live socket"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    client.ping().unwrap();

    // A leftover socket file with nobody listening is taken over.
    let stale = temp_socket("stale");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    let (tx, _rx) = calloop::channel::channel::<Envelope>();
    let _server = Server::start_at(stale.clone(), Bridge::new(tx)).unwrap();
    assert!(Client::with_socket(stale).ping().is_ok());
}
