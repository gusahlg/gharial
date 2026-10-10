//! `river_output_v1` event handler.
//!
//! Besides tracking geometry we bind the corresponding `wl_output`
//! global as soon as river names it — that's where the connector name
//! (`DP-1`) comes from, which is how users address outputs.

use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_output_v1 as iface;
use crate::wayland_proto::RiverOutputV1;

use super::super::leases::Value;
use super::super::windows::Evacuated;
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
                let Some(entry) = state.outputs.remove(&id) else {
                    return;
                };
                // Its windows are re-homed by the next visibility pass;
                // remember where they belong for when it comes back.
                for w in state.windows.iter_mut() {
                    if w.output.as_ref() == Some(&id) {
                        // A window already displaced by an earlier unplug
                        // keeps its original home.
                        if let (None, Some(name)) = (&w.evacuated, &entry.name) {
                            w.evacuated = Some(Evacuated {
                                output_name: name.clone(),
                                tags: w.tags,
                            });
                        }
                        w.output = None;
                    }
                }
                // Remember the view from before any `until-exit` lease, so
                // a temporary benchmark view doesn't stick after re-plug.
                let originals = state.leases.forget_output(&id);
                if let Some(remembered) = entry
                    .name
                    .as_ref()
                    .and_then(|n| state.outputs.remembered.get_mut(n))
                {
                    for value in originals {
                        match value {
                            Value::Tags(tags) => remembered.active_tags = tags,
                            Value::Presentation(p) => remembered.presentation = p,
                        }
                    }
                }
                state.mark_layout_dirty();
                if let Some(ls) = entry.layer_shell {
                    ls.destroy();
                }
                if let Some(wl_output) = entry.wl_output {
                    if wl_output.version() >= 3 {
                        wl_output.release();
                    }
                }
                entry.proxy.destroy();
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
                    if entry.position != (x, y) {
                        entry.position = (x, y);
                        state.mark_layout_dirty();
                    }
                }
            }
            iface::Event::Dimensions { width, height } => {
                if let Some(entry) = state.outputs.get_mut(&id) {
                    if entry.dimensions != (width, height) {
                        entry.dimensions = (width, height);
                        state.mark_layout_dirty();
                    }
                }
            }
        }
    }
}
