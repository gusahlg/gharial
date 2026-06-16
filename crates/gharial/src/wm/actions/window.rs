//! Window-state actions: close, toggle-float, swap-direction.

use crate::action::Direction;

use super::super::render;
use super::super::spatial::pick_neighbor;
use super::super::world::World;

pub(in crate::wm) fn close_focused(world: &mut World) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.as_ref() else {
        return;
    };
    let Some(window) = world.windows.get(focused) else {
        return;
    };
    window.proxy.close();
}

pub(in crate::wm) fn toggle_float(world: &mut World) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.clone() else {
        return;
    };
    let mut changed = false;
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.floating = !entry.floating;
        // Force re-evaluation: clear our cached "last position" so
        // flush_render reissues set_position once the window is tiled
        // again, and clear "last proposed" so propose_dimensions fires
        // anew when the window rejoins the stack.
        entry.position = None;
        entry.proposed = None;
        if entry.floating {
            // Floating windows sit on top of the tiled stack.
            entry.node.place_top();
        } else {
            entry.node.place_bottom();
        }
        changed = true;
    }
    if changed {
        world.mark_layout_dirty();
    }
}

pub(in crate::wm) fn toggle_fullscreen(world: &mut World) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.clone() else {
        return;
    };
    let mut changed = false;
    if let Some(entry) = world.windows.get_mut(&focused) {
        set_window_fullscreen(entry, !entry.fullscreen);
        changed = true;
    }
    if changed {
        world.mark_layout_dirty();
    }
}

/// Set a window's desired fullscreen state, resetting the cached
/// layout bookkeeping so it re-tiles cleanly when leaving fullscreen.
/// Shared by the toggle action and the client-driven fullscreen events.
pub(in crate::wm) fn set_window_fullscreen(
    entry: &mut super::super::windows::WindowEntry,
    fullscreen: bool,
) {
    if entry.fullscreen == fullscreen {
        return;
    }
    entry.fullscreen = fullscreen;
    // Force flush_render/flush_manage to reissue geometry once the
    // window rejoins (or leaves) the tiled layout: the server discards
    // our proposed size/position while fullscreen, so the cached values
    // no longer reflect what it will accept. The border is suppressed by
    // the server while fullscreen, so drop the cached signature too and
    // let flush_render repaint it on the way out.
    entry.proposed = None;
    entry.position = None;
    entry.borders_signature = None;
}

pub(in crate::wm) fn swap_direction(world: &mut World, dir: Direction) {
    let Some(seat) = world.seats.primary() else {
        return;
    };
    let Some(focused) = seat.focused.clone() else {
        return;
    };

    let target_id = if dir.is_spatial() {
        // Spatial: find the directional neighbour via the same picker
        // focus uses, so swap pairs match what the user sees.
        let targets = render::targets_snapshot(world);
        let Some(current_rect) = targets.get(&focused).copied() else {
            return;
        };
        let rects: Vec<_> = targets.into_iter().collect();
        match pick_neighbor(&rects, &focused, current_rect, dir) {
            Some(id) => id,
            None => return,
        }
    } else {
        // Stack: cycle among visible windows.
        let visible = world.windows.visible_ids();
        if visible.len() < 2 {
            return;
        }
        let Some(v_idx) = visible.iter().position(|id| id == &focused) else {
            return;
        };
        match dir {
            Direction::Next => visible[(v_idx + 1) % visible.len()].clone(),
            Direction::Prev => visible[(v_idx + visible.len() - 1) % visible.len()].clone(),
            _ => unreachable!("spatial dir handled in the other branch"),
        }
    };

    if world.windows.swap_ids(&focused, &target_id) {
        world.mark_layout_dirty();
    }
}
