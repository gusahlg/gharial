//! `river_window_v1` event handler — records lifecycle into `Windows`
//! and marks the sequence dirty for events that require a re-layout.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_window_v1 as iface;
use crate::wayland_proto::RiverWindowV1;

use super::super::world::World;

impl Dispatch<RiverWindowV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            iface::Event::Closed => {
                // Destroy must follow `closed`, per protocol.
                if let Some(entry) = state.windows.remove(&id) {
                    state.focus.forget(&id);
                    state.mark_layout_dirty();
                    entry.node.destroy();
                    entry.proxy.destroy();
                }
            }
            iface::Event::Dimensions { width, height } => {
                if let Some(entry) = state.windows.get_mut(&id) {
                    entry.actual = Some((width, height));
                }
            }
            iface::Event::DimensionsHint { .. } => {
                // We don't honor min/max yet — propose what layout says
                // and let the window negotiate down.
            }
            iface::Event::AppId { app_id } => {
                if let Some(entry) = state.windows.get_mut(&id) {
                    entry.app_id = app_id;
                }
            }
            iface::Event::Title { title } => {
                if let Some(entry) = state.windows.get_mut(&id) {
                    entry.title = title;
                }
            }
            // Events we don't act on yet — unit variants first, then
            // the struct-variant payloads we absorb wholesale.
            iface::Event::MaximizeRequested
            | iface::Event::UnmaximizeRequested
            | iface::Event::ExitFullscreenRequested
            | iface::Event::MinimizeRequested => {}
            iface::Event::Parent { .. }
            | iface::Event::DecorationHint { .. }
            | iface::Event::PointerMoveRequested { .. }
            | iface::Event::PointerResizeRequested { .. }
            | iface::Event::FullscreenRequested { .. }
            | iface::Event::ShowWindowMenuRequested { .. }
            | iface::Event::UnreliablePid { .. }
            | iface::Event::PresentationHint { .. }
            | iface::Event::Identifier { .. } => {}
        }
    }
}
