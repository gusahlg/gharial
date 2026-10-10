//! Window actions: close, the toggles, swap, and `window SELECTOR …`.

use gharial_ipc::{BoolValue, OutputTarget, Placement, WindowOp, WindowSelector};
use wayland_client::backend::ObjectId;

use crate::action::Direction;

use super::super::placement::{self, apply_command, move_to_output, resolve_output};
use super::super::procs;
use super::super::spatial::pick_neighbor;
use super::super::world::World;
use super::focus::{focus_window, focused_window, spatial_rects};
use super::tag::apply_tag_change;
use super::Outcome;

pub fn close_focused(world: &mut World) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    if let Some(w) = world.windows.get(&focused) {
        w.proxy.close();
    }
    Ok(String::new())
}

/// Toggle one placement flag on the focused window.
pub fn toggle(world: &mut World, set: fn(&mut Placement)) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    let mut p = Placement::default();
    set(&mut p);
    apply_command(world, &focused, &p)?;
    Ok(String::new())
}

pub fn toggle_float(p: &mut Placement) {
    p.floating = Some(BoolValue::Toggle);
}
pub fn toggle_fullscreen(p: &mut Placement) {
    p.fullscreen = Some(BoolValue::Toggle);
}
pub fn toggle_sticky(p: &mut Placement) {
    p.sticky = Some(BoolValue::Toggle);
}

/// Swap the focused tiled window with a neighbour. Spatial swaps work
/// across screens (the two windows trade places, outputs, and tags);
/// with no tiled neighbour that way, the window moves to the screen in
/// that direction instead. Focus stays on the moved window.
pub fn swap_direction(world: &mut World, dir: Direction) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    let entry = world.windows.get(&focused).ok_or("no focused window")?;
    if !entry.is_tiled() {
        return Err("swap: the focused window is not tiled".into());
    }
    let output = entry.output.clone();

    let target = if dir.is_spatial() {
        let rects: Vec<(ObjectId, crate::layout::Rect)> = spatial_rects(world)
            .into_iter()
            .filter(|(id, _)| world.windows.get(id).is_some_and(|w| w.is_tiled()))
            .collect();
        let Some(from) = rects.iter().find(|(id, _)| *id == focused).map(|(_, r)| *r) else {
            return Ok(String::new());
        };
        let recency = |id: &ObjectId| world.windows.get(id).map_or(0, |w| w.focused_at);
        match pick_neighbor(&rects, &focused, from, dir, recency) {
            Some(target) => target,
            None => {
                // Edge of the screen: carry the window over.
                let next = resolve_output(world, &OutputTarget::Direction(dir), output.as_ref());
                if let Some(next) = next.filter(|n| Some(n) != output.as_ref()) {
                    move_to_output(world, &focused, &next, false);
                    apply_tag_change(world);
                    focus_window(world, &focused);
                }
                return Ok(String::new());
            }
        }
    } else {
        let Some(output) = output.as_ref() else {
            return Ok(String::new());
        };
        let tiled = world.windows.tiled_on(output);
        let Some(i) = tiled.iter().position(|id| *id == focused) else {
            return Ok(String::new());
        };
        if tiled.len() < 2 {
            return Ok(String::new());
        }
        let j = match dir {
            Direction::Prev => (i + tiled.len() - 1) % tiled.len(),
            _ => (i + 1) % tiled.len(),
        };
        tiled[j].clone()
    };

    // Trade homes when the partner lives on another screen.
    let a = world
        .windows
        .get(&focused)
        .map(|w| (w.output.clone(), w.tags));
    let b = world
        .windows
        .get(&target)
        .map(|w| (w.output.clone(), w.tags));
    if let (Some((a_out, a_tags)), Some((b_out, b_tags))) = (a, b) {
        if a_out != b_out {
            for (id, out, tags) in [(&focused, b_out, b_tags), (&target, a_out, a_tags)] {
                if let Some(w) = world.windows.get_mut(id) {
                    w.output = out;
                    w.tags = tags;
                    w.evacuated = None;
                    w.reset_geometry();
                }
            }
        }
    }
    world.windows.swap_ids(&focused, &target);
    world.mark_layout_dirty();
    apply_tag_change(world);
    focus_window(world, &focused);
    Ok(String::new())
}

/// Windows a selector picks, in stack order.
pub fn select(world: &World, selector: &WindowSelector) -> Vec<ObjectId> {
    match selector {
        WindowSelector::Focused => focused_window(world).into_iter().collect(),
        WindowSelector::All => world.windows.ids(),
        WindowSelector::Id(uid) => world.windows.by_uid(*uid).into_iter().collect(),
        WindowSelector::Match(m) => world
            .windows
            .iter()
            .filter(|w| m.matches_text(w.app_id.as_deref(), w.title.as_deref()))
            .filter(|w| match m.pid {
                None => true,
                Some(want) => w.pid.is_some_and(|have| procs::is_related(have, want)),
            })
            .map(|w| w.id())
            .collect(),
    }
}

/// `window SELECTOR OP`. Replies with the affected windows' ids, one
/// per line.
pub fn window_command(world: &mut World, selector: &WindowSelector, op: &WindowOp) -> Outcome {
    let ids = select(world, selector);
    if ids.is_empty() {
        return Err(format!("no window matches {selector}"));
    }
    for id in &ids {
        match op {
            WindowOp::Close => {
                if let Some(w) = world.windows.get(id) {
                    w.proxy.close();
                }
            }
            WindowOp::Place(p) => placement::apply_command(world, id, p)?,
        }
    }
    apply_tag_change(world);
    let uids: Vec<String> = ids
        .iter()
        .filter_map(|id| world.windows.get(id))
        .map(|w| w.uid.to_string())
        .collect();
    Ok(uids.join("\n"))
}
