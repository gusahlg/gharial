//! User-facing layout orientation enum.
//!
//! Kept separate from the daemon-internal `layout::Orientation` so the
//! library doesn't drag layout-algorithm internals into the public
//! surface. The wire form (`"left"`, `"right"`, `"top"`, `"bottom"`)
//! is the shared contract — both types stringify to and parse from
//! the same set.

use std::fmt;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Orientation {
    Left,
    Right,
    Top,
    Bottom,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Orientation {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        match s {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_str() {
        for o in [
            Orientation::Left,
            Orientation::Right,
            Orientation::Top,
            Orientation::Bottom,
        ] {
            assert_eq!(o.as_str().parse::<Orientation>().unwrap(), o);
        }
    }

    #[test]
    fn unknown_fails() {
        assert!("diagonal".parse::<Orientation>().is_err());
    }
}
