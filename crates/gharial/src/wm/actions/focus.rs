//! Focus-related actions and the seat-focus invariants.
//!
//! Public to the parent module: `set_focus`, `clear_focus`, and
//! `ensure_focus_invariant` — the wayland dispatch layer reaches into
//! these. The rest is implementation detail.

use wayland_client::backend::ObjectId;

use crate::action::Direction;

use super::super::focus::pick_candidate;
use super::super::render;
use super::super::spatial::pick_neighbor;
use super::super::world::World;

pub(in crate::wm) fn focus_direction(world: &mut World, dir: Direction) {
    if dir.is_spatial() {
        focus_spatial(world, dir);
    } else {
        focus_stack(world, dir);
    }
}

pub(in crate::wm) fn focus_stack(world: &mut World, dir: Direction) {
    let visible = world.windows.visible_ids();
    if visible.is_empty() {
        return;
    }
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    let current = world.seats.get(&seat_id).and_then(|s| s.focused.clone());
    let current_idx = current
        .as_ref()
        .and_then(|id| visible.iter().position(|other| other == id));
    let new_idx = match (current_idx, dir) {
        (None, _) => 0,
        (Some(i), Direction::Next) => (i + 1) % visible.len(),
        (Some(i), Direction::Prev) => (i + visible.len() - 1) % visible.len(),
        // Spatial directions are handled by focus_spatial; this match
        // arm exists so the compiler can't complain about exhaustiveness.
        (Some(i), _) => i,
    };
    set_focus(world, &seat_id, &visible[new_idx]);
}

fn focus_spatial(world: &mut World, dir: Direction) {
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    let targets = render::targets_snapshot(world);
    if targets.is_empty() {
        return;
    }
    let current_id = world.seats.get(&seat_id).and_then(|s| s.focused.clone());
    let Some(current_id) = current_id else {
        // Nothing focused — pick any visible window. Reuse the stack
        // path to avoid duplicating the "first visible" fallback.
        focus_stack(world, Direction::Next);
        return;
    };
    let Some(current_rect) = targets.get(&current_id).copied() else {
        // Focused window has no layout rect (floating, or not yet sized).
        // Fall back to stack-cycle in a sensible direction.
        focus_stack(
            world,
            match dir {
                Direction::Right | Direction::Down => Direction::Next,
                _ => Direction::Prev,
            },
        );
        return;
    };
    let rects: Vec<_> = targets.into_iter().collect();
    if let Some(next) = pick_neighbor(&rects, &current_id, current_rect, dir) {
        set_focus(world, &seat_id, &next);
    }
    // No neighbour in that direction: deliberately no-op rather than
    // wrap or jump. Avoids surprising focus jumps to the far side.
}

pub(in crate::wm) fn set_focus(world: &mut World, seat_id: &ObjectId, window_id: &ObjectId) {
    let Some(window) = world.windows.get(window_id) else {
        return;
    };
    if !window.visible {
        return;
    }
    let window_tags = window.tags;
    let Some(seat) = world.seats.get_mut(seat_id) else {
        return;
    };
    seat.proxy.focus_window(&window.proxy);
    seat.focused = Some(window_id.clone());
    world
        .focus
        .remember(world.tags.active, window_tags, window_id);
}

pub(in crate::wm) fn clear_focus(world: &mut World, seat_id: &ObjectId) {
    if let Some(seat) = world.seats.get_mut(seat_id) {
        seat.proxy.clear_focus();
        seat.focused = None;
    }
}

/// "Always something focused" invariant — if the seat's focused window
/// has been closed or hidden, restore the active tag's remembered focus
/// or fall back to the first visible window. Clear focus only when
/// literally no visible windows exist. Called at the tail of every
/// `manage_start` drain.
pub(in crate::wm) fn ensure_focus_invariant(world: &mut World) {
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    // Don't compete with layer surfaces: while a launcher / panel has
    // keyboard focus (exclusive or non-exclusive), the protocol says
    // a focus_window request during the same manage sequence cancels
    // its focus. Skip — the layer surface needs to type.
    let layer_active = world
        .seats
        .get(&seat_id)
        .is_some_and(|s| s.layer_focus_active);
    if layer_active {
        return;
    }
    let current = world.seats.get(&seat_id).and_then(|s| s.focused.clone());
    let still_good = current
        .as_ref()
        .and_then(|id| world.windows.get(id))
        .is_some_and(|w| w.visible);
    if still_good {
        return;
    }
    let next = focus_candidate(world);
    match next {
        Some(id) => set_focus(world, &seat_id, &id),
        None => clear_focus(world, &seat_id),
    }
}

pub(in crate::wm) fn focus_candidate(world: &World) -> Option<ObjectId> {
    let remembered = world.focus.candidates(world.tags.active);
    let ordered = world.windows.visible_ids();
    pick_candidate(remembered, ordered, |id| world.windows.is_visible(id))
}
