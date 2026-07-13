//! Dispatch impls split per wayland interface so each file stays small
//! and the `World` mutation surface for a given event is obvious from
//! the file you're reading.

mod layer_shell;
mod manager;
mod output;
mod seat;
mod window;
mod wl_output;
mod xkb_binding;

use wayland_client::{
    delegate_noop, globals::GlobalListContents, protocol::wl_registry::WlRegistry,
};

use crate::wayland_proto::{
    RiverDecorationV1, RiverNodeV1, RiverPointerBindingV1, RiverShellSurfaceV1,
    RiverXkbBindingsSeatV1, RiverXkbBindingsV1,
};

use super::world::World;

// Interfaces with no events we act on. Pointer bindings, shell
// surfaces, decoration nodes and the chord-control seat object emit
// nothing we consume; the manager + render-list node never emit events.
delegate_noop!(World: ignore RiverNodeV1);
delegate_noop!(World: ignore RiverDecorationV1);
delegate_noop!(World: ignore RiverShellSurfaceV1);
delegate_noop!(World: ignore RiverPointerBindingV1);

delegate_noop!(World: ignore RiverXkbBindingsV1);
delegate_noop!(World: ignore RiverXkbBindingsSeatV1);

// Required for `registry_queue_init::<World>`. River surfaces outputs
// through the WM protocol, but the connector *name* lives on the plain
// `wl_output` global — so we track which wl_output globals exist (the
// initial burst is seeded from the GlobalList in `wm::run`; this keeps
// the map current across hotplug) and bind them on demand from the
// river output dispatch.
impl wayland_client::Dispatch<WlRegistry, GlobalListContents> for World {
    fn event(
        state: &mut Self,
        _proxy: &WlRegistry,
        event: wayland_client::protocol::wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_registry::Event;
        match event {
            Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => {
                state.wl_output_globals.insert(name, version);
            }
            Event::GlobalRemove { name } => {
                state.wl_output_globals.remove(&name);
            }
            _ => {}
        }
    }
}
