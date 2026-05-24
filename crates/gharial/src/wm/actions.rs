//! Action execution. Called from inside a manage sequence (for state-
//! changing actions) or directly (for `Spawn`, which has no protocol
//! effect). Each action mutates `World` and may issue protocol
//! requests; everything goes through this module so adding a new
//! action is one match arm.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::action::{Action, BindingSpec, Direction};

use super::bindings::{install_binding, refresh_mode_enables};
use super::render;
use super::spatial::pick_neighbor;
use super::tags::{set_visibility_targets, tag_mask};
use super::world::World;

pub fn execute(action: Action, world: &mut World) {
    match action {
        Action::Spawn { cmd, args } => spawn(&cmd, &args),
        Action::Close => close_focused(world),
        Action::FocusDirection(dir) => focus_direction(world, dir),
        Action::SwapDirection(dir) => swap_direction(world, dir),
        Action::ToggleFloat => toggle_float(world),
        Action::Layout { key, args } => apply_layout(world, &key, &args),
        Action::EnterMode(name) => enter_mode(world, name),
        Action::ExitMode => enter_mode(world, "default".into()),
        Action::Bind { spec, action, mode } => bind(world, spec, *action, mode),
        Action::Unbind { spec, mode } => unbind(world, &spec, &mode),
        Action::FocusTag(n) => focus_tag(world, n),
        Action::ToggleTag(n) => toggle_tag(world, n),
        Action::MoveToTag(n) => move_to_tag(world, n),
        Action::ToggleWindowTag(n) => toggle_window_tag(world, n),
    }
}

fn spawn(cmd: &str, args: &[String]) {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // setsid() detaches the child from gharial's controlling terminal
    // and process group so signals (SIGINT, SIGHUP, …) sent to us
    // don't propagate to it. SIGPIPE goes back to default just in
    // case some library on our side ever changes it.
    //
    // Safety: pre_exec runs in a single-threaded post-fork context.
    // signal/setsid are both async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            libc::setsid();
            Ok(())
        });
    }
    match command.spawn() {
        // Detach: drop the Child handle. The background reaper thread
        // started in wm::run consumes the eventual zombie via waitpid.
        Ok(child) => drop(child),
        Err(e) => eprintln!("gharial: spawn {cmd:?} failed: {e}"),
    }
}

fn close_focused(world: &mut World) {
    let Some(seat) = world.seats.primary() else { return };
    let Some(focused) = seat.focused.as_ref() else { return };
    let Some(window) = world.windows.get(focused) else { return };
    window.proxy.close();
}

fn focus_direction(world: &mut World, dir: Direction) {
    if dir.is_spatial() {
        focus_spatial(world, dir);
    } else {
        focus_stack(world, dir);
    }
}

fn focus_stack(world: &mut World, dir: Direction) {
    let visible: Vec<_> = world
        .windows
        .ordered_ids()
        .into_iter()
        .filter(|id| world.windows.get(id).is_some_and(|w| w.visible))
        .collect();
    if visible.is_empty() {
        return;
    }
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else { return };
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
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else { return };
    let targets = render::compute_targets(world);
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

fn toggle_float(world: &mut World) {
    let Some(seat) = world.seats.primary() else { return };
    let Some(focused) = seat.focused.clone() else { return };
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
    }
}

fn swap_direction(world: &mut World, dir: Direction) {
    let ordered = world.windows.ordered_ids();
    let Some(seat) = world.seats.primary() else { return };
    let Some(focused) = seat.focused.clone() else { return };

    let target_id = if dir.is_spatial() {
        // Spatial: find the directional neighbour via the same picker
        // focus uses, so swap pairs match what the user sees.
        let targets = render::compute_targets(world);
        let Some(current_rect) = targets.get(&focused).copied() else { return };
        let rects: Vec<_> = targets.into_iter().collect();
        match pick_neighbor(&rects, &focused, current_rect, dir) {
            Some(id) => id,
            None => return,
        }
    } else {
        // Stack: cycle among visible windows.
        let visible: Vec<_> = ordered
            .iter()
            .filter(|id| world.windows.get(id).is_some_and(|w| w.visible))
            .cloned()
            .collect();
        if visible.len() < 2 {
            return;
        }
        let Some(v_idx) = visible.iter().position(|id| id == &focused) else { return };
        match dir {
            Direction::Next => visible[(v_idx + 1) % visible.len()].clone(),
            Direction::Prev => visible[(v_idx + visible.len() - 1) % visible.len()].clone(),
            _ => unreachable!("spatial dir handled in the other branch"),
        }
    };

    let (Some(i), Some(j)) = (
        ordered.iter().position(|id| id == &focused),
        ordered.iter().position(|id| *id == target_id),
    ) else {
        return;
    };
    world.windows.swap(i, j);
}

fn apply_layout(world: &mut World, key: &str, args: &[String]) {
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    if let Err(e) = world.shared.apply(key, &arg_refs) {
        eprintln!("gharial: layout {key}: {e}");
    }
    // shared.take_dirty() will be drained on the next ping loop; for an
    // action triggered from inside this manage sequence we don't need to
    // wait — flush_manage right after this drain picks up the new params.
    world.shared.take_dirty();
}

fn enter_mode(world: &mut World, name: String) {
    world.modes.active = name;
    refresh_mode_enables(world);
}

fn bind(world: &mut World, spec: BindingSpec, action: Action, mode: String) {
    if let Err(e) = install_binding(world, spec, action, mode) {
        eprintln!("gharial: bind failed: {e}");
    }
}

fn unbind(world: &mut World, spec: &BindingSpec, mode: &str) {
    world.bindings.remove(spec, mode);
}

fn focus_tag(world: &mut World, n: u8) {
    world.tags.active = tag_mask(n);
    apply_tag_change(world);
}

fn toggle_tag(world: &mut World, n: u8) {
    world.tags.active ^= tag_mask(n);
    if world.tags.active == 0 {
        // Empty tag set leaves nothing visible — fall back to the tag
        // we just toggled off, so the user is never staring at a blank
        // screen with no way out.
        world.tags.active = tag_mask(n);
    }
    apply_tag_change(world);
}

fn move_to_tag(world: &mut World, n: u8) {
    let Some(seat) = world.seats.primary() else { return };
    let Some(focused) = seat.focused.clone() else { return };
    if let Some(entry) = world.windows.get_mut(&focused) {
        entry.tags = tag_mask(n);
    }
    apply_tag_change(world);
}

fn toggle_window_tag(world: &mut World, n: u8) {
    let Some(seat) = world.seats.primary() else { return };
    let Some(focused) = seat.focused.clone() else { return };
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
    // Refocus: if the previously focused window is no longer visible,
    // jump to the first visible one.
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else { return };
    let still_visible = world
        .seats
        .get(&seat_id)
        .and_then(|s| s.focused.as_ref())
        .and_then(|id| world.windows.get(id))
        .map(|w| w.visible)
        .unwrap_or(false);
    if !still_visible {
        let next = world
            .windows
            .ordered_ids()
            .into_iter()
            .find(|id| world.windows.get(id).is_some_and(|w| w.visible));
        match next {
            Some(id) => set_focus(world, &seat_id, &id),
            None => clear_focus(world, &seat_id),
        }
    }
}

pub(super) fn set_focus(
    world: &mut World,
    seat_id: &wayland_client::backend::ObjectId,
    window_id: &wayland_client::backend::ObjectId,
) {
    let Some(window) = world.windows.get(window_id) else { return };
    if let Some(seat) = world.seats.get_mut(seat_id) {
        seat.proxy.focus_window(&window.proxy);
        seat.focused = Some(window_id.clone());
    }
}

pub(super) fn clear_focus(
    world: &mut World,
    seat_id: &wayland_client::backend::ObjectId,
) {
    if let Some(seat) = world.seats.get_mut(seat_id) {
        seat.proxy.clear_focus();
        seat.focused = None;
    }
}

/// "Always something focused" invariant — if the seat's focused window
/// has been closed or hidden, repoint focus to the first visible
/// window, or clear focus only when literally no visible windows exist.
/// Called at the tail of every `manage_start` drain.
pub(super) fn ensure_focus_invariant(world: &mut World) {
    let Some(seat_id) = world.seats.primary().map(|s| s.id()) else { return };
    // Don't compete with layer surfaces: while a launcher / panel has
    // keyboard focus (exclusive or non-exclusive), the protocol says
    // a focus_window request during the same manage sequence cancels
    // its focus. Skip — the layer surface needs to type.
    let layer_active = world
        .seats
        .get(&seat_id)
        .map(|s| s.layer_focus_active)
        .unwrap_or(false);
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
    let next = world
        .windows
        .ordered_ids()
        .into_iter()
        .find(|id| world.windows.get(id).is_some_and(|w| w.visible));
    match next {
        Some(id) => set_focus(world, &seat_id, &id),
        None => clear_focus(world, &seat_id),
    }
}
