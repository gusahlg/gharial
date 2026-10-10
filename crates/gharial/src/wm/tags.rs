//! Tag bitmask helpers and the visibility pass. Tags 1..32 map to bits
//! 0..31; every output owns its own active mask (see
//! [`super::outputs::OutputEntry::active_tags`]).

use std::collections::HashMap;

use wayland_client::backend::ObjectId;

use super::world::World;

/// `1 << (n - 1)` with `n` already validated to 1..=32 by the caller.
pub fn tag_mask(n: u8) -> u32 {
    debug_assert!((1..=32).contains(&n), "tag_mask: n must be 1..=32, got {n}");
    1u32 << (n - 1)
}

/// The lowest tag in `mask`, as a single-tag mask (`0` stays `0`).
pub fn lowest_tag(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        1 << mask.trailing_zeros()
    }
}

/// Recompute every placed window's `visible` flag from its tags (or
/// stickiness) against its output's active mask. Run at the top of every
/// manage sequence and after any tag/output change.
///
/// Also heals homeless windows: a window whose output vanished moves to
/// the focused output *and onto its visible tags* (so it never silently
/// disappears), remembering where it came from; when an output with that
/// connector name returns, the window goes back with its old tags.
pub fn set_visibility_targets(world: &mut World) {
    let masks: HashMap<ObjectId, u32> = world
        .outputs
        .iter()
        .map(|o| (o.id(), o.active_tags))
        .collect();
    let names: HashMap<String, ObjectId> = world
        .outputs
        .iter()
        .filter_map(|o| Some((o.name.clone()?, o.id())))
        .collect();
    let fallback = world.outputs.focused_id();
    let fallback_mask = fallback.as_ref().and_then(|id| masks.get(id)).copied();

    let mut changed = false;
    for entry in world.windows.iter_mut() {
        if !entry.placed {
            continue;
        }
        // Back home?
        if let Some(home) = entry
            .evacuated
            .as_ref()
            .and_then(|e| names.get(&e.output_name))
        {
            let evac = entry.evacuated.take().expect("checked above");
            entry.output = Some(home.clone());
            entry.tags = evac.tags;
            entry.reset_geometry();
            changed = true;
        }
        let valid = entry
            .output
            .as_ref()
            .is_some_and(|id| masks.contains_key(id));
        if !valid && fallback.is_some() {
            entry.output = fallback.clone();
            if let Some(mask) = fallback_mask {
                if entry.tags & mask == 0 {
                    entry.tags = lowest_tag(mask);
                }
            }
            entry.reset_geometry();
            changed = true;
        }
        let visible = match entry.output.as_ref().and_then(|id| masks.get(id)) {
            Some(active) => entry.sticky || (entry.tags & active) != 0,
            // No outputs at all — keep windows nominally visible so the
            // first output to appear shows them immediately.
            None => true,
        };
        if visible != entry.visible {
            entry.visible = visible;
            changed = true;
        }
    }
    if changed {
        world.mark_layout_dirty();
        world.stacking_dirty = true;
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
    fn lowest_tag_picks_the_smallest_bit() {
        assert_eq!(lowest_tag(0b1010), 0b0010);
        assert_eq!(lowest_tag(0x8000_0000), 0x8000_0000);
        assert_eq!(lowest_tag(0), 0);
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
