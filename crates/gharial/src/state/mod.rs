//! The daemon's shared mutable state and the public command surface.
//!
//! `Shared` is the single mutex held by the wayland thread and any IPC
//! callers. It exposes a small typed API (`apply`, `get`, `status_line`)
//! and reports whether a state change should trigger a relayout via the
//! `dirty` flag.
//!
//! `Shared::send_action` is the bridge into the wayland thread for any
//! command that touches windows/focus/spawn — it goes through a calloop
//! channel installed by `wm::run`.
//!
//! All layout-param command parsing — including the `+0.05`/`-0.05`
//! relative-number grammar — lives in [`parser`].

mod parser;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use calloop::channel::Sender;

use crate::action::Action;
use crate::layout::Params;

use self::parser::apply_command;

/// Single border-color value as four pre-multiplied RGBA channels in
/// the protocol's u32 [0, 0xffffffff] scale.
pub type BorderColor = [u32; 4];

/// One output as mirrored for the IPC thread (`output list`). Written
/// by the wayland thread once per manage sequence; never authoritative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    /// Connector name (`DP-1`) or, when unknown, the 1-based
    /// advertisement index as a string.
    pub name: String,
    pub position: (i32, i32),
    pub dimensions: (i32, i32),
    pub active_tags: u32,
    pub focused: bool,
}

/// Output mirror for `output list`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OutputsInfo {
    pub outputs: Vec<OutputInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorderConfig {
    /// Border thickness in pixels. `0` disables borders entirely.
    pub width: u32,
    pub focused: BorderColor,
    pub unfocused: BorderColor,
}

impl Default for BorderConfig {
    fn default() -> Self {
        // Vivid red focused / vivid green unfocused at full alpha —
        // visible regardless of the desktop background. User overrides
        // via gharialctl.
        Self {
            width: 3,
            focused: premultiply_straight(0xC8, 0x32, 0x4B, 0xFF),
            unfocused: premultiply_straight(0x00, 0xC8, 0x96, 0xFF),
        }
    }
}

/// Convert straight (non-pre-multiplied) RGBA bytes to the protocol's
/// pre-multiplied four-u32 form. Each input byte spans 0..=0xff, each
/// output u32 spans 0..=0xffffffff (so `0xff` maps to `0xffffffff`).
pub fn premultiply_straight(r: u8, g: u8, b: u8, a: u8) -> BorderColor {
    // Pre-multiply: each color channel scales with alpha.
    let scale = |c: u8| -> u32 {
        // (c * a / 255) then expand 0..=255 byte to 0..=u32::MAX.
        let pm = (c as u32 * a as u32) / 0xff;
        pm * 0x0101_0101
    };
    [scale(r), scale(g), scale(b), (a as u32) * 0x0101_0101]
}

/// Handle on the daemon's parameter store. Cheap to clone (just an
/// `Arc`). Every mutator returns whether the state actually changed, so
/// the caller can decide whether to flag the layout dirty.
///
/// `tx` is the channel into the wayland thread; IPC handlers use it to
/// route window-management commands (close, focus, swap, spawn, …) that
/// can't be handled by mutating layout params alone.
#[derive(Clone)]
pub struct Shared {
    inner: Arc<Mutex<Inner>>,
    tx: Arc<Mutex<Option<Sender<Action>>>>,
}

struct Inner {
    params: Params,
    borders: BorderConfig,
    /// Output mirror for `output list`. Written by the wayland
    /// thread, read by IPC; changing it never sets `dirty` (it *is*
    /// derived from wayland state, not a cause of relayout).
    outputs: OutputsInfo,
    /// Set when params/borders have changed since the wayland thread
    /// last looked. Consumed by `take_dirty()`.
    dirty: bool,
}

/// Outcome of a command. Carries a short human-readable summary that is
/// fed back to the IPC caller and a `changed` bit that tells the
/// wayland thread whether to force a fresh layout demand.
#[must_use]
pub struct Applied {
    pub summary: String,
    pub changed: bool,
}

impl Shared {
    pub fn new(params: Params) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                params,
                borders: BorderConfig::default(),
                outputs: OutputsInfo::default(),
                dirty: false,
            })),
            tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Install the wayland-thread action channel. Called once during
    /// `wm::run` startup; before this is set, `send_action` is an error.
    pub fn set_action_sender(&self, tx: Sender<Action>) {
        *self.tx.lock().expect("tx mutex poisoned") = Some(tx);
    }

    pub fn send_action(&self, action: Action) -> Result<(), String> {
        match self.tx.lock().expect("tx mutex poisoned").as_ref() {
            Some(tx) => tx
                .send(action)
                .map_err(|e| format!("wayland thread not accepting actions: {e}")),
            None => Err("wayland thread not ready (no action channel yet)".into()),
        }
    }

    pub fn snapshot(&self) -> Params {
        self.inner
            .lock()
            .expect("params mutex poisoned")
            .params
            .clone()
    }

    pub fn borders(&self) -> BorderConfig {
        self.inner
            .lock()
            .expect("params mutex poisoned")
            .borders
            .clone()
    }

    /// Fetch both `Params` and `BorderConfig` under a single lock — the
    /// render path needs both per flush and grabbing them together avoids
    /// a second IPC-vs-renderer contention point.
    pub fn render_snapshot(&self) -> (Params, BorderConfig) {
        let inner = self.inner.lock().expect("params mutex poisoned");
        (inner.params.clone(), inner.borders.clone())
    }

    /// Apply a layout or border command. Sets the dirty flag on any
    /// real change. Border keys (`border-width`, `border-color-focused`,
    /// `border-color-unfocused`) route to the border config; everything
    /// else goes to layout `Params`.
    pub fn apply(&self, cmd: &str, args: &[&str]) -> Result<Applied, String> {
        let mut inner = self.inner.lock().expect("params mutex poisoned");
        let summary;
        let changed;
        match cmd {
            "border-width" | "border-color-focused" | "border-color-unfocused" => {
                let before = inner.borders.clone();
                parser::apply_border_command(&mut inner.borders, cmd, args)?;
                changed = inner.borders != before;
                summary = parser::summarize_border(&inner.borders, cmd);
            }
            _ => {
                let before = inner.params.clone();
                apply_command(&mut inner.params, cmd, args)?;
                inner.params.clamp();
                changed = inner.params != before;
                summary = parser::summarize(&inner.params, cmd);
            }
        }
        if changed {
            inner.dirty = true;
        }
        Ok(Applied { summary, changed })
    }

    /// Read a single value as a string.
    pub fn get(&self, key: &str) -> Result<String, String> {
        let inner = self.inner.lock().expect("params mutex poisoned");
        Ok(match key {
            "main-ratio" => format!("{:.4}", inner.params.main_ratio),
            "main-count" => inner.params.main_count.to_string(),
            "gaps" => inner.params.gaps.to_string(),
            "outer-padding" => inner.params.outer_padding.to_string(),
            "orientation" => inner.params.orientation.as_str().to_string(),
            "smart-gaps" => inner.params.smart_gaps.to_string(),
            "border-width" => inner.borders.width.to_string(),
            "border-color-focused" => format_color(&inner.borders.focused),
            "border-color-unfocused" => format_color(&inner.borders.unfocused),
            _ => return Err(format!("unknown key: {key}")),
        })
    }

    /// Full state as a single line of `key=value` pairs, semicolon-separated.
    pub fn status_line(&self) -> String {
        let inner = self.inner.lock().expect("params mutex poisoned");
        let p = &inner.params;
        let b = &inner.borders;
        format!(
            "main-ratio={:.4};main-count={};gaps={};outer-padding={};orientation={};smart-gaps={};\
             border-width={};border-color-focused={};border-color-unfocused={}",
            p.main_ratio,
            p.main_count,
            p.gaps,
            p.outer_padding,
            p.orientation.as_str(),
            p.smart_gaps,
            b.width,
            format_color(&b.focused),
            format_color(&b.unfocused),
        )
    }

    /// Returns `true` and clears the flag if a state change is pending.
    pub fn take_dirty(&self) -> bool {
        let mut inner = self.inner.lock().expect("params mutex poisoned");
        std::mem::replace(&mut inner.dirty, false)
    }

    /// Replace the output mirror. Called by the wayland thread; does
    /// not touch the dirty flag (the mirror is derived state).
    pub fn set_outputs_info(&self, info: OutputsInfo) {
        let mut inner = self.inner.lock().expect("params mutex poisoned");
        inner.outputs = info;
    }

    /// Snapshot of the output mirror for `output list`.
    pub fn outputs_info(&self) -> OutputsInfo {
        self.inner
            .lock()
            .expect("params mutex poisoned")
            .outputs
            .clone()
    }
}

/// Render a `BorderColor` as the user-friendly `0xRRGGBBAA` form by
/// inverting the pre-multiplication. The reported alpha is taken
/// directly from channel 3; for the colour channels we divide out the
/// alpha (rounded) so the value reflects what the user originally typed
/// (within rounding error).
fn format_color(c: &BorderColor) -> String {
    // Inverse of `(byte * 0x0101_0101)` with rounding. Saturating add
    // because the additive rounding offset would overflow on inputs
    // near `u32::MAX`.
    let to_byte = |v: u32| (v.saturating_add(0x0080_0080) / 0x0101_0101).min(0xff) as u8;
    let a_byte = to_byte(c[3]);
    let demul = |v: u32| -> u8 {
        if a_byte == 0 {
            0
        } else {
            let pm = to_byte(v);
            ((pm as u32 * 0xff) / a_byte as u32).min(0xff) as u8
        }
    };
    format!(
        "0x{:02X}{:02X}{:02X}{:02X}",
        demul(c[0]),
        demul(c[1]),
        demul(c[2]),
        a_byte
    )
}
