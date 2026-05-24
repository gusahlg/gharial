//! `river_xkb_binding_v1` event handler. The compositor fires `pressed`
//! whenever a registered chord triggers; we translate that into the
//! bound `Action`, enqueue it, and let the upcoming `manage_start`
//! (guaranteed by the protocol after every pressed event) drain it.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::xkb_bindings::river_xkb_binding_v1 as iface;
use crate::wayland_proto::RiverXkbBindingV1;

use super::super::world::World;

impl Dispatch<RiverXkbBindingV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverXkbBindingV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            iface::Event::Pressed => {
                let id = proxy.id();
                if let Some(entry) = state.bindings.get_by_proxy(&id) {
                    state.pending_actions.push_back(entry.action.clone());
                }
                // No manage_dirty: the xkb-bindings protocol guarantees a
                // manage_start event follows every pressed event.
            }
            iface::Event::Released => {
                // Actions fire on press; release is a no-op until a
                // press-and-hold feature wants it.
            }
            iface::Event::StopRepeat => {
                // Repeat is server-side; nothing to do client-side.
            }
        }
    }
}
