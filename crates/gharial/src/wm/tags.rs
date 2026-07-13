//! Tag bitmask helpers. Tags 1..32 map to bits 0..31.
//!
//! Since v0.3 every output owns its own active mask (see
//! [`super::outputs::OutputEntry::active_tags`]) — each screen is an
//! independent view into the tag space. The helpers here recompute
//! window visibility from those per-output masks.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;

use super::world::World;

/// `1 << (n - 1)` with `n` already validated to 1..=32 by the caller.
/// The debug-assert catches drift in the upstream parsers that promise
/// the 1..=32 range; release builds trust the contract.
pub fn tag_mask(n: u8) -> u32 {
    debug_assert!((1..=32).contains(&n), "tag_mask: n must be 1..=32, got {n}");
    1u32 << (n - 1)
}

/// Recompute every window's `visible` flag from its tag mask vs its
/// output's active mask. Windows whose output has disappeared (or was
/// never set) are re-homed to the focused output first. Called after
/// any tag- or output-mutation action and at the top of every manage
/// sequence.
pub fn set_visibility_targets(world: &mut World) {
    let masks: HashMap<ObjectId, u32> = world
        .outputs
        .iter()
        .map(|o| (o.id(), o.active_tags))
        .collect();
    let fallback = world.outputs.focused_id();
    let (_, by_id) = world.windows.split_mut();
    for entry in by_id.values_mut() {
        let valid = entry
            .output
            .as_ref()
            .is_some_and(|id| masks.contains_key(id));
        if !valid {
            entry.output = fallback.clone();
        }
        entry.visible = match entry.output.as_ref().and_then(|id| masks.get(id)) {
            Some(active) => (entry.tags & active) != 0,
            // No outputs at all — keep windows nominally visible so the
            // first output to appear shows them immediately.
            None => true,
        };
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
