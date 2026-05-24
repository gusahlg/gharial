//! Per-seat state. Owns "which window does this seat focus" — river's
//! protocol leaves focus tracking to the WM, so this is the source of
//! truth for keyboard focus.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::wayland_proto::{RiverLayerShellSeatV1, RiverSeatV1};

#[derive(Debug)]
pub struct SeatEntry {
    pub proxy: RiverSeatV1,
    pub wl_seat_name: Option<u32>,
    /// Window the WM has told the compositor this seat should focus.
    /// `None` after `clear_focus`, or before any explicit focus.
    pub focused: Option<ObjectId>,
    /// Last window the pointer entered; useful for click-to-focus
    /// policies, currently informational only.
    pub pointer_over: Option<ObjectId>,
    /// Layer-shell focus tracker for this seat.
    pub layer_shell: Option<RiverLayerShellSeatV1>,
    /// `true` while a layer surface holds keyboard focus (exclusive
    /// or non-exclusive). While set, gharial leaves seat focus alone
    /// so launchers like tofi can actually receive input.
    pub layer_focus_active: bool,
}

impl SeatEntry {
    pub fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            wl_seat_name: None,
            focused: None,
            pointer_over: None,
            layer_shell: None,
            layer_focus_active: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }
}

#[derive(Default)]
pub struct Seats {
    by_id: HashMap<ObjectId, SeatEntry>,
    order: Vec<ObjectId>,
}

impl Seats {
    pub fn get(&self, id: &ObjectId) -> Option<&SeatEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut SeatEntry> {
        self.by_id.get_mut(id)
    }

    pub fn primary(&self) -> Option<&SeatEntry> {
        self.order.first().and_then(|id| self.by_id.get(id))
    }

    /// Iterate over the seat IDs in insertion order — used by dispatch
    /// impls that need to find a seat by an unrelated proxy id.
    pub fn iter_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.order.iter().cloned()
    }

    pub fn insert(&mut self, entry: SeatEntry) {
        let id = entry.id();
        self.order.push(id.clone());
        self.by_id.insert(id, entry);
    }

    pub fn remove(&mut self, id: &ObjectId) -> Option<SeatEntry> {
        self.order.retain(|i| i != id);
        self.by_id.remove(id)
    }
}
