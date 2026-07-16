//! Output (screen) actions: switch the focused output and send the
//! focused window to another output.
//!
//! The focused output is where new windows appear, where tag commands
//! apply, and where keyboard focus is restored. By default switching it
//! also warps the pointer to that screen (unless it is already there);
//! configuration can leave the pointer in place instead.

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::action::Direction;
use crate::layout::Rect;
use crate::output::OutputTarget;
use crate::value::BoolValue;

use super::super::spatial::pick_neighbor;
use super::super::world::World;
use super::focus::{clear_focus, focus_candidate, set_focus};
use super::tag::apply_tag_change;

/// `pointer_warp` needs river_seat_v1 version 3.
const SEAT_WARP_SINCE: u32 = 3;

/// Resolve an `output focus`/`output send` target to a live output.
fn resolve_target(world: &World, target: &OutputTarget) -> Option<ObjectId> {
    match target {
        OutputTarget::Named(token) => world.outputs.resolve_named(token),
        OutputTarget::Direction(Direction::Next) => world.outputs.cycle_from_focused(true),
        OutputTarget::Direction(Direction::Prev) => world.outputs.cycle_from_focused(false),
        OutputTarget::Direction(dir) => {
            // Spatially nearest output in the given direction, using the
            // same neighbour picker windows use.
            let rects: Vec<(ObjectId, Rect)> = world
                .outputs
                .iter()
                .filter(|o| o.has_dimensions())
                .map(|o| (o.id(), o.rect()))
                .collect();
            let current_id = world.outputs.focused_id()?;
            let current_rect = world.outputs.get(&current_id)?.rect();
            pick_neighbor(&rects, &current_id, current_rect, *dir)
        }
    }
}

pub(in crate::wm) fn focus_output(world: &mut World, target: &OutputTarget) {
    let Some(output_id) = resolve_target(world, target) else {
        return;
    };
    if world.outputs.focused_id().as_ref() == Some(&output_id) {
        return;
    }
    world.outputs.set_focused(&output_id);

    // Move keyboard focus onto the new screen: remembered focus for its
    // active tags first, stack order second, cleared if it is empty.
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    match focus_candidate(world) {
        Some(id) => set_focus(world, &seat_id, &id),
        None => clear_focus(world, &seat_id),
    }
    if world.warp_pointer_on_output_focus {
        warp_pointer_to_output(world, &output_id);
    }
}

pub(in crate::wm) fn send_to_output(world: &mut World, target: &OutputTarget) {
    let Some(output_id) = resolve_target(world, target) else {
        return;
    };
    let Some(focused) = world.seats.primary().and_then(|s| s.focused.clone()) else {
        return;
    };
    let active_tags = world
        .outputs
        .get(&output_id)
        .map(|o| o.active_tags)
        .unwrap_or(1);
    let Some(entry) = world.windows.get_mut(&focused) else {
        return;
    };
    if entry.output.as_ref() == Some(&output_id) {
        return;
    }
    entry.output = Some(output_id);
    // The window adopts the target screen's current view so "send to
    // that screen" always means "visible on that screen".
    entry.tags = active_tags;
    // Force fresh geometry on the new output.
    entry.proposed = None;
    entry.position = None;
    // Focus stays on the current screen; apply_tag_change refocuses it
    // since the sent window is no longer visible here.
    apply_tag_change(world);
}

/// Configure whether explicit output-focus actions move the pointer to
/// the newly focused output.
pub(in crate::wm) fn set_output_focus_warp(world: &mut World, value: BoolValue) {
    world.warp_pointer_on_output_focus =
        resolve_output_focus_warp(world.warp_pointer_on_output_focus, value);
}

fn resolve_output_focus_warp(current: bool, value: BoolValue) -> bool {
    match value {
        BoolValue::On => true,
        BoolValue::Off => false,
        BoolValue::Toggle => !current,
    }
}

/// Warp the pointer to the centre of `output_id` unless it is already
/// on that output. Requires seat v3; called from inside a manage
/// sequence (pointer_warp is manage-restricted).
fn warp_pointer_to_output(world: &mut World, output_id: &ObjectId) {
    let Some(rect) = world.outputs.get(output_id).map(|o| o.rect()) else {
        return;
    };
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    let Some(seat) = world.seats.get_mut(&seat_id) else {
        return;
    };
    if let Some((px, py)) = seat.last_pointer {
        let inside = px >= rect.x
            && px < rect.x + rect.w as i32
            && py >= rect.y
            && py < rect.y + rect.h as i32;
        if inside {
            return;
        }
    }
    if seat.proxy.version() < SEAT_WARP_SINCE {
        return;
    }
    let center = (rect.x + rect.w as i32 / 2, rect.y + rect.h as i32 / 2);
    seat.proxy.pointer_warp(center.0, center.1);
    seat.last_pointer = Some(center);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_focus_warp_setting_handles_on_off_and_toggle() {
        assert!(resolve_output_focus_warp(false, BoolValue::On));
        assert!(!resolve_output_focus_warp(true, BoolValue::Off));
        assert!(resolve_output_focus_warp(false, BoolValue::Toggle));
        assert!(!resolve_output_focus_warp(true, BoolValue::Toggle));
    }
}
