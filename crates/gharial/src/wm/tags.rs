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
pub fn tag_mask(n: u8) -> u32 {
    1u32 << (n - 1)
}

/// Recompute every window's `visible` flag from its tag mask vs the
/// active mask. Called after any tag-mutation action.
pub fn set_visibility_targets(world: &mut World) {
    let active = world.tags.active;
    for id in world.windows.ordered_ids() {
        if let Some(entry) = world.windows.get_mut(&id) {
            entry.visible = (entry.tags & active) != 0;
        }
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
}
