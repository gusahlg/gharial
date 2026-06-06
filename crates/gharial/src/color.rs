//! User-facing RGBA colour type for border colours.
//!
//! The wire form is the eight-digit hex string `0xRRGGBBAA` that the
//! daemon's colour parser accepts. We give callers the freedom to spell
//! it `Color::rgba(r, g, b, a)`, `Color::rgb(r, g, b)` (full alpha), or
//! the hex shortcut [`Color::hex`].

use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Convenience: full alpha (0xFF).
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 0xFF)
    }

    /// Build a `Color` from a single u32 in `0xRRGGBBAA` byte order
    /// (matches the on-the-wire spelling).
    pub const fn hex(rgba: u32) -> Self {
        Self::rgba(
            ((rgba >> 24) & 0xFF) as u8,
            ((rgba >> 16) & 0xFF) as u8,
            ((rgba >> 8) & 0xFF) as u8,
            (rgba & 0xFF) as u8,
        )
    }

    /// Render the colour as the `0xRRGGBBAA` form the daemon expects.
    pub fn to_hex_string(&self) -> String {
        format!("0x{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }

    // Some named constants for documentation-friendly defaults.
    pub const BLACK: Color = Color::rgb(0x00, 0x00, 0x00);
    pub const WHITE: Color = Color::rgb(0xFF, 0xFF, 0xFF);
    pub const TRANSPARENT: Color = Color::rgba(0x00, 0x00, 0x00, 0x00);
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex_string())
    }
}

impl From<u32> for Color {
    fn from(rgba: u32) -> Self {
        Self::hex(rgba)
    }
}

impl From<(u8, u8, u8, u8)> for Color {
    fn from((r, g, b, a): (u8, u8, u8, u8)) -> Self {
        Self::rgba(r, g, b, a)
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Self::rgb(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_string_matches_wire_format() {
        assert_eq!(Color::rgb(0xC8, 0x32, 0x4B).to_hex_string(), "0xC8324BFF");
        assert_eq!(
            Color::rgba(0x00, 0xC8, 0x96, 0x80).to_hex_string(),
            "0x00C89680"
        );
    }

    #[test]
    fn hex_constructor_round_trips_through_string() {
        let c = Color::hex(0xC8324BFF);
        assert_eq!(c, Color::rgba(0xC8, 0x32, 0x4B, 0xFF));
        assert_eq!(c.to_hex_string(), "0xC8324BFF");
    }

    #[test]
    fn display_uses_hex_form() {
        let c = Color::rgb(0xFF, 0x00, 0x00);
        assert_eq!(format!("{c}"), "0xFF0000FF");
    }

    #[test]
    fn from_tuple_constructors_match_rgba() {
        let c: Color = (0x12, 0x34, 0x56, 0x78).into();
        assert_eq!(c, Color::rgba(0x12, 0x34, 0x56, 0x78));
        let c: Color = (0xAB, 0xCD, 0xEF).into();
        assert_eq!(c, Color::rgb(0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn from_u32_uses_byte_order_matching_wire() {
        let c: Color = 0xC8324BFFu32.into();
        assert_eq!(c, Color::rgba(0xC8, 0x32, 0x4B, 0xFF));
    }

    #[test]
    fn named_constants() {
        assert_eq!(Color::BLACK.to_hex_string(), "0x000000FF");
        assert_eq!(Color::WHITE.to_hex_string(), "0xFFFFFFFF");
        assert_eq!(Color::TRANSPARENT.to_hex_string(), "0x00000000");
    }
}
