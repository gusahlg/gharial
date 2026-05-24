//! `river_seat_v1` event handler.
//!
//! Seat input is informational — `pointer_enter` updates our
//! `pointer_over` field so future click-to-focus / interactive-move
//! policies have something to read, but v0.2 doesn't act on it.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_seat_v1 as iface;
use crate::wayland_proto::RiverSeatV1;

use super::super::world::World;

impl Dispatch<RiverSeatV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverSeatV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            iface::Event::Removed => {
                if let Some(entry) = state.seats.remove(&id) {
                    entry.proxy.destroy();
                }
            }
            iface::Event::WlSeat { name } => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.wl_seat_name = Some(name);
                }
            }
            iface::Event::PointerEnter { window } => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.pointer_over = Some(window.id());
                }
            }
            iface::Event::PointerLeave => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.pointer_over = None;
                }
            }
            iface::Event::WindowInteraction { .. }
            | iface::Event::ShellSurfaceInteraction { .. }
            | iface::Event::OpDelta { .. }
            | iface::Event::OpRelease
            | iface::Event::PointerPosition { .. } => {
                // Interactive ops (move/resize), click-to-focus, pointer
                // motion: all v0.3 concerns. Drop on the floor for now.
            }
        }
    }
}
