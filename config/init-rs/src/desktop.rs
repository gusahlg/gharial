//! The desktop policy — the part that should be edited for a machine.
//!
//! The policy is built with gharial-ipc's typed configuration API. Invalid
//! ratios, tags, and literal chords therefore fail during compilation rather
//! than becoming broken startup commands.

use std::env;

use gharial_ipc::config::{Bindings, Config, Layout};
use gharial_ipc::{chord, ratio, Action, Client, Color, Direction};

const MOD: &str = "Super";

pub fn configure(client: &Client) -> Result<(), String> {
    let home = env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    build_config(&home)
        .apply(client)
        .map_err(|error| error.to_string())
}

fn build_config(home: &str) -> Config {
    let notes = format!("{home}/Notes");
    let night_light = format!("{home}/.local/share/night-light");
    let rec = format!("{home}/.local/share/rec");
    let rec_stop = format!("{home}/.local/share/rec-stop");
    let wallpaper = format!("{home}/Pictures/doctor_nath.png");

    let mut tofi_args = vec![
        "--drun-launch=true".to_string(),
        "--height".to_string(),
        "1000".to_string(),
        "--width".to_string(),
        "500".to_string(),
        "--font-size".to_string(),
        "12".to_string(),
    ];
    if let Ok(font) = env::var("FONT") {
        tofi_args.splice(1..1, ["--font".to_string(), font]);
    }

    let mut bindings = Bindings::new()
        .bind(chord!("Super+Q"), Action::spawn("rio", [] as [&str; 0]))
        .bind(
            chord!("Super+T"),
            Action::spawn("qutebrowser", [] as [&str; 0]),
        )
        .bind(chord!("Super+E"), Action::spawn("thunar", [] as [&str; 0]))
        .bind(chord!("Super+C"), Action::Close)
        .bind(chord!("Super+V"), Action::ToggleFloat)
        .bind(chord!("Super+F"), Action::ToggleFullscreen)
        .bind(chord!("Super+R"), Action::spawn("tofi-drun", tofi_args))
        .bind(
            chord!("Super+N"),
            Action::spawn("rio", ["-e", "nvim", notes.as_str()]),
        )
        .bind(chord!("Super+D"), Action::spawn("rio", ["-e", "concord"]))
        .bind(
            chord!("Super+Tab"),
            Action::spawn("rio", ["-e", "create-session"]),
        )
        .bind(
            chord!("Super+Shift+Tab"),
            Action::spawn("rio", ["-e", "attach-session"]),
        )
        .bind(
            chord!("Super+F1"),
            Action::spawn(night_light.as_str(), [] as [&str; 0]),
        )
        .bind(
            chord!("Super+X"),
            Action::spawn(rec.as_str(), [] as [&str; 0]),
        )
        .bind(
            chord!("Super+Shift+X"),
            Action::spawn(rec_stop.as_str(), [] as [&str; 0]),
        )
        .bind(
            chord!("XF86AudioRaiseVolume"),
            Action::spawn(
                "wpctl",
                ["set-volume", "-l", "1", "@DEFAULT_AUDIO_SINK@", "5%+"],
            ),
        )
        .bind(
            chord!("XF86AudioLowerVolume"),
            Action::spawn("wpctl", ["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"]),
        )
        .bind(
            chord!("XF86AudioMute"),
            Action::spawn("wpctl", ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]),
        )
        .bind(
            chord!("XF86AudioMicMute"),
            Action::spawn("wpctl", ["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]),
        )
        .bind(
            chord!("XF86MonBrightnessUp"),
            Action::spawn("brightnessctl", ["-e4", "-n2", "set", "5%+"]),
        )
        .bind(
            chord!("XF86MonBrightnessDown"),
            Action::spawn("brightnessctl", ["-e4", "-n2", "set", "5%-"]),
        )
        .bind(
            chord!("XF86AudioNext"),
            Action::spawn("playerctl", ["next"]),
        )
        .bind(
            chord!("XF86AudioPrev"),
            Action::spawn("playerctl", ["previous"]),
        )
        .bind(
            chord!("XF86AudioPlay"),
            Action::spawn("playerctl", ["play-pause"]),
        )
        .bind(
            chord!("XF86AudioPause"),
            Action::spawn("playerctl", ["play-pause"]),
        );

    for &(key, direction) in &[
        ("H", Direction::Left),
        ("L", Direction::Right),
        ("K", Direction::Up),
        ("J", Direction::Down),
    ] {
        let focus_chord = format!("{MOD}+{key}");
        let swap_chord = format!("{MOD}+Shift+{key}");
        bindings = bindings
            .bind(&focus_chord, Action::FocusDirection(direction))
            .bind(&swap_chord, Action::SwapDirection(direction));
    }

    for tag in 1..=10u8 {
        let key = if tag == 10 {
            "0".to_string()
        } else {
            tag.to_string()
        };
        let focus_chord = format!("{MOD}+{key}");
        let move_chord = format!("{MOD}+Shift+{key}");
        bindings = bindings
            .bind(&focus_chord, Action::FocusTag(tag))
            .bind(&move_chord, Action::MoveToTag(tag));
    }

    // Outputs (screens): cycle the focused screen — new windows,
    // tag commands, and keyboard input follow. By default the pointer
    // follows explicit output-focus changes too. Shift variants move
    // the focused window instead.
    bindings = bindings
        .bind(
            chord!("Super+Period"),
            Action::focus_output(Direction::Next),
        )
        .bind(chord!("Super+Comma"), Action::focus_output(Direction::Prev))
        .bind(
            chord!("Super+Shift+Period"),
            Action::send_to_output(Direction::Next),
        )
        .bind(
            chord!("Super+Shift+Comma"),
            Action::send_to_output(Direction::Prev),
        );

    let screenshot = r#"mkdir -p "$HOME/Pictures/Screenshots" && grim -g "$(slurp)" - | tee "$HOME/Pictures/Screenshots/$(date +%Y-%m-%d_%H-%M-%S).png" | wl-copy"#;
    let hall_of_fame = r#"mkdir -p "$HOME/Pictures/hall-of-fame" && grim -g "$(slurp)" - | tee "$HOME/Pictures/hall-of-fame/$(date +%Y-%m-%d_%H-%M-%S).png" | wl-copy"#;
    if command_available("grim") && command_available("slurp") {
        bindings = bindings
            .bind(chord!("Super+Z"), Action::spawn("sh", ["-c", screenshot]))
            .bind(
                chord!("Super+Ctrl+Z"),
                Action::spawn("sh", ["-c", hall_of_fame]),
            );
    }

    bindings = bindings
        .bind(chord!("Super+B"), Action::enter_mode("tile_ratio"))
        .bind_in_mode(
            "tile_ratio",
            chord!("Super+H"),
            Action::adjust_main_ratio(-0.05),
        )
        .bind_in_mode(
            "tile_ratio",
            chord!("Super+L"),
            Action::adjust_main_ratio(0.05),
        )
        .bind_in_mode("tile_ratio", chord!("Super+B"), Action::ExitMode)
        .bind_in_mode("tile_ratio", chord!("Escape"), Action::ExitMode);

    Config::new()
        // Output-focus pointer warping is enabled by default. Uncomment
        // this to keep the pointer in place when the bindings above
        // switch screens.
        // .warp_pointer_on_output_focus(false)
        .layout(
            Layout::new()
                .gaps(0)
                .outer_padding(0)
                .main_ratio(ratio!(0.55))
                .smart_gaps(true)
                .border_width(3)
                .border_color_focused(Color::hex(0xC8324BFF))
                .border_color_unfocused(Color::hex(0x00C896FF)),
        )
        // Configure output modes, positions, and pointer adjacency with
        // the compositor's output manager (for example kanshi).
        .bindings(bindings)
        .spawn(["waybar"])
        .spawn(["wl-paste", "--type", "text", "--watch", "cliphist", "store"])
        .spawn([
            "wl-paste", "--type", "image", "--watch", "cliphist", "store",
        ])
        .spawn(["swaybg", "-i", wallpaper.as_str(), "-m", "fill"])
}

fn command_available(command: &str) -> bool {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(command))
        .any(|candidate| candidate.is_file())
}
