//! `wl_output` event handler — we bind these globals only to learn the
//! connector name (`DP-1`, `HDMI-A-1`, …) so users can refer to outputs
//! stably in `output focus` / `output link` config. Geometry comes from
//! the river protocol, not from here.

use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use super::super::world::World;

impl Dispatch<WlOutput, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            let wl_id = proxy.id();
            if let Some(output_id) = state.outputs.id_by_wl_output(&wl_id) {
                if let Some(entry) = state.outputs.get_mut(&output_id) {
                    entry.name = Some(name);
                }
            }
        }
        // Geometry / Mode / Scale / Description / Done — river's own
        // position + dimensions events are authoritative; ignore.
    }
}
