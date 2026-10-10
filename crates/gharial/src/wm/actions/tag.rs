//! Tag-set actions: focus/toggle/move + window-toggle.
//!
//! Tag view actions (`focus`/`toggle`) apply to the *focused output's*
//! active mask — every screen is an independent view into the tag
//! space. All share [`apply_tag_change`] for the visibility recompute
//! and focus repair that follows any mutation.

use super::super::tags::{set_visibility_targets, tag_mask};
use super::super::world::World;
use super::focus::{clear_focus, focus_candidate, focused_window, set_focus};
use super::Outcome;

pub fn focus_tag(world: &mut World, n: u8) -> Outcome {
    let output = world.outputs.focused_mut().ok_or("no output")?;
    output.active_tags = tag_mask(n);
    apply_tag_change(world);
    Ok(String::new())
}

pub fn toggle_tag(world: &mut World, n: u8) -> Outcome {
    let output = world.outputs.focused_mut().ok_or("no output")?;
    output.active_tags ^= tag_mask(n);
    if output.active_tags == 0 {
        // An empty view shows nothing and offers no way back; keep the
        // tag we just tried to toggle off.
        output.active_tags = tag_mask(n);
    }
    apply_tag_change(world);
    Ok(String::new())
}

pub fn move_to_tag(world: &mut World, n: u8) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.tags = tag_mask(n);
    }
    apply_tag_change(world);
    Ok(String::new())
}

pub fn toggle_window_tag(world: &mut World, n: u8) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.tags ^= tag_mask(n);
        if entry.tags == 0 {
            entry.tags = tag_mask(n);
        }
    }
    apply_tag_change(world);
    Ok(String::new())
}

/// Recompute visibility and, if the focused window just disappeared,
/// restore the focused output's remembered focus.
pub fn apply_tag_change(world: &mut World) {
    set_visibility_targets(world);
    world.mark_layout_dirty();
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    if world
        .seats
        .get(&seat_id)
        .is_some_and(|s| s.layer_has_exclusive_focus())
    {
        return;
    }
    let still_visible = focused_window(world)
        .and_then(|id| world.windows.get(&id))
        .is_some_and(|w| w.visible && w.output == world.outputs.focused_id());
    if !still_visible {
        match focus_candidate(world) {
            Some(id) => set_focus(world, &seat_id, &id),
            None => clear_focus(world, &seat_id),
        }
    }
}
