//! Tag sets: which of the 32 tags a window belongs to, or which tags an
//! output shows.
//!
//! Wire spellings: a single tag (`9`), a comma list (`1,3,9`), `all`, or
//! a raw mask (`0x101`). Tags are 1-indexed; tag `n` is bit `n - 1`.

use std::fmt;

/// Lowest tag number.
pub const TAG_MIN: u8 = 1;
/// Highest tag number.
pub const TAG_MAX: u8 = 32;

/// A non-empty set of tags.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TagSet(u32);

impl TagSet {
    /// Every tag — a window with this set shows whatever tags its output
    /// views.
    pub const ALL: TagSet = TagSet(u32::MAX);

    /// The set holding only tag `n`, or `None` outside `1..=32`.
    pub const fn single(n: u8) -> Option<Self> {
        if n >= TAG_MIN && n <= TAG_MAX {
            Some(Self(1 << (n - 1)))
        } else {
            None
        }
    }

    /// Wrap a raw bitmask; `None` for the empty mask.
    pub const fn from_mask(mask: u32) -> Option<Self> {
        if mask == 0 {
            None
        } else {
            Some(Self(mask))
        }
    }

    pub const fn mask(self) -> u32 {
        self.0
    }

    pub const fn contains(self, n: u8) -> bool {
        n >= TAG_MIN && n <= TAG_MAX && self.0 & (1 << (n - 1)) != 0
    }

    /// Tag numbers in ascending order.
    pub fn iter(self) -> impl Iterator<Item = u8> {
        (TAG_MIN..=TAG_MAX).filter(move |&n| self.contains(n))
    }

    /// Parse `9`, `1,3,9`, `all`, or `0x101`.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s == "all" {
            return Ok(Self::ALL);
        }
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            let mask =
                u32::from_str_radix(hex, 16).map_err(|_| format!("invalid tag mask: {s}"))?;
            return Self::from_mask(mask).ok_or_else(|| format!("empty tag mask: {s}"));
        }
        let mut mask = 0u32;
        for part in s.split(',') {
            let n: u8 = part.trim().parse().map_err(|_| {
                format!("invalid tag {part:?} (expected 1..32, a comma list, or all)")
            })?;
            let single = Self::single(n).ok_or_else(|| format!("tag {n} out of range 1..32"))?;
            mask |= single.0;
        }
        Self::from_mask(mask).ok_or_else(|| format!("empty tag set: {s}"))
    }

    /// Canonical token: `all` or an ascending comma list. Round-trips
    /// with [`TagSet::parse`].
    pub fn to_token(self) -> String {
        if self == Self::ALL {
            return "all".into();
        }
        let mut out = String::new();
        for n in self.iter() {
            if !out.is_empty() {
                out.push(',');
            }
            out.push_str(&n.to_string());
        }
        out
    }
}

impl fmt::Display for TagSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_token())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tags_map_to_bits() {
        assert_eq!(TagSet::single(1).unwrap().mask(), 1);
        assert_eq!(TagSet::single(9).unwrap().mask(), 0x100);
        assert_eq!(TagSet::single(32).unwrap().mask(), 0x8000_0000);
        assert!(TagSet::single(0).is_none());
        assert!(TagSet::single(33).is_none());
    }

    #[test]
    fn parse_accepts_every_spelling() {
        assert_eq!(TagSet::parse("9").unwrap().mask(), 0x100);
        assert_eq!(TagSet::parse("1,3,9").unwrap().mask(), 0x105);
        assert_eq!(TagSet::parse("all").unwrap(), TagSet::ALL);
        assert_eq!(TagSet::parse("0x101").unwrap().mask(), 0x101);
    }

    #[test]
    fn parse_rejects_bad_input() {
        for bad in ["", "0", "33", "1,,2", "x", "0x0", "0xZZ", "-1"] {
            assert!(TagSet::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn token_round_trips() {
        for s in ["1", "1,3,9", "all", "32", "2,4,6,8,10"] {
            let t = TagSet::parse(s).unwrap();
            assert_eq!(t.to_token(), s);
            assert_eq!(TagSet::parse(&t.to_token()).unwrap(), t);
        }
        // Hex input canonicalises to the list form.
        assert_eq!(TagSet::parse("0x5").unwrap().to_token(), "1,3");
    }
}
