//! Keyboard binding storage and lifecycle.
//!
//! A binding is an (mode, chord) pair plus the action to fire. The
//! compositor receives the actual key events; we register the
//! river_xkb_binding_v1 proxies and the protocol calls us back via
//! `pressed`/`released`. Only bindings whose `mode` matches the active
//! mode are `enable()`d at any moment.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::action::{Action, BindingSpec};
use crate::wayland_proto::window_management::river_seat_v1::Modifiers;
use crate::wayland_proto::RiverXkbBindingV1;

use super::world::World;

#[derive(Debug)]
pub struct BindingEntry {
    pub spec: BindingSpec,
    pub action: Action,
    pub mode: String,
    pub proxy: RiverXkbBindingV1,
    pub enabled: bool,
}

#[derive(Default)]
pub struct Bindings {
    by_proxy: HashMap<ObjectId, BindingEntry>,
    by_key: HashMap<(String, BindingSpec), ObjectId>,
}

impl Bindings {
    pub fn get_by_proxy(&self, id: &ObjectId) -> Option<&BindingEntry> {
        self.by_proxy.get(id)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut BindingEntry> {
        self.by_proxy.values_mut()
    }

    pub fn insert(&mut self, entry: BindingEntry) {
        let proxy_id = entry.proxy.id();
        let key = (entry.mode.clone(), entry.spec.clone());
        // Replace an existing binding silently — last-bind wins.
        if let Some(old_id) = self.by_key.insert(key.clone(), proxy_id.clone()) {
            if let Some(old) = self.by_proxy.remove(&old_id) {
                old.proxy.destroy();
            }
        }
        self.by_proxy.insert(proxy_id, entry);
    }

    pub fn remove(&mut self, spec: &BindingSpec, mode: &str) -> Option<BindingEntry> {
        let key = (mode.to_string(), spec.clone());
        let id = self.by_key.remove(&key)?;
        let entry = self.by_proxy.remove(&id)?;
        entry.proxy.destroy();
        Some(entry)
    }
}

/// Install a fresh binding on the primary seat. The binding's `enable`
/// happens only if its mode matches the active mode — otherwise it
/// sits dormant until the user enters that mode.
pub fn install_binding(
    world: &mut World,
    spec: BindingSpec,
    action: Action,
    mode: String,
) -> Result<(), String> {
    let seat = world
        .seats
        .primary()
        .ok_or("no seat available — wait for compositor to advertise one")?;
    let modifiers = Modifiers::from_bits_truncate(spec.modifiers);
    let proxy = world
        .globals
        .xkb
        .get_xkb_binding(&seat.proxy, spec.keysym, modifiers, &world.qh, ());
    let mut entry = BindingEntry {
        spec,
        action,
        mode,
        proxy,
        enabled: false,
    };
    if entry.mode == world.modes.active {
        entry.proxy.enable();
        entry.enabled = true;
    }
    world.bindings.insert(entry);
    Ok(())
}

/// Reconcile every binding's `enabled` state with the active mode.
/// Called from `EnterMode` / `ExitMode`. Must run inside a manage
/// sequence (enable/disable are manage-bucket).
pub fn refresh_mode_enables(world: &mut World) {
    let active = world.modes.active.clone();
    for entry in world.bindings.iter_mut() {
        let want = entry.mode == active;
        if want && !entry.enabled {
            entry.proxy.enable();
            entry.enabled = true;
        } else if !want && entry.enabled {
            entry.proxy.disable();
            entry.enabled = false;
        }
    }
}
