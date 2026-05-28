//! Dispatch tables for the `river-layer-shell-v1` protocol.
//!
//! River uses layer-shell focus events to tell us when a layer surface
//! (a launcher, a panel) wants keyboard input. While that's true we
//! must not call `seat.focus_window` — doing so during the same manage
//! sequence cancels the layer focus, and tofi-like clients stop
//! receiving keys.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::layer_shell::{
    river_layer_shell_output_v1 as out_iface, river_layer_shell_seat_v1 as seat_iface,
};
use crate::wayland_proto::{RiverLayerShellOutputV1, RiverLayerShellSeatV1, RiverLayerShellV1};

use super::super::world::World;

impl Dispatch<RiverLayerShellOutputV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverLayerShellOutputV1,
        event: out_iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let proxy_id = proxy.id();
        let output_id = state.outputs.iter_ids().find(|id| {
            state
                .outputs
                .get(id)
                .and_then(|o| o.layer_shell.as_ref().map(Proxy::id))
                == Some(proxy_id.clone())
        });
        let Some(output_id) = output_id else { return };
        let Some(entry) = state.outputs.get_mut(&output_id) else {
            return;
        };
        match event {
            out_iface::Event::NonExclusiveArea {
                x,
                y,
                width,
                height,
            } => {
                // width/height come over the wire as i32; clamp to 0
                // before promoting to u32. The protocol guarantees
                // they're non-negative but being defensive keeps the
                // panic case in the renderer impossible.
                let w = width.max(0) as u32;
                let h = height.max(0) as u32;
                let area = crate::wm::outputs::Rect { x, y, w, h };
                let changed = entry.non_exclusive_area != Some(area);
                entry.non_exclusive_area = Some(area);
                if changed {
                    state.mark_layout_dirty();
                }
            }
        }
    }
}

impl Dispatch<RiverLayerShellSeatV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverLayerShellSeatV1,
        event: seat_iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Find the seat whose layer-shell handle matches this proxy.
        let proxy_id = proxy.id();
        let seat_id = state.seats.iter_ids().find(|id| {
            state
                .seats
                .get(id)
                .and_then(|s| s.layer_shell.as_ref().map(Proxy::id))
                == Some(proxy_id.clone())
        });
        let Some(seat_id) = seat_id else { return };
        let Some(seat) = state.seats.get_mut(&seat_id) else {
            return;
        };
        match event {
            seat_iface::Event::FocusExclusive | seat_iface::Event::FocusNonExclusive => {
                seat.layer_focus_active = true;
            }
            seat_iface::Event::FocusNone => {
                seat.layer_focus_active = false;
            }
        }
    }
}

// The layer-shell global itself has no events; same for the per-output
// object's request side. delegate_noop for the global; no need for
// anything more.
wayland_client::delegate_noop!(World: ignore RiverLayerShellV1);
