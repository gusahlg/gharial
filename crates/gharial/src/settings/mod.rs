//! Layout and border settings — the `set`/`get`/`status` store.
//!
//! Owned by the wayland thread (no locks): every mutation arrives as an
//! `Action::Layout` and is applied in the manage sequence that consumes
//! it, so the next layout is computed from exactly what the user set.
//!
//! All command parsing — including the `+0.05`/`-0.05` relative-number
//! grammar — lives in [`parser`].

mod parser;

#[cfg(test)]
mod tests;

use crate::layout::Params;

/// Single border-color value as four pre-multiplied RGBA channels in
/// the protocol's u32 [0, 0xffffffff] scale.
pub type BorderColor = [u32; 4];

/// Widest border accepted, in pixels.
pub const MAX_BORDER_WIDTH: u32 = 100;

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
    let scale = |c: u8| -> u32 {
        // (c * a / 255) then expand 0..=255 byte to 0..=u32::MAX.
        let pm = (c as u32 * a as u32) / 0xff;
        pm * 0x0101_0101
    };
    [scale(r), scale(g), scale(b), (a as u32) * 0x0101_0101]
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub params: Params,
    pub borders: BorderConfig,
    /// Whether an explicit `output focus` moves the pointer to the newly
    /// focused output.
    pub warp_on_output_focus: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self::with_params(Params::default())
    }
}

/// Outcome of a settings command: a short summary for the caller and
/// whether anything actually changed (a no-op needs no relayout).
#[must_use]
pub struct Applied {
    pub summary: String,
    pub changed: bool,
}

impl Settings {
    pub fn with_params(params: Params) -> Self {
        Self {
            params,
            borders: BorderConfig::default(),
            warp_on_output_focus: true,
        }
    }

    /// Apply a layout or border command. Border keys route to the border
    /// config; everything else goes to layout `Params`. Nothing changes
    /// when the command errors.
    pub fn apply(&mut self, cmd: &str, args: &[&str]) -> Result<Applied, String> {
        match cmd {
            "border-width" | "border-color-focused" | "border-color-unfocused" => {
                let mut borders = self.borders.clone();
                parser::apply_border_command(&mut borders, cmd, args)?;
                // Sent as an i32 width; a negative one is a protocol error.
                borders.width = borders.width.min(MAX_BORDER_WIDTH);
                let changed = borders != self.borders;
                self.borders = borders;
                Ok(Applied {
                    summary: parser::summarize_border(&self.borders, cmd),
                    changed,
                })
            }
            _ => {
                let mut params = self.params.clone();
                parser::apply_command(&mut params, cmd, args)?;
                params.clamp();
                let changed = params != self.params;
                self.params = params;
                Ok(Applied {
                    summary: parser::summarize(&self.params, cmd),
                    changed,
                })
            }
        }
    }

    /// Read a single value as a string.
    pub fn get(&self, key: &str) -> Result<String, String> {
        let p = &self.params;
        let b = &self.borders;
        Ok(match key {
            "main-ratio" => format!("{:.4}", p.main_ratio),
            "main-count" => p.main_count.to_string(),
            "gaps" => p.gaps.to_string(),
            "outer-padding" => p.outer_padding.to_string(),
            "orientation" => p.orientation.as_str().to_string(),
            "smart-gaps" => p.smart_gaps.to_string(),
            "border-width" => b.width.to_string(),
            "border-color-focused" => format_color(&b.focused),
            "border-color-unfocused" => format_color(&b.unfocused),
            "output-focus-warp" => self.warp_on_output_focus.to_string(),
            _ => return Err(format!("unknown key: {key}")),
        })
    }

    /// Full state as a single line of `key=value` pairs, semicolon-separated.
    pub fn status_line(&self) -> String {
        let p = &self.params;
        let b = &self.borders;
        format!(
            "main-ratio={:.4};main-count={};gaps={};outer-padding={};orientation={};smart-gaps={};\
             border-width={};border-color-focused={};border-color-unfocused={};output-focus-warp={}",
            p.main_ratio,
            p.main_count,
            p.gaps,
            p.outer_padding,
            p.orientation.as_str(),
            p.smart_gaps,
            b.width,
            format_color(&b.focused),
            format_color(&b.unfocused),
            self.warp_on_output_focus,
        )
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
