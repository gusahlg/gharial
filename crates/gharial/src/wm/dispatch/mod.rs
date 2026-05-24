//! Dispatch impls split per wayland interface so each file stays small
//! and the `World` mutation surface for a given event is obvious from
//! the file you're reading.

mod layer_shell;
mod manager;
mod output;
mod seat;
mod window;
mod xkb_binding;

use wayland_client::{
    delegate_noop,
    globals::GlobalListContents,
    protocol::wl_registry::WlRegistry,
};

use crate::wayland_proto::{
    RiverDecorationV1, RiverNodeV1, RiverPointerBindingV1, RiverShellSurfaceV1,
    RiverXkbBindingsSeatV1, RiverXkbBindingsV1,
};

use super::world::World;

// The registry is bound through `registry_queue_init` and we don't act
// on hotplug events directly (river surfaces outputs through the WM
// protocol). We still need a Dispatch impl to satisfy the bound.
delegate_noop!(World: ignore WlRegistry);

// Interfaces with no events we act on. Pointer bindings, shell
// surfaces, decoration nodes and the chord-control seat object are all
// v0.3+ concerns; the manager + render-list node never emit events.
delegate_noop!(World: ignore RiverNodeV1);
delegate_noop!(World: ignore RiverDecorationV1);
delegate_noop!(World: ignore RiverShellSurfaceV1);
delegate_noop!(World: ignore RiverPointerBindingV1);

delegate_noop!(World: ignore RiverXkbBindingsV1);
delegate_noop!(World: ignore RiverXkbBindingsSeatV1);

// Required for `registry_queue_init::<World>` — we don't act on
// global add/remove, but we still need to satisfy the bound.
impl wayland_client::Dispatch<WlRegistry, GlobalListContents> for World {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: wayland_client::protocol::wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<Self>,
    ) {
    }
}
