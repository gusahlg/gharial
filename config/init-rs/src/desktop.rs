//! The desktop policy — the part you actually edit.
//!
//! Equivalent to everything past `gharialctl wait` in `config/init`:
//! layout defaults, application launchers, focus/swap chords, tag
//! bindings 1..10, the `tile_ratio` ratio-tweak mode, media keys, and
//! autostarts.

use std::env;
use std::path::{Path, PathBuf};

use crate::ipc;

const MOD: &str = "Super";

pub const FONT: &str = "/nix/store/msa8aj23csl6747wja4y9s0r5z5mcxv7-nerd-fonts-hack-3.4.0+3.003/share/fonts/truetype/NerdFonts/Hack/HackNerdFont-Regular.ttf";

pub fn configure(socket: &Path) -> Result<(), String> {
    let home = env::var("HOME").map_err(|_| "HOME not set".to_string())?;

    layout_defaults(socket)?;
    application_bindings(socket, &home)?;
    focus_and_swap(socket)?;
    tag_bindings(socket)?;
    tile_ratio_mode(socket)?;
    utilities(socket, &home)?;
    media_keys(socket)?;
    autostart(socket, &home)?;
    Ok(())
}

fn layout_defaults(socket: &Path) -> Result<(), String> {
    ipc::set(socket, "gaps", &["0"])?;
    ipc::set(socket, "outer-padding", &["0"])?;
    ipc::set(socket, "main-ratio", &["0.55"])?;
    ipc::set(socket, "smart-gaps", &["on"])?;
    Ok(())
}

fn application_bindings(socket: &Path, home: &str) -> Result<(), String> {
    ipc::bind(socket, &chord("Q"), &["spawn", "rio"])?;
    ipc::bind(socket, &chord("T"), &["spawn", "qutebrowser"])?;
    ipc::bind(socket, &chord("E"), &["spawn", "thunar"])?;
    ipc::bind(socket, &chord("C"), &["close"])?;
    ipc::bind(
        socket,
        &chord("R"),
        &[
            "spawn",
            "tofi-drun",
            "--drun-launch=true",
            "--font",
            FONT,
            "--height",
            "1000",
            "--width",
            "500",
            "--font-size",
            "12",
        ],
    )?;
    let docs = format!("{home}/DOCUMENTATION.txt");
    ipc::bind(socket, &chord("D"), &["spawn", "rio", "-e", "nvim", &docs])?;
    Ok(())
}

fn focus_and_swap(socket: &Path) -> Result<(), String> {
    // H/K go backwards, L/J go forwards — matches the shell init.
    for &(key, direction) in &[("H", "prev"), ("L", "next"), ("K", "prev"), ("J", "next")] {
        ipc::bind(socket, &chord(key), &["focus", direction])?;
        ipc::bind(socket, &shift_chord(key), &["swap", direction])?;
    }
    Ok(())
}

fn tag_bindings(socket: &Path) -> Result<(), String> {
    // Tags 1..=10. The '0' key targets tag 10 — same as dwm/river muscle memory.
    for i in 1u8..=10 {
        let key = if i == 10 { "0".to_string() } else { i.to_string() };
        let tag = i.to_string();
        ipc::bind(socket, &chord(&key), &["tag", "focus", &tag])?;
        ipc::bind(socket, &shift_chord(&key), &["tag", "move", &tag])?;
    }
    Ok(())
}

fn tile_ratio_mode(socket: &Path) -> Result<(), String> {
    ipc::bind(socket, &chord("B"), &["mode", "tile_ratio"])?;
    ipc::bind_in_mode(socket, "tile_ratio", &chord("H"), &["main-ratio", "-0.05"])?;
    ipc::bind_in_mode(socket, "tile_ratio", &chord("L"), &["main-ratio", "+0.05"])?;
    ipc::bind_in_mode(socket, "tile_ratio", &chord("B"), &["mode", "exit"])?;
    ipc::bind_in_mode(socket, "tile_ratio", "Escape", &["mode", "exit"])?;
    Ok(())
}

fn utilities(socket: &Path, home: &str) -> Result<(), String> {
    let night_light = format!("{home}/.local/share/night-light");
    ipc::bind(socket, &chord("F1"), &["spawn", &night_light])?;

    if which("grim").is_some() && which("slurp").is_some() {
        let screenshot = format!(
            "mkdir -p {home}/Pictures/Screenshots && \
             grim -g \"$(slurp)\" - | \
             tee {home}/Pictures/Screenshots/$(date +%Y-%m-%d_%H-%M-%S).png | \
             wl-copy"
        );
        ipc::bind(socket, &chord("Z"), &["spawn", "sh", "-c", &screenshot])?;
    }

    let rec = format!("{home}/.local/share/rec");
    let rec_stop = format!("{home}/.local/share/rec-stop");
    ipc::bind(socket, &chord("X"), &["spawn", &rec])?;
    ipc::bind(socket, &shift_chord("X"), &["spawn", &rec_stop])?;
    Ok(())
}

fn media_keys(socket: &Path) -> Result<(), String> {
    ipc::bind(
        socket,
        "XF86AudioRaiseVolume",
        &["spawn", "wpctl", "set-volume", "-l", "1", "@DEFAULT_AUDIO_SINK@", "5%+"],
    )?;
    ipc::bind(
        socket,
        "XF86AudioLowerVolume",
        &["spawn", "wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"],
    )?;
    ipc::bind(
        socket,
        "XF86AudioMute",
        &["spawn", "wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"],
    )?;
    ipc::bind(
        socket,
        "XF86AudioMicMute",
        &["spawn", "wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"],
    )?;

    ipc::bind(
        socket,
        "XF86MonBrightnessUp",
        &["spawn", "brightnessctl", "-e4", "-n2", "set", "5%+"],
    )?;
    ipc::bind(
        socket,
        "XF86MonBrightnessDown",
        &["spawn", "brightnessctl", "-e4", "-n2", "set", "5%-"],
    )?;

    ipc::bind(socket, "XF86AudioNext", &["spawn", "playerctl", "next"])?;
    ipc::bind(socket, "XF86AudioPrev", &["spawn", "playerctl", "previous"])?;
    ipc::bind(socket, "XF86AudioPlay", &["spawn", "playerctl", "play-pause"])?;
    ipc::bind(socket, "XF86AudioPause", &["spawn", "playerctl", "play-pause"])?;
    Ok(())
}

fn autostart(socket: &Path, home: &str) -> Result<(), String> {
    ipc::spawn(socket, &["waybar"])?;
    ipc::spawn(
        socket,
        &["wl-paste", "--type", "text", "--watch", "cliphist", "store"],
    )?;
    ipc::spawn(
        socket,
        &["wl-paste", "--type", "image", "--watch", "cliphist", "store"],
    )?;
    let wallpaper = format!("{home}/Pictures/doctor_nath.png");
    ipc::spawn(socket, &["swaybg", "-i", &wallpaper, "-m", "fill"])?;
    Ok(())
}

fn chord(key: &str) -> String {
    format!("{MOD}+{key}")
}

fn shift_chord(key: &str) -> String {
    format!("{MOD}+Shift+{key}")
}

fn which(prog: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(prog);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
