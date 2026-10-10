//! Per-window state. One `WindowEntry` per `river_window_v1` we hold.
//!
//! Window lifecycle:
//!   * `manager.window` creates the proxy and an *unplaced* entry here;
//!     `get_node` reserves its render-list handle right away.
//!   * River sends the window's app_id/title/pid/parent before the next
//!     `manage_start`, where [`super::placement`] resolves defaults and
//!     rules into its output, tags, and floating/fullscreen state — so
//!     the very first configure the client sees already has its final
//!     size.
//!   * Every manage sequence reconciles desired state against what we
//!     last sent (the `*_sent` mirrors); render sequences position,
//!     show/hide, border, and stack it.

use std::collections::HashMap;

use gharial_ipc::FloatPos;
use wayland_client::backend::ObjectId;
use wayland_client::{Proxy, QueueHandle};

use crate::layout::Rect;
use crate::wayland_proto::{RiverNodeV1, RiverWindowV1};

use super::world::World;

/// The window's preferred size limits (`dimensions_hint`); 0 = none.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SizeHint {
    pub min: (i32, i32),
    pub max: (i32, i32),
}

impl SizeHint {
    /// A window that can only be one size — dialogs, splash screens.
    pub fn is_fixed(&self) -> bool {
        self.min.0 > 0 && self.min.1 > 0 && self.min == self.max
    }
}

/// Where a window lived before its output disappeared, so it can go
/// home when an output with that connector name returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evacuated {
    pub output_name: String,
    pub tags: u32,
}

#[derive(Debug)]
pub struct WindowEntry {
    /// gharial's own id: small, never reused while the daemon runs.
    pub uid: u64,
    pub proxy: RiverWindowV1,
    pub node: RiverNodeV1,

    // ── What the window told us ──────────────────────────────────────
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub pid: Option<u32>,
    pub parent: Option<ObjectId>,
    pub hint: SizeHint,
    /// `river_window_v1.presentation_hint` (0 = vsync, 1 = async).
    pub presentation_hint: Option<u32>,

    // ── Policy state ─────────────────────────────────────────────────
    /// Initial placement (defaults + rules) has been applied.
    pub placed: bool,
    pub tags: u32,
    /// The output this window lives on. `None` (or a removed output's
    /// id) is healed to the focused output at the next manage sequence.
    pub output: Option<ObjectId>,
    pub evacuated: Option<Evacuated>,
    pub floating: bool,
    pub fullscreen: bool,
    /// Visible on every tag of its output.
    pub sticky: bool,
    /// Geometry frozen: once sized and positioned, layout changes never
    /// resize or move it. Explicit commands still do.
    pub locked: bool,
    /// Draw gharial's border around it.
    pub border: bool,
    /// `nofocus`: never handed keyboard focus automatically (on open, or
    /// as the fallback when the focused window goes away). Clicking it,
    /// directional focus, or `window … focus` still focus it.
    pub no_auto_focus: bool,
    /// Floating content size; `None` lets the client choose.
    pub float_size: Option<(u32, u32)>,
    /// Floating position relative to its output's top-left corner.
    /// `Center` resolves to a concrete offset once the size is known.
    pub float_pos: Option<FloatPos>,
    /// Whether the window is on a visible tag (or sticky) of its output.
    pub visible: bool,
    /// Stacking serial: higher sits above lower within its layer.
    pub raised: u64,
    /// Serial of the last time it took keyboard focus (0 = never);
    /// directional focus prefers recently used windows on ties.
    pub focused_at: u64,

    // ── Mirrors of what the server last heard / told us ──────────────
    pub proposed: Option<(i32, i32)>,
    /// Last size the window confirmed (`dimensions` event).
    pub actual: Option<(i32, i32)>,
    /// A proposal or fullscreen change is outstanding.
    pub awaiting_dims: bool,
    pub position: Option<(i32, i32)>,
    pub hidden_on_server: bool,
    /// Output the server currently has it fullscreen on.
    pub fullscreen_on: Option<ObjectId>,
    pub borders_sent: Option<(u32, bool)>,
    pub ssd_sent: bool,
    pub tiled_sent: Option<bool>,
    pub bounds_sent: Option<(i32, i32)>,
    /// A render sequence finished after it was sized and positioned:
    /// it is on screen (if visible) at its current geometry.
    pub rendered: bool,
    /// It has been settled at least once (sticky) — `wait-window --new`
    /// ignores such windows even while they are being resized.
    pub ever_settled: bool,
    /// Oneshot rules this window consumed, kept so a late app_id/title
    /// re-placement doesn't lose them (a placed `spawn`, say).
    pub consumed_rules: Vec<gharial_ipc::Rule>,
}

impl WindowEntry {
    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }

    /// The content rectangle in global coordinates, once known.
    pub fn content_rect(&self) -> Option<Rect> {
        let (w, h) = self.actual?;
        let (x, y) = self.position?;
        Some(Rect {
            x,
            y,
            w: w.max(0) as u32,
            h: h.max(0) as u32,
        })
    }

    /// Participates in the tiling layout.
    pub fn is_tiled(&self) -> bool {
        !self.floating && !self.fullscreen
    }

    /// Placed, sized, and drawn at its final geometry (or, on a hidden
    /// tag, sized and ready to show) — what `wait-window` waits for.
    /// `rendered` is computed in the render flush with exactly those
    /// conditions.
    pub fn is_settled(&self) -> bool {
        self.placed && self.actual.is_some() && !self.awaiting_dims && self.rendered
    }

    /// Forget what we last sent about geometry so the next manage/render
    /// reissues it (after a placement change or leaving fullscreen).
    pub fn reset_geometry(&mut self) {
        self.proposed = None;
        self.position = None;
        self.borders_sent = None;
    }
}

/// All known windows plus their stack order (insertion order, reordered
/// by `swap`) — the layout uses that order for master-stack slots.
#[derive(Default)]
pub struct Windows {
    by_id: HashMap<ObjectId, WindowEntry>,
    order: Vec<ObjectId>,
    next_uid: u64,
}

impl Windows {
    pub fn get(&self, id: &ObjectId) -> Option<&WindowEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut WindowEntry> {
        self.by_id.get_mut(id)
    }

    pub fn by_uid(&self, uid: u64) -> Option<ObjectId> {
        self.iter().find(|w| w.uid == uid).map(WindowEntry::id)
    }

    /// Entries in stack order.
    pub fn iter(&self) -> impl Iterator<Item = &WindowEntry> {
        self.order.iter().filter_map(|id| self.by_id.get(id))
    }

    pub fn ids(&self) -> Vec<ObjectId> {
        self.order.clone()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut WindowEntry> {
        self.by_id.values_mut()
    }

    /// Visible windows on `output`, in stack order.
    pub fn visible_on(&self, output: &ObjectId) -> Vec<ObjectId> {
        self.iter()
            .filter(|w| w.visible && w.output.as_ref() == Some(output))
            .map(WindowEntry::id)
            .collect()
    }

    /// Visible tiled windows on `output`, in stack order — the per-output
    /// layout input.
    pub fn tiled_on(&self, output: &ObjectId) -> Vec<ObjectId> {
        self.iter()
            .filter(|w| w.visible && w.is_tiled() && w.output.as_ref() == Some(output))
            .map(WindowEntry::id)
            .collect()
    }

    pub fn is_visible(&self, id: &ObjectId) -> bool {
        self.by_id.get(id).is_some_and(|w| w.visible)
    }

    /// Borrow the stack order and the entry map disjointly so a caller
    /// can walk windows in order while mutating each entry.
    pub fn split_mut(&mut self) -> (&[ObjectId], &mut HashMap<ObjectId, WindowEntry>) {
        (&self.order, &mut self.by_id)
    }

    pub fn insert(&mut self, mut entry: WindowEntry) {
        self.next_uid += 1;
        entry.uid = self.next_uid;
        let id = entry.id();
        self.order.push(id.clone());
        self.by_id.insert(id, entry);
    }

    pub fn remove(&mut self, id: &ObjectId) -> Option<WindowEntry> {
        self.order.retain(|i| i != id);
        self.by_id.remove(id)
    }

    pub fn swap_ids(&mut self, a: &ObjectId, b: &ObjectId) -> bool {
        let i = self.order.iter().position(|o| o == a);
        let j = self.order.iter().position(|o| o == b);
        match (i, j) {
            (Some(i), Some(j)) => {
                self.order.swap(i, j);
                true
            }
            _ => false,
        }
    }
}

/// A fresh, unplaced entry for a window the manager just announced.
/// `get_node` is not sequence-restricted, so we can call it in any phase.
pub fn entry_for(proxy: RiverWindowV1, qh: &QueueHandle<World>) -> WindowEntry {
    let node = proxy.get_node(qh, ());
    WindowEntry {
        uid: 0,
        proxy,
        node,
        app_id: None,
        title: None,
        pid: None,
        parent: None,
        hint: SizeHint::default(),
        presentation_hint: None,
        placed: false,
        tags: 1,
        output: None,
        evacuated: None,
        floating: false,
        fullscreen: false,
        sticky: false,
        locked: false,
        border: true,
        no_auto_focus: false,
        float_size: None,
        float_pos: None,
        visible: false,
        raised: 0,
        focused_at: 0,
        proposed: None,
        actual: None,
        awaiting_dims: false,
        position: None,
        hidden_on_server: false,
        fullscreen_on: None,
        borders_sent: None,
        ssd_sent: false,
        tiled_sent: None,
        bounds_sent: None,
        rendered: false,
        ever_settled: false,
        consumed_rules: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_size_hint_needs_equal_nonzero_bounds() {
        let fixed = SizeHint {
            min: (400, 300),
            max: (400, 300),
        };
        assert!(fixed.is_fixed());
        assert!(!SizeHint::default().is_fixed());
        assert!(!SizeHint {
            min: (400, 300),
            max: (800, 600),
        }
        .is_fixed());
        assert!(!SizeHint {
            min: (0, 300),
            max: (0, 300),
        }
        .is_fixed());
    }
}
