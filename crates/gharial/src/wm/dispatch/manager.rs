//! Dispatch table for `river_window_manager_v1` — the top-level driver
//! of the manage/render loop.
//!
//! On `manage_start` we drain pending policy changes, refresh cached
//! layout targets when dirty, flush window-management requests, then
//! immediately `manage_finish`. On `render_start` we reuse those cached
//! targets unless an intervening event invalidated them.

use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_window_manager_v1 as iface;
use crate::wayland_proto::{RiverOutputV1, RiverSeatV1, RiverWindowManagerV1, RiverWindowV1};

use super::super::actions::{self, ensure_focus_invariant, set_focus};
use super::super::outputs::OutputEntry;
use super::super::render;
use super::super::seats::SeatEntry;
use super::super::tags::set_visibility_targets;
use super::super::windows::entry_for;
use super::super::world::World;

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
                set_visibility_targets(state);
                // Make sure one output is the layer-shell default so
                // launchers without a target output have a home.
                ensure_layer_shell_default(state);
                while let Some(action) = state.pending_actions.pop_front() {
                    actions::execute(action, state);
                }
                drain_pending_focus(state);
                ensure_focus_invariant(state);
                let targets = render::targets(state);
                render::flush_manage(state, &targets);
                proxy.manage_finish();
                state.sequence.exit_manage();
            }
            iface::Event::RenderStart => {
                state.sequence.enter_render();
                let targets = render::targets(state);
                render::flush_render(state, &targets);
                proxy.render_finish();
                state.sequence.exit_render();
            }
            iface::Event::Window { id } => {
                let entry = entry_for(id, &state.qh, state.tags.active);
                let window_id = entry.proxy.id();
                state.windows.insert(entry);
                state.pending_focus.push(window_id);
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
                // Session lock policy is a v0.3 concern.
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
fn drain_pending_focus(state: &mut World) {
    let pending = std::mem::take(&mut state.pending_focus);
    let Some(seat_id) = state.seats.primary().map(|s| s.id()) else {
        return;
    };
    for id in pending.into_iter().rev() {
        if state.windows.is_visible(&id) {
            set_focus(state, &seat_id, &id);
            return;
        }
    }
}
