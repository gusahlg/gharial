//! `wl_output` event handler — bound only to learn connector names
//! (`DP-1`, `HDMI-A-1`, …). Geometry comes from the river protocol.
//!
//! When a name arrives for an output that was connected before, its old
//! view (tags, presentation) comes back, and the next manage sequence
//! returns its evacuated windows.

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
        let wl_output::Event::Name { name } = event else {
            return;
        };
        let Some(output_id) = state.outputs.id_by_wl_output(&proxy.id()) else {
            return;
        };
        let remembered = state.outputs.remembered.remove(&name);
        if let Some(entry) = state.outputs.get_mut(&output_id) {
            if let Some(r) = remembered {
                entry.active_tags = r.active_tags;
                entry.presentation = r.presentation;
            }
            entry.name = Some(name);
        }
        // Not tied to a manage sequence; ask for one so returning
        // windows and restored tags take effect.
        state.mark_layout_dirty();
        state.request_manage();
    }
}
