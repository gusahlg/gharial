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
    /// Layer-shell focus tracker for this seat.
    pub layer_shell: Option<RiverLayerShellSeatV1>,
    /// Which kind of keyboard focus a layer surface holds, if any.
    pub layer_focus: LayerFocus,
    /// Last pointer position reported via `pointer_position`, in the
    /// compositor's global logical coordinates. Used to avoid redundant
    /// pointer warps during explicit output-focus changes.
    pub last_pointer: Option<(i32, i32)>,
}

impl SeatEntry {
    pub fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            wl_seat_name: None,
            focused: None,
            layer_shell: None,
            layer_focus: LayerFocus::None,
            last_pointer: None,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }

    /// A launcher holds exclusive focus: the server ignores focus
    /// requests, and making one would only fight it — leave focus alone.
    pub fn layer_has_exclusive_focus(&self) -> bool {
        self.layer_focus == LayerFocus::Exclusive
    }
}

/// Layer-shell keyboard focus on a seat. With `NonExclusive` (on-demand
/// panels) the window manager stays in control and may move focus away;
/// with `Exclusive` (launchers) focus requests are ignored until it ends.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayerFocus {
    None,
    Exclusive,
    NonExclusive,
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

    pub fn primary_mut(&mut self) -> Option<&mut SeatEntry> {
        let id = self.order.first()?.clone();
        self.by_id.get_mut(&id)
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
