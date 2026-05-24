//! Per-output state.
//!
//! Outputs come and go via `manager.output` / `output.removed`. Each
//! output reports its logical position and size as a pair of events
//! right after creation, and again whenever they change. We don't need
//! to bind `wl_output` directly — the `output.wl_output` event hands us
//! the global name if we ever do.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::wayland_proto::{RiverLayerShellOutputV1, RiverOutputV1};

#[derive(Debug)]
pub struct OutputEntry {
    pub proxy: RiverOutputV1,
    pub wl_output_name: Option<u32>,
    pub position: (i32, i32),
    pub dimensions: (i32, i32),
    /// Layer-shell handle for this output, created via
    /// `river_layer_shell_v1.get_output`. Holding it tells river we
    /// want layer surfaces on this output. `None` until the manager
    /// dispatch creates it.
    pub layer_shell: Option<RiverLayerShellOutputV1>,
    /// `true` once we've made this output the layer-shell default
    /// (`set_default` request). One-shot per session; only one output
    /// may be the default at a time.
    pub layer_shell_default_sent: bool,
    /// Area of this output left after subtracting layer-surface
    /// exclusive zones (waybar / panels / docks). Reported by
    /// `river_layer_shell_output_v1.non_exclusive_area` in *global*
    /// compositor coordinates. `None` until that event has fired.
    pub non_exclusive_area: Option<Rect>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl OutputEntry {
    pub fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            wl_output_name: None,
            position: (0, 0),
            dimensions: (0, 0),
            layer_shell: None,
            layer_shell_default_sent: false,
            non_exclusive_area: None,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }

    pub fn has_dimensions(&self) -> bool {
        self.dimensions.0 > 0 && self.dimensions.1 > 0
    }

    /// Effective tiling area for this output — non-exclusive area when
    /// the layer-shell has reported one, otherwise the full output
    /// rectangle. Returned as (origin_x, origin_y, width, height) in
    /// *global* coordinates, ready for the layout origin.
    pub fn tiling_area(&self) -> (i32, i32, u32, u32) {
        match self.non_exclusive_area {
            Some(area) if area.w > 0 && area.h > 0 => (area.x, area.y, area.w, area.h),
            _ => (
                self.position.0,
                self.position.1,
                self.dimensions.0.max(0) as u32,
                self.dimensions.1.max(0) as u32,
            ),
        }
    }
}

#[derive(Default)]
pub struct Outputs {
    by_id: HashMap<ObjectId, OutputEntry>,
    order: Vec<ObjectId>,
}

impl Outputs {
    pub fn get(&self, id: &ObjectId) -> Option<&OutputEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut OutputEntry> {
        self.by_id.get_mut(id)
    }

    /// Output we'll attach windows to. v0.2 has no per-output window
    /// assignment policy — everything goes on the primary, defined as
    /// the first output the compositor advertised.
    pub fn primary(&self) -> Option<&OutputEntry> {
        self.order.first().and_then(|id| self.by_id.get(id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &OutputEntry> {
        self.order.iter().filter_map(|id| self.by_id.get(id))
    }

    pub fn iter_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.order.iter().cloned()
    }

    pub fn insert(&mut self, entry: OutputEntry) {
        let id = entry.id();
        self.order.push(id.clone());
        self.by_id.insert(id, entry);
    }

    pub fn remove(&mut self, id: &ObjectId) -> Option<OutputEntry> {
        self.order.retain(|i| i != id);
        self.by_id.remove(id)
    }
}
