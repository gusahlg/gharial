//! `river_output_v1` event handler.
//!
//! Besides tracking geometry we bind the corresponding `wl_output`
//! global as soon as river tells us its global name — that's where the
//! connector name (`DP-1`) comes from, which is how users refer to
//! outputs in `output focus`/`output send` config.

use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_output_v1 as iface;
use crate::wayland_proto::RiverOutputV1;

use super::super::world::World;

/// Highest wl_output version we understand. v4 adds the `name` event.
const WL_OUTPUT_MAX_VERSION: u32 = 4;

impl Dispatch<RiverOutputV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverOutputV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            iface::Event::Removed => {
                if let Some(entry) = state.outputs.remove(&id) {
                    state.mark_layout_dirty();
                    if let Some(wl_output) = entry.wl_output {
                        if wl_output.version() >= 3 {
                            wl_output.release();
                        }
                    }
                    entry.proxy.destroy();
                    // Windows that lived here are re-homed to the
                    // focused output by the visibility pass at the top
                    // of the next manage sequence.
                }
            }
            iface::Event::WlOutput { name } => {
                let version = state.wl_output_globals.get(&name).copied();
                let bound: Option<WlOutput> = version.map(|v| {
                    state
                        .globals
                        .registry
                        .bind(name, v.min(WL_OUTPUT_MAX_VERSION), qh, ())
                });
                if let Some(entry) = state.outputs.get_mut(&id) {
                    entry.wl_output_name = Some(name);
                    entry.wl_output = bound;
                } else if let Some(wl_output) = bound {
                    // Output vanished between events; don't leak the bind.
                    if wl_output.version() >= 3 {
                        wl_output.release();
                    }
                }
            }
            iface::Event::Position { x, y } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    let changed = entry.position != (x, y);
                    entry.position = (x, y);
                    if changed {
                        state.mark_layout_dirty();
                    }
                }
            }
            iface::Event::Dimensions { width, height } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    let changed = entry.dimensions != (width, height);
                    entry.dimensions = (width, height);
                    if changed {
                        state.mark_layout_dirty();
                    }
                }
            }
        }
    }
}
