//! `river_seat_v1` event handler.
//!
//!   * `pointer_position` records the cursor's last global position so
//!     explicit output-focus changes avoid redundant pointer warps.
//!   * `window_interaction` is click-to-focus: clicking a window gives
//!     it keyboard focus, which also moves output focus to its screen.

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
                    if let Some(ls) = entry.layer_shell {
                        ls.destroy();
                    }
                    entry.proxy.destroy();
                    state.bindings.dirty = true;
                }
            }
            iface::Event::WlSeat { name } => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.wl_seat_name = Some(name);
                }
            }
            iface::Event::PointerPosition { x, y } => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.last_pointer = Some((x, y));
                }
            }
            iface::Event::WindowInteraction { window } => {
                // Focus requests are manage-bucket; a manage sequence is
                // guaranteed to follow this event.
                state.push_pending_focus(window.id());
            }
            iface::Event::PointerEnter { .. }
            | iface::Event::PointerLeave
            | iface::Event::ShellSurfaceInteraction { .. }
            | iface::Event::OpDelta { .. }
            | iface::Event::OpRelease => {}
        }
    }
}
