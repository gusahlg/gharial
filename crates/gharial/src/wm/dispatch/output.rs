//! `river_output_v1` event handler.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_output_v1 as iface;
use crate::wayland_proto::RiverOutputV1;

use super::super::world::World;

impl Dispatch<RiverOutputV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverOutputV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            iface::Event::Removed => {
                if let Some(entry) = state.outputs.remove(&id) {
                    entry.proxy.destroy();
                }
            }
            iface::Event::WlOutput { name } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    entry.wl_output_name = Some(name);
                }
            }
            iface::Event::Position { x, y } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    entry.position = (x, y);
                }
            }
            iface::Event::Dimensions { width, height } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    entry.dimensions = (width, height);
                }
            }
        }
    }
}
