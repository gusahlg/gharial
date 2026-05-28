//! Per-window state. One `WindowEntry` per `river_window_v1` we hold.
//!
//! Window lifecycle:
//!   * `manager.window` event creates the proxy and an entry here.
//!   * We call `window.get_node()` immediately to reserve our render-list
//!     handle (it isn't sequence-restricted).
//!   * On the next manage sequence we issue `propose_dimensions` if our
//!     desired size has changed.
//!   * On the following render sequence we position the window via the
//!     node we reserved.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::{Proxy, QueueHandle};

use crate::wayland_proto::{RiverNodeV1, RiverWindowV1};

use super::world::World;

#[derive(Debug)]
pub struct WindowEntry {
    pub proxy: RiverWindowV1,
    pub node: RiverNodeV1,
    pub app_id: Option<String>,
    pub title: Option<String>,
    /// The last (width, height) we sent via propose_dimensions.
    pub proposed: Option<(i32, i32)>,
    /// The last (width, height) the window confirmed via the dimensions
    /// event. None until the window has acknowledged at least once; the
    /// window is not visible to the user until then.
    pub actual: Option<(i32, i32)>,
    /// The last (x, y) we sent via node.set_position.
    pub position: Option<(i32, i32)>,
    /// Tag bitmask this window belongs to.
    pub tags: u32,
    /// Whether the window is currently on a visible tag. Recomputed
    /// after any tag-state change; flushed to the server via show/hide
    /// during the next render sequence.
    pub visible: bool,
    /// Whether we've issued `hide` for this window (so it isn't drawn).
    /// Tracks server-side state to avoid redundant requests.
    pub hidden_on_server: bool,
    /// Floating windows keep their own size and are skipped by the
    /// tiling layout. They still participate in focus and tag visibility.
    pub floating: bool,
    /// The (width, focused?) tuple we last sent via set_borders, so we
    /// can avoid redundant wire chatter and re-send only on real change.
    pub borders_signature: Option<(u32, bool)>,
    /// `true` once we've sent `use_ssd` to suppress CSD titlebars and
    /// app-drawn borders. One-shot per window.
    pub csd_disabled: bool,
    /// Track which `set_tiled` edges we last announced so we can flip
    /// it when a window toggles between tiled and floating without
    /// flooding the wire on every render.
    pub tiled_edges_sent: Option<bool>,
}

impl WindowEntry {
    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }
}

/// All currently-known windows plus an insertion-ordered list of their
/// IDs — the layout uses that order for the master-stack slot index.
#[derive(Default)]
pub struct Windows {
    by_id: HashMap<ObjectId, WindowEntry>,
    order: Vec<ObjectId>,
}

impl Windows {
    pub fn get(&self, id: &ObjectId) -> Option<&WindowEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut WindowEntry> {
        self.by_id.get_mut(id)
    }

    /// Snapshot of the insertion-ordered ID list. Callers that need to
    /// mutate each entry should pair this with `get_mut` — safer than a
    /// streaming `&mut` iterator that has to fight aliasing.
    pub fn ordered_ids(&self) -> Vec<ObjectId> {
        self.order.clone()
    }

    pub fn visible_ids(&self) -> Vec<ObjectId> {
        self.order
            .iter()
            .filter(|id| self.is_visible(id))
            .cloned()
            .collect()
    }

    pub fn visible_tiled_ids(&self) -> Vec<ObjectId> {
        self.order
            .iter()
            .filter(|id| self.is_visible_tiled(id))
            .cloned()
            .collect()
    }

    pub fn is_visible(&self, id: &ObjectId) -> bool {
        self.by_id.get(id).is_some_and(|w| w.visible)
    }

    pub fn is_visible_tiled(&self, id: &ObjectId) -> bool {
        self.by_id.get(id).is_some_and(|w| w.visible && !w.floating)
    }

    pub fn index_of(&self, id: &ObjectId) -> Option<usize> {
        self.order.iter().position(|other| other == id)
    }

    pub fn insert(&mut self, entry: WindowEntry) {
        let id = entry.id();
        self.order.push(id.clone());
        self.by_id.insert(id, entry);
    }

    pub fn remove(&mut self, id: &ObjectId) -> Option<WindowEntry> {
        self.order.retain(|i| i != id);
        self.by_id.remove(id)
    }

    pub fn swap(&mut self, i: usize, j: usize) {
        self.order.swap(i, j);
    }

    pub fn swap_ids(&mut self, a: &ObjectId, b: &ObjectId) -> bool {
        let (Some(i), Some(j)) = (self.index_of(a), self.index_of(b)) else {
            return false;
        };
        self.swap(i, j);
        true
    }
}

/// Construct a fresh entry for a window the manager just announced.
/// `get_node` is not sequence-restricted, so we can call it here in any
/// phase. New windows are placed on the currently-active tags so they
/// appear in front of the user immediately.
pub fn entry_for(proxy: RiverWindowV1, qh: &QueueHandle<World>, tags: u32) -> WindowEntry {
    let node = proxy.get_node(qh, ());
    WindowEntry {
        proxy,
        node,
        app_id: None,
        title: None,
        proposed: None,
        actual: None,
        position: None,
        tags,
        visible: true,
        hidden_on_server: false,
        floating: false,
        borders_signature: None,
        csd_disabled: false,
        tiled_edges_sent: None,
    }
}
