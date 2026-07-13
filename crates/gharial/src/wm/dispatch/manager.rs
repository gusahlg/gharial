//! Dispatch table for `river_window_manager_v1` — the top-level driver
//! of the manage/render loop.
//!
//! On `manage_start` we heal the focused-output pointer, drain pending
//! policy changes, refresh cached layout targets when dirty, apply
//! pointer edge-link warps, flush window-management requests, then
//! immediately `manage_finish`. On `render_start` we reuse those cached
//! targets unless an intervening event invalidated them.

use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

use crate::layout::Rect;
use crate::state::{OutputInfo, OutputsInfo};
use crate::wayland_proto::window_management::river_window_manager_v1 as iface;
use crate::wayland_proto::{RiverOutputV1, RiverSeatV1, RiverWindowManagerV1, RiverWindowV1};

use super::super::actions::{self, ensure_focus_invariant, set_focus};
use super::super::links::{near_linked_edge, warp_destination, ResolvedLink};
use super::super::outputs::OutputEntry;
use super::super::render;
use super::super::seats::SeatEntry;
use super::super::tags::set_visibility_targets;
use super::super::windows::entry_for;
use super::super::world::World;

/// `pointer_warp` needs river_seat_v1 version 3.
const SEAT_WARP_SINCE: u32 = 3;

impl Dispatch<RiverWindowManagerV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowManagerV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            iface::Event::Unavailable => {
                eprintln!(
                    "gharial: river refused to advertise window-management (already \
                     in use). Stop the other WM and try again."
                );
                state.shutdown();
            }
            iface::Event::Finished => {
                eprintln!("gharial: river closed the window-management session");
                state.shutdown();
            }
            iface::Event::ManageStart => {
                state.sequence.enter_manage();
                state.outputs.ensure_focused();
                set_visibility_targets(state);
                // Make sure one output is the layer-shell default so
                // launchers without a target output have a home.
                ensure_layer_shell_default(state);
                while let Some(action) = state.pending_actions.pop_front() {
                    actions::execute(action, state);
                }
                drain_pending_focus(state);
                ensure_focus_invariant(state);
                pointer_edge_warp(state);
                render::ensure_targets(state);
                // One lock acquisition for both layout+border snapshots so
                // an IPC-thread change can't tear values mid-flush.
                let (_, borders) = state.shared.render_snapshot();
                render::flush_manage(state, &borders);
                sync_outputs_mirror(state);
                proxy.manage_finish();
                state.sequence.exit_manage();
            }
            iface::Event::RenderStart => {
                state.sequence.enter_render();
                render::ensure_targets(state);
                let borders = state.shared.borders();
                render::flush_render(state, &borders);
                proxy.render_finish();
                state.sequence.exit_render();
            }
            iface::Event::Window { id } => {
                // New windows land on the focused output, on its
                // currently-active tags, so they appear in front of the
                // user immediately.
                let (tags, output) = match state.outputs.focused() {
                    Some(o) => (o.active_tags, Some(o.id())),
                    None => (1, None),
                };
                let entry = entry_for(id, &state.qh, tags, output);
                let window_id = entry.proxy.id();
                state.windows.insert(entry);
                state.push_pending_focus(window_id);
                state.mark_layout_dirty();
            }
            iface::Event::Output { id } => {
                let mut entry = OutputEntry::new(id);
                // Pair every river_output_v1 with a layer-shell handle
                // so panels/launchers on that output are accepted by
                // river. `set_default` follows in manage_start.
                entry.layer_shell = Some(state.globals.layer_shell.get_output(
                    &entry.proxy,
                    &state.qh,
                    (),
                ));
                state.outputs.insert(entry);
                state.mark_layout_dirty();
            }
            iface::Event::Seat { id } => {
                let mut entry = SeatEntry::new(id);
                entry.layer_shell = Some(state.globals.layer_shell.get_seat(
                    &entry.proxy,
                    &state.qh,
                    (),
                ));
                state.seats.insert(entry);
            }
            iface::Event::SessionLocked | iface::Event::SessionUnlocked => {
                // Session lock policy is a future concern.
            }
        }
    }

    // Three of the manager's events (`window`, `output`, `seat`) carry a
    // `new_id` payload. wayland-client requires us to tell it which
    // user_data to attach to each freshly-created child proxy; the
    // default `event_created_child` panics. We attach `()` to all three
    // because their per-instance state lives in `Windows` / `Outputs` /
    // `Seats` keyed by ObjectId, not in the wayland user-data slot.
    event_created_child!(World, RiverWindowManagerV1, [
        iface::EVT_WINDOW_OPCODE => (RiverWindowV1, ()),
        iface::EVT_OUTPUT_OPCODE => (RiverOutputV1, ()),
        iface::EVT_SEAT_OPCODE   => (RiverSeatV1, ()),
    ]);
}

/// Pick the first output we know about and mark it as the layer-shell
/// default if we haven't already. `set_default` is manage-bucket, so
/// this only runs from inside `manage_start`.
fn ensure_layer_shell_default(state: &mut World) {
    let mut chosen: Option<wayland_client::backend::ObjectId> = None;
    for output in state.outputs.iter() {
        if output.layer_shell_default_sent {
            return; // someone already owns the default — nothing to do
        }
        if chosen.is_none() && output.layer_shell.is_some() {
            chosen = Some(output.id());
        }
    }
    let Some(id) = chosen else { return };
    if let Some(entry) = state.outputs.get_mut(&id) {
        if let Some(ls) = entry.layer_shell.as_ref() {
            ls.set_default();
            entry.layer_shell_default_sent = true;
        }
    }
}

/// Promote the most-recently-announced window to keyboard focus on the
/// primary seat. Discards earlier pending IDs in the same drain so a
/// burst of opens lands on the visually topmost window.
///
/// While a layer surface holds keyboard focus (launcher/panel), defer
/// the drain — issuing focus_window would cancel that layer focus and
/// drop the user's keystrokes mid-launch. Pending IDs survive until the
/// next manage_start that sees the layer surface release focus.
fn drain_pending_focus(state: &mut World) {
    let Some(seat_id) = state.seats.primary().map(|s| s.id()) else {
        return;
    };
    let layer_active = state
        .seats
        .get(&seat_id)
        .is_some_and(|s| s.layer_focus_active);
    if layer_active {
        return;
    }
    let pending = std::mem::take(&mut state.pending_focus);
    for id in pending.into_iter().rev() {
        if state.windows.is_visible(&id) {
            set_focus(state, &seat_id, &id);
            return;
        }
    }
}

/// Warp pointers through configured edge links.
///
/// Runs inside every manage sequence (pointer_warp is manage-bucket).
/// River only reports pointer position during manage sequences, so
/// while the pointer keeps moving inside the poll zone around a linked
/// edge we ask for another manage sequence to keep samples flowing; the
/// poll stops as soon as the pointer stops or leaves the zone.
fn pointer_edge_warp(state: &mut World) {
    if state.links.is_empty() {
        return;
    }
    let mut resolved: Vec<ResolvedLink> = Vec::new();
    for (a, b) in state.links.iter() {
        let rect_of = |token: &str| {
            state
                .outputs
                .resolve_named(token)
                .and_then(|id| state.outputs.get(&id))
                .filter(|o| o.has_dimensions())
                .map(|o| o.rect())
        };
        let (Some(ra), Some(rb)) = (rect_of(&a.output), rect_of(&b.output)) else {
            continue; // one side not connected (yet) — link stays dormant
        };
        // Links are bidirectional: store both directions.
        resolved.push(ResolvedLink {
            from: ra,
            from_edge: a.edge,
            to: rb,
            to_edge: b.edge,
        });
        resolved.push(ResolvedLink {
            from: rb,
            from_edge: b.edge,
            to: ra,
            to_edge: a.edge,
        });
    }
    if resolved.is_empty() {
        return;
    }
    let output_rects: Vec<Rect> = state
        .outputs
        .iter()
        .filter(|o| o.has_dimensions())
        .map(|o| o.rect())
        .collect();

    let seat_ids: Vec<_> = state.seats.iter_ids().collect();
    let mut keep_polling = false;
    for seat_id in seat_ids {
        let Some(seat) = state.seats.get_mut(&seat_id) else {
            continue;
        };
        let moved = std::mem::replace(&mut seat.pointer_moved, false);
        let Some(pos) = seat.last_pointer else {
            continue;
        };
        if seat.proxy.version() >= SEAT_WARP_SINCE {
            if let Some(dest) = warp_destination(pos, &resolved, &output_rects) {
                seat.proxy.pointer_warp(dest.0, dest.1);
                seat.last_pointer = Some(dest);
                continue;
            }
        }
        if moved && near_linked_edge(pos, &resolved) {
            keep_polling = true;
        }
    }
    if keep_polling {
        state.globals.manager.manage_dirty();
    }
}

/// Mirror output state into `Shared` so the IPC thread can answer
/// `output list` without crossing into the wayland thread.
fn sync_outputs_mirror(state: &World) {
    let focused = state.outputs.focused_id();
    let outputs = state
        .outputs
        .iter()
        .enumerate()
        .map(|(idx, o)| OutputInfo {
            // Index fallback matches the resolve grammar: outputs
            // without a connector name are addressable as "1", "2", …
            name: o.name.clone().unwrap_or_else(|| (idx + 1).to_string()),
            position: o.position,
            dimensions: o.dimensions,
            active_tags: o.active_tags,
            focused: focused.as_ref() == Some(&o.id()),
        })
        .collect();
    let links = state
        .links
        .iter()
        .map(|(a, b)| (a.to_token(), b.to_token()))
        .collect();
    state
        .shared
        .set_outputs_info(OutputsInfo { outputs, links });
}
