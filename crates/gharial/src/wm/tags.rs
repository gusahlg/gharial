//! Tag bitmask tracking. Tags 1..32 map to bits 0..31.
//!
//! v0.2 keeps a single global active mask shared across all outputs.
//! Per-output tag sets are an obvious v0.3 extension; the data model
//! here doesn't preclude it.

use super::world::World;

#[derive(Debug)]
pub struct Tags {
    /// Bitmask of currently-visible tags.
    pub active: u32,
}

impl Default for Tags {
    fn default() -> Self {
        Self { active: 1 }
    }
}

/// `1 << (n - 1)` with `n` already validated to 1..=32 by the caller.
/// The debug-assert catches drift in the upstream parsers that promise
/// the 1..=32 range; release builds trust the contract.
pub fn tag_mask(n: u8) -> u32 {
    debug_assert!(
        (1..=32).contains(&n),
        "tag_mask: n must be 1..=32, got {n}"
    );
    1u32 << (n - 1)
}

/// Recompute every window's `visible` flag from its tag mask vs the
/// active mask. Called after any tag-mutation action.
pub fn set_visibility_targets(world: &mut World) {
    let active = world.tags.active;
    let (_, by_id) = world.windows.split_mut();
    for entry in by_id.values_mut() {
        entry.visible = (entry.tags & active) != 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_mask_is_one_indexed() {
        assert_eq!(tag_mask(1), 0x0000_0001);
        assert_eq!(tag_mask(2), 0x0000_0002);
        assert_eq!(tag_mask(9), 0x0000_0100);
        assert_eq!(tag_mask(32), 0x8000_0000);
    }

    #[test]
    fn default_tags_show_tag_one() {
        assert_eq!(Tags::default().active, 0x1);
    }

    #[test]
    fn tag_mask_covers_every_valid_tag() {
        // 32 unique tags, every bit set exactly once across the range —
        // the union of all masks must be u32::MAX.
        let mut union = 0u32;
        for n in 1..=32u8 {
            let m = tag_mask(n);
            assert_ne!(m, 0, "tag {n} produced an empty mask");
            assert_eq!(union & m, 0, "tag {n} overlaps a previous mask");
            union |= m;
        }
        assert_eq!(union, u32::MAX);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "tag_mask")]
    fn tag_mask_panics_on_zero_in_debug() {
        let _ = tag_mask(0);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "tag_mask")]
    fn tag_mask_panics_on_overflow_in_debug() {
        let _ = tag_mask(33);
    }
}
