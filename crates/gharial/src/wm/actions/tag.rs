//! Tag-set actions: focus/toggle/move + window-toggle.
//!
//! All four tag actions share `apply_tag_change` for the
//! visibility recompute + focus restore that follows any mutation.

use super::super::tags::{set_visibility_targets, tag_mask};
use super::super::world::World;
use super::focus::{clear_focus, focus_candidate, set_focus};

pub(in crate::wm) fn focus_tag(world: &mut World, n: u8) {
    world.tags.active = tag_mask(n);
    apply_tag_change(world);
}

pub(in crate::wm) fn toggle_tag(world: &mut World, n: u8) {
    world.tags.active ^= tag_mask(n);
    if world.tags.active == 0 {
        // Empty tag set leaves nothing visible — fall back to the tag
        // we just toggled off, so the user is never staring at a blank
        // screen with no way out.
        world.tags.active = tag_mask(n);
    }
    apply_tag_change(world);
}

pub(in crate::wm) fn move_to_tag(world: &mut World, n: u8) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.clone() else {
        return;
    };
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.tags = tag_mask(n);
    }
    apply_tag_change(world);
}

pub(in crate::wm) fn toggle_window_tag(world: &mut World, n: u8) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.clone() else {
        return;
    };
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.tags ^= tag_mask(n);
        if entry.tags == 0 {
            entry.tags = tag_mask(n);
        }
    }
    apply_tag_change(world);
}

fn apply_tag_change(world: &mut World) {
    set_visibility_targets(world);
    world.mark_layout_dirty();
    // Refocus: if the previously focused window is no longer visible,
    // restore the remembered focus for the active tag before falling
    // back to stack order.
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    let still_visible = world
        .seats
        .get(&seat_id)
        .and_then(|s| s.focused.as_ref())
        .and_then(|id| world.windows.get(id))
        .is_some_and(|w| w.visible);
    if !still_visible {
        match focus_candidate(world) {
            Some(id) => set_focus(world, &seat_id, &id),
            None => clear_focus(world, &seat_id),
        }
    }
}
