//! Mode actions and binding definitions. Entering a mode validates it
//! against the bindings that exist, so a typo can't soft-lock the
//! keyboard.

use crate::action::{Action, BindingSpec};

use super::super::modes::DEFAULT_MODE;
use super::super::world::World;
use super::Outcome;

pub fn enter_mode(world: &mut World, name: &str) -> Outcome {
    if name != DEFAULT_MODE && !world.bindings.has_mode(name) {
        // An unknown mode would disable every binding (none match),
        // leaving no key to get back out with.
        return Err(format!(
            "mode {name:?} has no bindings — refusing to switch (would soft-lock the keyboard)"
        ));
    }
    if world.modes.active != name {
        world.modes.active = name.to_string();
        world.bindings.dirty = true;
    }
    Ok(String::new())
}

pub fn bind(world: &mut World, mode: String, spec: BindingSpec, action: Action) -> Outcome {
    world.bindings.define(mode, spec, action);
    Ok(String::new())
}

pub fn unbind(world: &mut World, mode: &str, spec: &BindingSpec) -> Outcome {
    if world.bindings.undefine(mode, spec) {
        Ok(String::new())
    } else {
        Err(format!("no such binding in mode {mode:?}"))
    }
}
