//! Mode-related actions: enter/exit a binding mode, install/uninstall
//! bindings. Mode-entry validates against the set of known modes to
//! prevent soft-locking the keyboard with a typoed name.

use crate::action::{Action, BindingSpec};

use super::super::bindings::{install_binding, refresh_mode_enables};
use super::super::world::World;

pub(in crate::wm) fn enter_mode(world: &mut World, name: String) {
    if !world.modes.is_known(&name) {
        // Refuse rather than soft-lock: an unknown mode would disable
        // every binding (none would match), leaving the user with no
        // keyboard to escape with. Log and keep the current mode.
        eprintln!(
            "gharial: mode {name:?} has no bindings — refusing to switch (would soft-lock the keyboard)"
        );
        return;
    }
    world.modes.active = name;
    refresh_mode_enables(world);
}

pub(in crate::wm) fn bind(world: &mut World, spec: BindingSpec, action: Action, mode: String) {
    if let Err(e) = install_binding(world, spec, action, mode) {
        eprintln!("gharial: bind failed: {e}");
    }
}

pub(in crate::wm) fn unbind(world: &mut World, spec: &BindingSpec, mode: &str) {
    world.bindings.remove(spec, mode);
}
