//! Keyboard focus: directional/stack focus actions and the invariants.
//!
//! Focus and outputs are kept coherent from one place: [`set_focus`]
//! records the window in its output's focus memory *and* moves output
//! focus to that output, so keyboard focus and the focused screen never
//! disagree.

use wayland_client::backend::ObjectId;

use crate::action::Direction;
use crate::layout::Rect;

use super::super::focus::pick_candidate;
use super::super::render;
use super::super::spatial::pick_neighbor;
use super::super::world::World;
use super::output::focus_output_id;
use super::Outcome;

/// The primary seat's focused window.
pub fn focused_window(world: &World) -> Option<ObjectId> {
    world.seats.primary().and_then(|s| s.focused.clone())
}

pub fn focus_direction(world: &mut World, dir: Direction) -> Outcome {
    if dir.is_spatial() {
        focus_spatial(world, dir)
    } else {
        focus_stack(world, dir)
    }
}

/// Cycle focus through the stack order of the focused output.
fn focus_stack(world: &mut World, dir: Direction) -> Outcome {
    let Some(output) = world.outputs.focused_id() else {
        return Err("no output".into());
    };
    let visible = world.windows.visible_on(&output);
    if visible.is_empty() {
        return Ok(String::new());
    }
    let current =
        focused_window(world).and_then(|id| visible.iter().position(|other| *other == id));
    let idx = match (current, dir) {
        (None, _) => 0,
        (Some(i), Direction::Prev) => (i + visible.len() - 1) % visible.len(),
        (Some(i), _) => (i + 1) % visible.len(),
    };
    focus_window(world, &visible[idx]);
    Ok(String::new())
}

/// What directional moves see: every visible window's on-screen rect,
/// across all outputs. An output showing a fullscreen window contributes
/// only that window (covering the output) — the windows hidden behind it
/// are not reachable by direction.
pub fn spatial_rects(world: &mut World) -> Vec<(ObjectId, Rect)> {
    render::ensure_targets(world);
    let mut covering: std::collections::HashMap<ObjectId, (u64, ObjectId)> = Default::default();
    for w in world.windows.iter().filter(|w| w.visible && w.fullscreen) {
        if let Some(out) = &w.output {
            let slot = covering.entry(out.clone()).or_insert((w.raised, w.id()));
            if w.raised >= slot.0 {
                *slot = (w.raised, w.id());
            }
        }
    }
    let mut rects = Vec::new();
    for w in world.windows.iter().filter(|w| w.visible && w.placed) {
        let Some(out) = &w.output else { continue };
        let rect = match covering.get(out) {
            Some((_, top)) if *top == w.id() => world.outputs.get(out).map(|o| o.rect()),
            Some(_) => None,
            None if w.is_tiled() => world.targets.get(&w.id()),
            None => w.content_rect(),
        };
        if let Some(rect) = rect {
            rects.push((w.id(), rect));
        }
    }
    rects
}

/// Focus the window visually next to the focused one, across screens.
/// With nothing in that direction, focus moves to the screen there (if
/// any) — so `focus right` from an empty screen still gets you over.
fn focus_spatial(world: &mut World, dir: Direction) -> Outcome {
    let rects = spatial_rects(world);
    let current = focused_window(world);
    let origin = current
        .as_ref()
        .and_then(|id| rects.iter().find(|(r, _)| r == id))
        .map(|(_, rect)| *rect)
        .or_else(|| world.outputs.focused().map(|o| o.rect()));
    let Some(origin) = origin else {
        return Ok(String::new());
    };

    // Index-keyed so "no focused window" needs no sentinel ObjectId.
    let indexed: Vec<(usize, Rect)> = rects
        .iter()
        .enumerate()
        .map(|(i, (_, r))| (i, *r))
        .collect();
    let from = current
        .as_ref()
        .and_then(|id| rects.iter().position(|(r, _)| r == id))
        .unwrap_or(usize::MAX);
    let recency = |i: &usize| world.windows.get(&rects[*i].0).map_or(0, |w| w.focused_at);
    if let Some(i) = pick_neighbor(&indexed, &from, origin, dir, recency) {
        let target = rects[i].0.clone();
        focus_window(world, &target);
        return Ok(String::new());
    }

    // Nothing that way on any screen: hop to the screen in that
    // direction (it may be empty).
    let here = current
        .as_ref()
        .and_then(|id| world.windows.get(id))
        .and_then(|w| w.output.clone())
        .or_else(|| world.outputs.focused_id());
    let next = super::super::placement::resolve_output(
        world,
        &gharial_ipc::OutputTarget::Direction(dir),
        here.as_ref(),
    );
    if let Some(output) = next {
        focus_output_id(world, &output);
    }
    Ok(String::new())
}

/// Give `window_id` keyboard focus on the primary seat (if it's visible).
pub fn focus_window(world: &mut World, window_id: &ObjectId) {
    if let Some(seat_id) = world.seats.primary().map(|s| s.id()) {
        set_focus(world, &seat_id, window_id);
    }
}

pub fn set_focus(world: &mut World, seat_id: &ObjectId, window_id: &ObjectId) {
    let Some(window) = world.windows.get(window_id) else {
        return;
    };
    if !window.visible {
        return;
    }
    let window_tags = window.tags;
    let window_output = window.output.clone();
    let Some(seat) = world.seats.get_mut(seat_id) else {
        return;
    };
    // Always (re)send: after a layer surface releases keyboard focus the
    // server's notion may differ from ours, and the request is cheap.
    seat.proxy.focus_window(&window.proxy);
    seat.focused = Some(window_id.clone());
    let serial = world.next_serial();
    if let Some(w) = world.windows.get_mut(window_id) {
        w.focused_at = serial;
        if w.floating || w.fullscreen {
            w.raised = serial;
            world.stacking_dirty = true;
        }
    }
    // Keyboard focus defines the focused screen: remember the window in
    // its output's per-tag memory and move output focus there.
    if let Some(output_id) = window_output {
        world.outputs.set_focused(&output_id);
        if let Some(output) = world.outputs.get_mut(&output_id) {
            output
                .focus
                .remember(output.active_tags, window_tags, window_id);
        }
    }
}

pub fn clear_focus(world: &mut World, seat_id: &ObjectId) {
    if let Some(seat) = world.seats.get_mut(seat_id) {
        if seat.focused.take().is_some() {
            seat.proxy.clear_focus();
        }
    }
}

/// "Always something focused" invariant — if the seat's focused window
/// has been closed or hidden, restore the focused output's remembered
/// focus or fall back to the first visible window there. Clear focus
/// only when the focused output has no visible windows.
pub fn ensure_focus_invariant(world: &mut World) {
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    // Don't compete with layer surfaces: while a launcher / panel holds
    // keyboard focus, a focus_window request in the same manage
    // sequence would cancel it.
    if world
        .seats
        .get(&seat_id)
        .is_some_and(|s| s.layer_has_exclusive_focus())
    {
        return;
    }
    let still_good = focused_window(world)
        .and_then(|id| world.windows.get(&id))
        .is_some_and(|w| w.visible);
    if still_good {
        return;
    }
    match focus_candidate(world) {
        Some(id) => set_focus(world, &seat_id, &id),
        None => clear_focus(world, &seat_id),
    }
}

/// Move focus off `id` to the next best window on the focused output.
pub fn refocus_elsewhere(world: &mut World, id: &ObjectId) {
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else {
        return;
    };
    let output = world.outputs.focused_id();
    let next = output.and_then(|o| {
        world
            .windows
            .visible_on(&o)
            .into_iter()
            .filter(|w| w != id)
            .filter(|w| world.windows.get(w).is_some_and(|e| !e.no_auto_focus))
            .max_by_key(|w| world.windows.get(w).map_or(0, |e| e.focused_at))
    });
    match next {
        Some(next) => set_focus(world, &seat_id, &next),
        None => clear_focus(world, &seat_id),
    }
}

/// Best focus candidate on the focused output: its per-tag remembered
/// focus first, then the first visible window in stack order. Both must
/// still be visible *on that output* — a remembered window that moved to
/// another screen must not drag focus over there.
pub fn focus_candidate(world: &World) -> Option<ObjectId> {
    let output = world.outputs.focused()?;
    let output_id = output.id();
    let remembered = output.focus.candidates(output.active_tags);
    let ordered = world.windows.visible_on(&output_id);
    pick_candidate(remembered, ordered, |id| {
        world
            .windows
            .get(id)
            .is_some_and(|w| w.visible && !w.no_auto_focus && w.output.as_ref() == Some(&output_id))
    })
}

/// Forget a removed window in every output's focus memory.
pub fn forget_window(world: &mut World, window_id: &ObjectId) {
    for output in world.outputs.iter_mut() {
        output.focus.forget(window_id);
    }
}
