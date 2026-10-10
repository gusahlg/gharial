//! Dispatch table for `river_window_manager_v1` — the top-level driver
//! of the manage/render loop.
//!
//! `manage_start`: place new windows, run queued commands, settle focus,
//! flush window-management state, `manage_finish`, then answer the IPC
//! callers whose commands just took effect. `render_start`: position,
//! show/hide, border, and stack, `render_finish`, then resolve waits.

use wayland_client::{event_created_child, Connection, Dispatch, QueueHandle};

use crate::wayland_proto::window_management::river_window_manager_v1 as iface;
use crate::wayland_proto::{RiverOutputV1, RiverSeatV1, RiverWindowManagerV1, RiverWindowV1};

use super::super::actions::focus::{ensure_focus_invariant, set_focus};
use super::super::outputs::OutputEntry;
use super::super::seats::SeatEntry;
use super::super::tags::set_visibility_targets;
use super::super::windows::entry_for;
use super::super::world::World;
use super::super::{bindings, commands, placement, render, waits};

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
                    "gharial: river refused window management (another WM is \
                     running?). Stop it and try again."
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
                placement::place_new_windows(state);
                set_visibility_targets(state);
                commands::drain(state);
                bindings::sync(state);
                drain_pending_focus(state);
                ensure_focus_invariant(state);
                ensure_layer_shell_default(state);
                render::ensure_targets(state);
                render::manage::flush(state);
                proxy.manage_finish();
                state.sequence.exit_manage();
                commands::flush_replies(state);
                waits::check(state);
            }
            iface::Event::RenderStart => {
                state.sequence.enter_render();
                render::ensure_targets(state);
                render::draw::flush(state);
                proxy.render_finish();
                state.sequence.exit_render();
                waits::check(state);
            }
            iface::Event::Window { id } => {
                state.windows.insert(entry_for(id, &state.qh));
                state.mark_layout_dirty();
            }
            iface::Event::Output { id } => {
                let mut entry = OutputEntry::new(id);
                // Pair every river_output_v1 with a layer-shell handle
                // so panels/launchers on that output are accepted.
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
                // Bindings are materialised per seat.
                state.bindings.dirty = true;
            }
            iface::Event::SessionLocked | iface::Event::SessionUnlocked => {}
        }
    }

    // The manager's `window`/`output`/`seat` events carry new objects;
    // their per-instance state lives in `Windows`/`Outputs`/`Seats`
    // keyed by ObjectId, so the user-data slot is just `()`.
    event_created_child!(World, RiverWindowManagerV1, [
        iface::EVT_WINDOW_OPCODE => (RiverWindowV1, ()),
        iface::EVT_OUTPUT_OPCODE => (RiverOutputV1, ()),
        iface::EVT_SEAT_OPCODE   => (RiverSeatV1, ()),
    ]);
}

/// Keep the layer-shell default on the focused output, so a launcher
/// that doesn't pick an output opens on the screen being used.
/// `set_default` is manage-bucket.
fn ensure_layer_shell_default(state: &mut World) {
    let Some(focused) = state.outputs.focused_id() else {
        return;
    };
    if state.outputs.layer_default.as_ref() == Some(&focused) {
        return;
    }
    if let Some(ls) = state
        .outputs
        .get(&focused)
        .and_then(|o| o.layer_shell.as_ref())
    {
        ls.set_default();
        state.outputs.layer_default = Some(focused);
    }
}

/// Give keyboard focus to the most recent pending window that is still
/// visible (newly mapped, or clicked). While a layer surface holds
/// focus the queue is kept for later — focusing a window now would
/// cancel the launcher's focus mid-typing.
fn drain_pending_focus(state: &mut World) {
    let Some(seat_id) = state.seats.primary().map(|s| s.id()) else {
        return;
    };
    if state
        .seats
        .get(&seat_id)
        .is_some_and(|s| s.layer_has_exclusive_focus())
    {
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
