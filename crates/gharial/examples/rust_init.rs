//! Worked example: an init script written in Rust.
//!
//! Build a binary out of this and use it instead of `config/init`:
//!
//! ```sh
//! cargo build --release --example rust_init
//! cp target/release/examples/rust_init ~/.config/river/init
//! chmod +x ~/.config/river/init
//! ```
//!
//! Then river will exec this binary as the session init. The binary
//! starts the daemon, waits for it, applies layout params, installs
//! every binding, and finally `wait`s on a child so river holds the
//! session open.
//!
//! Run with `--dry-run` to see the encoded action tokens without
//! talking to a daemon — useful for verifying your config before
//! pointing river at it.

use std::process::{Command, Stdio};
use std::time::Duration;

use gharial::{Action, BoolValue, Client, Color, Direction, Orientation};

const MOD: &str = "Super";

fn chord(suffix: &str) -> String {
    format!("{MOD}+{suffix}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    if !dry_run {
        // Daemon side-by-side: this binary launches gharial as a child
        // and lets it inherit our session. The reaper inside gharial
        // takes care of grandchildren spawned through `Client::spawn`.
        Command::new("gharial")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
    }

    let g = Client::new();
    if !dry_run {
        g.wait_until_ready(Duration::from_secs(2))?;
    }

    // ── Layout ───────────────────────────────────────────────────────
    let layout: Vec<Action> = vec![
        Action::set_gaps(8),
        Action::set_outer_padding(8),
        Action::set_main_ratio(0.55),
        Action::set_orientation(Orientation::Left),
        Action::set_smart_gaps(BoolValue::On),
        Action::set_border_width(3),
        Action::set_border_color_focused(Color::rgba(0xC8, 0x32, 0x4B, 0xFF)),
        Action::set_border_color_unfocused(Color::rgba(0x00, 0xC8, 0x96, 0xFF)),
    ];
    for a in layout {
        run(&g, dry_run, None, a)?;
    }

    // ── Bindings ─────────────────────────────────────────────────────
    let bindings: Vec<(String, Action)> = vec![
        (chord("Q"), Action::Close),
        (chord("Return"), Action::spawn("rio", [] as [&str; 0])),
        (chord("T"), Action::spawn("qutebrowser", [] as [&str; 0])),
        (chord("E"), Action::spawn("thunar", [] as [&str; 0])),
        (chord("L"), Action::focus(Direction::Next)),
        (chord("H"), Action::focus(Direction::Prev)),
        (chord("Shift+L"), Action::swap(Direction::Next)),
        (chord("Shift+H"), Action::swap(Direction::Prev)),
        (chord("Space"), Action::ToggleFloat),
        (chord("F"), Action::ToggleFullscreen),
        // Resize mode: enter via Super+R, leave via Escape or Super+R.
        (chord("R"), Action::enter_mode("resize")),
    ];
    for (chord, action) in &bindings {
        bind(&g, dry_run, None, chord, action.clone())?;
    }

    // ── Resize mode ──────────────────────────────────────────────────
    let mod_r = chord("R");
    let resize_bindings: Vec<(&str, Action)> = vec![
        ("Escape", Action::ExitMode),
        ("L", Action::adjust_main_ratio(0.05)),
        ("H", Action::adjust_main_ratio(-0.05)),
        (mod_r.as_str(), Action::ExitMode),
    ];
    for (chord_str, action) in resize_bindings {
        bind(&g, dry_run, Some("resize"), chord_str, action)?;
    }

    // ── Tags 1..10, with `0` keying tag 10 ──────────────────────────
    for n in 1..=10u8 {
        let key = if n == 10 { "0".to_string() } else { n.to_string() };
        bind(&g, dry_run, None, &chord(&key), Action::FocusTag(n))?;
        bind(
            &g,
            dry_run,
            None,
            &chord(&format!("Shift+{key}")),
            Action::MoveToTag(n),
        )?;
    }

    // ── Autostart ────────────────────────────────────────────────────
    if !dry_run {
        g.spawn("waybar", &[] as &[&str])?;
    } else {
        println!("[dry-run] spawn waybar");
    }

    Ok(())
}

fn run(
    g: &Client,
    dry_run: bool,
    mode: Option<&str>,
    action: Action,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run {
        let prefix = mode.map(|m| format!("[{m}] ")).unwrap_or_default();
        println!("[dry-run] {prefix}execute {:?}", action.to_tokens());
        Ok(())
    } else {
        g.execute(action)?;
        Ok(())
    }
}

fn bind(
    g: &Client,
    dry_run: bool,
    mode: Option<&str>,
    chord: &str,
    action: Action,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run {
        let prefix = mode.map(|m| format!("[{m}] ")).unwrap_or_default();
        println!(
            "[dry-run] {prefix}bind {chord} {:?}",
            action.to_tokens()
        );
        Ok(())
    } else {
        match mode {
            Some(m) => g.bind_in_mode(m, chord, action)?,
            None => g.bind(chord, action)?,
        }
        Ok(())
    }
}
