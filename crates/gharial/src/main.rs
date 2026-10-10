//! gharial: a window manager for the river Wayland compositor.
//!
//! The binary entry point lives here; the public Rust API (`Action`,
//! `Client`, …) lives in the `gharial` library crate alongside it. The
//! wire vocabulary is shared, so re-exporting the lib modules at this
//! crate root keeps daemon-internal `crate::action::*` paths working.

mod bridge;
mod ipc;
mod layout;
mod settings;
mod wayland_proto;
mod wm;

pub use gharial::{action, keysyms};

use std::process::ExitCode;

use settings::Settings;

fn main() -> ExitCode {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("gharial {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("gharial: unknown argument: {other}\n");
                print_help();
                return ExitCode::from(2);
            }
        }
    }

    if let Err(e) = wm::run(Settings::default()) {
        eprintln!("gharial: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        "gharial {version} - window manager for river

Usage: gharial [OPTIONS]

Options:
  -h, --help       Show this help and exit
  -V, --version    Show version and exit

Configure and control it at runtime with gharialctl, e.g.:
  gharialctl set main-ratio 0.55
  gharialctl rule add app-id=pavucontrol float size 800x600
  gharialctl window list

IPC socket: $GHARIAL_SOCKET, or $XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock",
        version = env!("CARGO_PKG_VERSION"),
    );
}
