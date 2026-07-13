//! gharial: a minimal master-stack layout generator for the river
//! Wayland compositor.
//!
//! The binary entry point lives here; the public Rust API (`Action`,
//! `Client`, …) lives in the `gharial` library crate alongside this
//! binary. The wire-format vocabulary is shared, so re-exporting the
//! lib modules at this crate root keeps daemon-internal call sites
//! using `crate::action::*` / `crate::keysyms::*` paths unchanged.

mod ipc;
mod layout;
mod state;
mod wayland_proto;
mod wm;

// Single source of truth: the action vocabulary, keysym table, and
// value-typed helpers all live in the library. Re-export them so the
// daemon's internal modules keep their existing `crate::*` paths.
pub use gharial::{action, color, edge, keysyms, orientation, value};

use std::process::ExitCode;

use layout::Params;
use state::Shared;

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

    let shared = Shared::new(Params::default());
    if let Err(e) = wm::run(shared) {
        eprintln!("gharial: wm loop exited with error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        "gharial {version} - master-stack layout daemon for river

Usage: gharial [OPTIONS]

Options:
  -h, --help       Show this help and exit
  -V, --version    Show version and exit

Tuning at runtime:
  gharialctl set main-ratio 0.55
  gharialctl set gaps 8

IPC socket: $GHARIAL_SOCKET, or $XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock",
        version = env!("CARGO_PKG_VERSION"),
    );
}
