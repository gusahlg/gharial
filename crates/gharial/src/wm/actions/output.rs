//! Output (screen) actions: focus a screen, send a window to one, show
//! tags or change presentation on one — optionally only while a process
//! runs.
//!
//! The focused output is where new windows appear, where tag commands
//! apply, and where keyboard focus is restored. By default switching it
//! also warps the pointer to that screen (unless it is already there).

use gharial_ipc::{BoolValue, OutputTarget, Presentation, TagSet};
use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use super::super::leases::{Lease, Value};
use super::super::placement::{move_to_output, resolve_output};
use super::super::world::World;
use super::focus::{clear_focus, focus_candidate, focused_window, set_focus};
use super::tag::apply_tag_change;
use super::Outcome;

/// `pointer_warp` needs river_seat_v1 version 3.
const SEAT_WARP_SINCE: u32 = 3;

fn resolve(world: &World, target: &OutputTarget) -> Result<ObjectId, String> {
    resolve_output(world, target, None).ok_or_else(|| match target {
        OutputTarget::Named(name) => format!("no output named {name}"),
        OutputTarget::Direction(dir) => format!("no output to the {}", dir.as_str()),
    })
}

pub fn focus_output(world: &mut World, target: &OutputTarget) -> Outcome {
    let output = resolve(world, target)?;
    focus_output_id(world, &output);
    Ok(String::new())
}

/// Make `output` the focused screen, moving keyboard focus there (its
/// remembered window, else its first visible one, else nothing).
pub fn focus_output_id(world: &mut World, output: &ObjectId) {
    if world.outputs.focused_id().as_ref() == Some(output) {
        return;
    }
    world.outputs.set_focused(output);
    if let Some(seat_id) = world.seats.primary().map(|s| s.id()) {
        match focus_candidate(world) {
            Some(id) => set_focus(world, &seat_id, &id),
            None => clear_focus(world, &seat_id),
        }
    }
    if world.settings.warp_on_output_focus {
        warp_pointer_to_output(world, output);
    }
}

pub fn send_to_output(world: &mut World, target: &OutputTarget) -> Outcome {
    let focused = focused_window(world).ok_or("no focused window")?;
    let from = world.windows.get(&focused).and_then(|w| w.output.clone());
    let output = resolve_output(world, target, from.as_ref())
        .ok_or_else(|| format!("no output {target}"))?;
    move_to_output(world, &focused, &output, false);
    // Focus stays on this screen; apply_tag_change refocuses it since
    // the sent window is no longer visible here.
    apply_tag_change(world);
    Ok(String::new())
}

pub fn set_output_focus_warp(world: &mut World, value: BoolValue) -> Outcome {
    let current = world.settings.warp_on_output_focus;
    world.settings.warp_on_output_focus = match value {
        BoolValue::On => true,
        BoolValue::Off => false,
        BoolValue::Toggle => !current,
    };
    Ok(format!(
        "output-focus-warp={}",
        world.settings.warp_on_output_focus
    ))
}

/// Show `tags` on an output without touching which output (or window)
/// has focus — unless it *is* the focused output, where keyboard focus
/// follows the view like `tag focus` would.
pub fn view_tags(
    world: &mut World,
    target: &OutputTarget,
    tags: TagSet,
    until_exit: Option<u32>,
) -> Outcome {
    let output = resolve(world, target)?;
    let entry = world.outputs.get_mut(&output).ok_or("output vanished")?;
    let previous = entry.active_tags;
    entry.active_tags = tags.mask();
    if let Some(pid) = until_exit {
        world.leases.add(Lease {
            pid,
            output,
            previous: Value::Tags(previous),
            set: Value::Tags(tags.mask()),
        });
        super::super::ticker::arm(world);
    }
    apply_tag_change(world);
    Ok(String::new())
}

pub fn set_presentation(
    world: &mut World,
    target: &OutputTarget,
    mode: Presentation,
    until_exit: Option<u32>,
) -> Outcome {
    let output = resolve(world, target)?;
    let entry = world.outputs.get_mut(&output).ok_or("output vanished")?;
    let previous = entry.presentation;
    entry.presentation = mode;
    if let Some(pid) = until_exit {
        world.leases.add(Lease {
            pid,
            output,
            previous: Value::Presentation(previous),
            set: Value::Presentation(mode),
        });
        super::super::ticker::arm(world);
    }
    Ok(String::new())
}

/// Warp the pointer to the centre of `output_id` unless it is already
/// on that output. Requires seat v3; called from inside a manage
/// sequence (pointer_warp is manage-restricted).
fn warp_pointer_to_output(world: &mut World, output_id: &ObjectId) {
    let Some(output) = world.outputs.get(output_id) else {
        return;
    };
    let rect = output.rect();
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let inside = |p: (i32, i32)| output.contains(p);
    let Some(seat) = world.seats.primary_mut() else {
        return;
    };
    if seat.last_pointer.is_some_and(inside) {
        return;
    }
    if seat.proxy.version() < SEAT_WARP_SINCE {
        return;
    }
    let center = (rect.x + rect.w as i32 / 2, rect.y + rect.h as i32 / 2);
    seat.proxy.pointer_warp(center.0, center.1);
    seat.last_pointer = Some(center);
}
