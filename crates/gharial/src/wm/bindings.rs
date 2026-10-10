//! Keyboard bindings: definitions, and their per-seat protocol objects.
//!
//! A definition is a (mode, chord) → action entry and exists whether or
//! not any seat does — a `bind` issued before river announces a seat is
//! kept, not dropped. [`sync`] materialises one `river_xkb_binding_v1`
//! per (seat, definition) and enables exactly those in the active mode.
//! It runs inside a manage sequence (enable/disable are manage-bucket)
//! whenever definitions, the active mode, or the seat set change.

use std::collections::{HashMap, HashSet};

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::action::{Action, BindingSpec};
use crate::wayland_proto::window_management::river_seat_v1::Modifiers;
use crate::wayland_proto::RiverXkbBindingV1;

use super::world::World;

pub type Key = (String, BindingSpec);

struct Bound {
    seat: ObjectId,
    key: Key,
    proxy: RiverXkbBindingV1,
    enabled: bool,
}

#[derive(Default)]
pub struct Bindings {
    defs: HashMap<Key, Action>,
    bound: HashMap<ObjectId, Bound>,
    /// Definitions, mode switches, or seats changed since the last sync.
    pub dirty: bool,
}

impl Bindings {
    /// Add or replace a definition (last bind wins).
    pub fn define(&mut self, mode: String, spec: BindingSpec, action: Action) {
        self.defs.insert((mode, spec), action);
        self.dirty = true;
    }

    /// Remove a definition; `false` if there was none.
    pub fn undefine(&mut self, mode: &str, spec: &BindingSpec) -> bool {
        let removed = self
            .defs
            .remove(&(mode.to_string(), spec.clone()))
            .is_some();
        self.dirty |= removed;
        removed
    }

    /// The action a triggered binding object stands for.
    pub fn action_for(&self, proxy: &ObjectId) -> Option<&Action> {
        self.defs.get(&self.bound.get(proxy)?.key)
    }

    /// Whether any binding lives in `mode` — entering a mode with none
    /// would disable every key with no way back.
    pub fn has_mode(&self, mode: &str) -> bool {
        self.defs.keys().any(|(m, _)| m == mode)
    }
}

/// Reconcile binding objects with definitions, seats, and the active
/// mode. Must run inside a manage sequence.
pub fn sync(world: &mut World) {
    if !world.bindings.dirty {
        return;
    }
    world.bindings.dirty = false;
    let seats: HashSet<ObjectId> = world.seats.iter_ids().collect();
    let active = world.modes.active.clone();
    let bindings = &mut world.bindings;

    // Drop objects whose seat or definition is gone.
    bindings.bound.retain(|_, b| {
        let keep = seats.contains(&b.seat) && bindings.defs.contains_key(&b.key);
        if !keep {
            b.proxy.destroy();
        }
        keep
    });

    // Create missing objects.
    let have: HashSet<(ObjectId, Key)> = bindings
        .bound
        .values()
        .map(|b| (b.seat.clone(), b.key.clone()))
        .collect();
    for seat_id in &seats {
        let Some(seat) = world.seats.get(seat_id) else {
            continue;
        };
        for key in bindings.defs.keys() {
            if have.contains(&(seat_id.clone(), key.clone())) {
                continue;
            }
            let modifiers = Modifiers::from_bits_truncate(key.1.modifiers);
            let proxy = world.globals.xkb.get_xkb_binding(
                &seat.proxy,
                key.1.keysym,
                modifiers,
                &world.qh,
                (),
            );
            bindings.bound.insert(
                proxy.id(),
                Bound {
                    seat: seat_id.clone(),
                    key: key.clone(),
                    proxy,
                    enabled: false,
                },
            );
        }
    }

    // Enable exactly the active mode.
    for b in bindings.bound.values_mut() {
        let want = b.key.0 == active;
        if want != b.enabled {
            if want {
                b.proxy.enable();
            } else {
                b.proxy.disable();
            }
            b.enabled = want;
        }
    }
}
