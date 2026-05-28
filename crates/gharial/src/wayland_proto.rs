//! Generated bindings for the river protocols we speak.
//!
//! Two protocols are vendored under `protocol/` and bound at compile time
//! via `wayland-scanner`'s proc-macros. `river-xkb-bindings-v1` references
//! `river_seat_v1` from `river-window-management-v1`, so the modules are
//! ordered to make those types available.
//!
//! See `crates/gharial/protocol/README.md` for the pinned upstream rev.

// The wayland-scanner output references `wayland_client`, `wayland_client::protocol::*`
// (for shared interfaces like wl_surface), and `__interfaces::*` for the interface
// metadata. The compiler sometimes can't see those uses inside the macro expansion
// and flags them as unused; suppress here, scoped to this file only.
#![allow(unused_imports)]

pub mod window_management {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocol/river-window-management-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-window-management-v1.xml");
}

pub mod xkb_bindings {
    use wayland_client;
    use wayland_client::protocol::*;

    // The xkb-bindings protocol references the river_seat_v1 module from
    // the window-management protocol (notably the Modifiers bitflag). The
    // scanner emits `super::river_seat_v1::...`, so we make the *module*
    // visible at this level, not just a type from inside it.
    use super::window_management::river_seat_v1;

    pub mod __interfaces {
        use super::super::window_management::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocol/river-xkb-bindings-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-xkb-bindings-v1.xml");
}

pub mod layer_shell {
    use wayland_client;
    use wayland_client::protocol::*;

    // Layer-shell references river_output_v1 and river_seat_v1 from the
    // window-management protocol — same cross-import dance as xkb-bindings.
    use super::window_management::{river_output_v1, river_seat_v1};

    pub mod __interfaces {
        use super::super::window_management::__interfaces::*;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocol/river-layer-shell-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocol/river-layer-shell-v1.xml");
}

// Convenience re-exports of the proxy types — every other module imports
// from here so the cross-protocol module paths stay in one place.
pub use layer_shell::{
    river_layer_shell_output_v1::RiverLayerShellOutputV1,
    river_layer_shell_seat_v1::RiverLayerShellSeatV1, river_layer_shell_v1::RiverLayerShellV1,
};
pub use window_management::{
    river_decoration_v1::RiverDecorationV1, river_node_v1::RiverNodeV1,
    river_output_v1::RiverOutputV1, river_pointer_binding_v1::RiverPointerBindingV1,
    river_seat_v1::RiverSeatV1, river_shell_surface_v1::RiverShellSurfaceV1,
    river_window_manager_v1::RiverWindowManagerV1, river_window_v1::RiverWindowV1,
};
pub use xkb_bindings::{
    river_xkb_binding_v1::RiverXkbBindingV1, river_xkb_bindings_seat_v1::RiverXkbBindingsSeatV1,
    river_xkb_bindings_v1::RiverXkbBindingsV1,
};
