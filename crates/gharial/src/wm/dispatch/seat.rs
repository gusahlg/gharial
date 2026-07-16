//! `river_seat_v1` event handler.
//!
//! Three inputs matter here:
//!   * `pointer_enter`/`pointer_leave` keep `pointer_over` current.
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
            iface::Event::PointerPosition { x, y } => {
                if let Some(entry) = state.seats.get_mut(&id) {
                    entry.last_pointer = Some((x, y));
                }
            }
            iface::Event::WindowInteraction { window } => {
                // Click-to-focus. Focus requests are manage-bucket, so
                // route through the pending queue that the next
                // manage_start (which this event is guaranteed to be
                // followed by) drains.
                state.push_pending_focus(window.id());
            }
            iface::Event::ShellSurfaceInteraction { .. }
            | iface::Event::OpDelta { .. }
            | iface::Event::OpRelease => {
                // Interactive ops (move/resize) are a future concern.
            }
        }
    }
}
