//! A gharial config written against the dependency-light `gharial-ipc`
//! crate — no Wayland in the build graph, and the compile-time-checked
//! `ratio!` / `tag!` / `chord!` macros catch bad values before the
//! binary ever runs.
//!
//! Run it against a live daemon with `cargo run --example typed_config`.
//! Try changing `ratio!(0.55)` to `ratio!(1.5)`, or `chord!("Super+Q")`
//! to `chord!("Supr+Q")`, and watch it fail to *compile*.

use std::time::Duration;

use gharial_ipc::config::{Bindings, Config, Layout};
use gharial_ipc::{chord, ratio, tag, Action, Color, Direction, Orientation};

fn main() -> gharial_ipc::Result<()> {
    let g = gharial_ipc::Client::new();
    g.wait_until_ready(Duration::from_secs(2))?;

    let layout = Layout::new()
        .main_ratio(ratio!(0.55))
        .main_count(1)
        .gaps(8)
        .outer_padding(8)
        .orientation(Orientation::Left)
        .smart_gaps(true)
        .border_width(3)
        .border_color_focused(Color::rgb(0xC8, 0x32, 0x4B))
        .border_color_unfocused(Color::rgb(0x00, 0xC8, 0x96));

    let mut bindings = Bindings::new()
        .bind(chord!("Super+Q"), Action::Close)
        .bind(chord!("Super+Space"), Action::ToggleFloat)
        .bind(chord!("Super+F"), Action::ToggleFullscreen)
        .bind(
            chord!("Super+Return"),
            Action::spawn("rio", [] as [&str; 0]),
        )
        .bind(chord!("Super+L"), Action::focus(Direction::Next))
        .bind(chord!("Super+H"), Action::focus(Direction::Prev))
        .bind(chord!("Super+Shift+L"), Action::swap(Direction::Next))
        // A ratio-tweak mode: enter with Super+R, leave with Escape.
        .bind(chord!("Super+R"), Action::enter_mode("resize"))
        .bind_in_mode("resize", chord!("Escape"), Action::ExitMode)
        .bind_in_mode("resize", chord!("L"), Action::adjust_main_ratio(0.05))
        .bind_in_mode("resize", chord!("H"), Action::adjust_main_ratio(-0.05));

    // Tags 1..=9 focus, Super+Shift+N sends the focused window there.
    // `tag!(n)` is checked at compile time, so an out-of-range tag here
    // would not build.
    for (key, t) in [
        (chord!("Super+1"), tag!(1)),
        (chord!("Super+2"), tag!(2)),
        (chord!("Super+3"), tag!(3)),
    ] {
        bindings = bindings.bind(key, t.focus());
    }

    Config::new()
        .layout(layout)
        .bindings(bindings)
        .spawn(["waybar"])
        .spawn([
            "swaybg",
            "-i",
            "/usr/share/backgrounds/default.png",
            "-m",
            "fill",
        ])
        .apply(&g)?;

    Ok(())
}
