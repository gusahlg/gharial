//! Per-output state.
//!
//! Outputs come and go via `manager.output` / `output.removed`. Each
//! output reports its logical position and size as a pair of events
//! right after creation, and again whenever they change. We bind the
//! corresponding `wl_output` global (the `output.wl_output` event hands
//! us the global name) to learn the connector name (`DP-1`, …) so users
//! can refer to outputs stably in config.
//!
//! Every output is an independent view into the tag space: it carries
//! its own active tag mask and its own per-tag focus memory. Exactly one
//! output is *focused* at a time — new windows land there, tag commands
//! apply there, and keyboard focus is restored from its memory.

use std::collections::HashMap;

use gharial_ipc::Presentation;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::Proxy;

use crate::layout::Rect;
use crate::wayland_proto::{RiverLayerShellOutputV1, RiverOutputV1};

use super::focus::FocusMemory;

#[derive(Debug)]
pub struct OutputEntry {
    pub proxy: RiverOutputV1,
    pub wl_output_name: Option<u32>,
    /// Bound `wl_output` global — held to receive the `name` event.
    pub wl_output: Option<WlOutput>,
    /// Connector name reported by `wl_output.name` (`DP-1`, `HDMI-A-1`).
    /// `None` until the event arrives (or forever on wl_output < v4).
    pub name: Option<String>,
    pub position: (i32, i32),
    pub dimensions: (i32, i32),
    /// Bitmask of tags visible on this output. Every output starts on
    /// tag 1, like the single-screen behaviour before multi-output.
    pub active_tags: u32,
    /// Per-tag "last focused window" memory for this output.
    pub focus: FocusMemory<ObjectId>,
    /// Layer-shell handle for this output, created via
    /// `river_layer_shell_v1.get_output`. Holding it tells river we
    /// want layer surfaces on this output.
    pub layer_shell: Option<RiverLayerShellOutputV1>,
    /// Desired presentation mode, and the raw mode last sent (render
    /// state, reconciled in `flush_render`).
    pub presentation: Presentation,
    pub presentation_sent: Option<u32>,
    /// Area of this output left after subtracting layer-surface
    /// exclusive zones (waybar / panels / docks). Reported by
    /// `river_layer_shell_output_v1.non_exclusive_area` in *global*
    /// compositor coordinates. `None` until that event has fired.
    pub non_exclusive_area: Option<Rect>,
}

impl OutputEntry {
    pub fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            wl_output_name: None,
            wl_output: None,
            name: None,
            position: (0, 0),
            dimensions: (0, 0),
            active_tags: 1,
            focus: FocusMemory::default(),
            layer_shell: None,
            presentation: Presentation::Vsync,
            presentation_sent: None,
            non_exclusive_area: None,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.proxy.id()
    }

    pub fn has_dimensions(&self) -> bool {
        self.dimensions.0 > 0 && self.dimensions.1 > 0
    }

    /// The full output rectangle in global compositor coordinates.
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.position.0,
            y: self.position.1,
            w: self.dimensions.0.max(0) as u32,
            h: self.dimensions.1.max(0) as u32,
        }
    }

    /// Effective tiling area for this output — non-exclusive area when
    /// the layer-shell has reported one, otherwise the full output
    /// rectangle. Coordinates are *global* (matching the layout origin).
    pub fn tiling_area(&self) -> Rect {
        usable_area(self.rect(), self.non_exclusive_area)
    }

    /// Name for listings and rules: the connector, or `fallback`.
    pub fn display_name(&self, fallback: usize) -> String {
        self.name.clone().unwrap_or_else(|| fallback.to_string())
    }

    pub fn contains(&self, (x, y): (i32, i32)) -> bool {
        let r = self.rect();
        x >= r.x && x < r.x + r.w as i32 && y >= r.y && y < r.y + r.h as i32
    }
}

/// The usable area: the layer-shell's non-exclusive area when it is a
/// sane sub-rectangle of the output, else the whole output. River may
/// report a zero-sized or stale area transiently (mode changes); the
/// layout must never be handed an empty or off-screen box.
pub fn usable_area(output: Rect, non_exclusive: Option<Rect>) -> Rect {
    match non_exclusive {
        Some(area) if area.w > 0 && area.h > 0 && intersects(area, output) => area,
        _ => output,
    }
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w as i32
        && b.x < a.x + a.w as i32
        && a.y < b.y + b.h as i32
        && b.y < a.y + a.h as i32
}

/// Per-connector state kept across an output's removal, so a monitor
/// that reconnects (or a reconfiguration that recreates outputs) comes
/// back showing what it showed before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remembered {
    pub active_tags: u32,
    pub presentation: Presentation,
}

#[derive(Default)]
pub struct Outputs {
    by_id: HashMap<ObjectId, OutputEntry>,
    order: Vec<ObjectId>,
    /// The output that receives new windows, tag commands, and keyboard
    /// focus restoration. `None` only while no outputs exist; healed by
    /// `ensure_focused` at the top of every manage sequence.
    focused: Option<ObjectId>,
    /// The output we last made the layer-shell default.
    pub layer_default: Option<ObjectId>,
    /// State of removed outputs, by connector name.
    pub remembered: HashMap<String, Remembered>,
}

impl Outputs {
    pub fn get(&self, id: &ObjectId) -> Option<&OutputEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut OutputEntry> {
        self.by_id.get_mut(id)
    }

    /// The currently focused output's id, if any.
    pub fn focused_id(&self) -> Option<ObjectId> {
        self.focused.clone()
    }

    pub fn focused(&self) -> Option<&OutputEntry> {
        self.focused.as_ref().and_then(|id| self.by_id.get(id))
    }

    pub fn focused_mut(&mut self) -> Option<&mut OutputEntry> {
        let id = self.focused.clone()?;
        self.by_id.get_mut(&id)
    }

    /// Point output focus at `id`. No-op if the output is unknown.
    pub fn set_focused(&mut self, id: &ObjectId) {
        if self.by_id.contains_key(id) {
            self.focused = Some(id.clone());
        }
    }

    /// Heal the focused-output pointer: if it is unset or refers to a
    /// removed output, fall back to the first advertised output.
    /// Returns the (possibly fresh) focused id.
    pub fn ensure_focused(&mut self) -> Option<ObjectId> {
        match &self.focused {
            Some(id) if self.by_id.contains_key(id) => Some(id.clone()),
            _ => {
                self.focused = self.order.first().cloned();
                self.focused.clone()
            }
        }
    }

    /// Advertisement-order neighbour of `from` (default: the focused
    /// output). `forward` is `next`; wraps around.
    pub fn cycle_from(&self, from: Option<&ObjectId>, forward: bool) -> Option<ObjectId> {
        if self.order.is_empty() {
            return None;
        }
        let current = from
            .or(self.focused.as_ref())
            .and_then(|id| self.order.iter().position(|o| o == id))
            .unwrap_or(0);
        let len = self.order.len();
        let idx = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
        Some(self.order[idx].clone())
    }

    /// Resolve a user-supplied output token: connector name first, then
    /// a 1-based index into the advertisement order.
    pub fn resolve_named(&self, token: &str) -> Option<ObjectId> {
        for entry in self.iter() {
            if entry.name.as_deref() == Some(token) {
                return Some(entry.id());
            }
        }
        let idx: usize = token.parse().ok()?;
        if idx == 0 {
            return None;
        }
        self.order.get(idx - 1).cloned()
    }

    /// Find the output entry that owns a bound `wl_output` proxy — used
    /// by the wl_output dispatch to route `name` events.
    pub fn id_by_wl_output(&self, wl_output_id: &ObjectId) -> Option<ObjectId> {
        self.iter()
            .find(|entry| entry.wl_output.as_ref().map(Proxy::id).as_ref() == Some(wl_output_id))
            .map(|entry| entry.id())
    }

    pub fn iter(&self) -> impl Iterator<Item = &OutputEntry> {
        self.order.iter().filter_map(|id| self.by_id.get(id))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut OutputEntry> {
        self.by_id.values_mut()
    }

    /// 1-based advertisement index — the name fallback for listings.
    pub fn index_of(&self, id: &ObjectId) -> Option<usize> {
        self.order.iter().position(|o| o == id).map(|i| i + 1)
    }

    /// Listing name: connector, or 1-based index.
    pub fn name_of(&self, id: &ObjectId) -> Option<String> {
        let entry = self.by_id.get(id)?;
        Some(entry.display_name(self.index_of(id).unwrap_or(0)))
    }

    pub fn insert(&mut self, entry: OutputEntry) {
        let id = entry.id();
        self.order.push(id.clone());
        self.by_id.insert(id.clone(), entry);
        if self.focused.is_none() {
            self.focused = Some(id);
        }
    }

    pub fn remove(&mut self, id: &ObjectId) -> Option<OutputEntry> {
        self.order.retain(|i| i != id);
        if self.focused.as_ref() == Some(id) {
            self.focused = self.order.first().cloned();
        }
        if self.layer_default.as_ref() == Some(id) {
            self.layer_default = None;
        }
        let entry = self.by_id.remove(id)?;
        if let Some(name) = &entry.name {
            self.remembered.insert(
                name.clone(),
                Remembered {
                    active_tags: entry.active_tags,
                    presentation: entry.presentation,
                },
            );
        }
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn usable_area_falls_back_to_output_rect_until_layer_event_arrives() {
        assert_eq!(
            usable_area(r(10, 20, 1920, 1080), None),
            r(10, 20, 1920, 1080)
        );
    }

    #[test]
    fn usable_area_uses_non_exclusive_when_present() {
        let nea = r(0, 30, 1920, 1050);
        assert_eq!(usable_area(r(0, 0, 1920, 1080), Some(nea)), nea);
    }

    #[test]
    fn usable_area_ignores_degenerate_or_stale_areas() {
        let out = r(0, 0, 800, 600);
        assert_eq!(usable_area(out, Some(r(0, 0, 0, 0))), out);
        // An area from before the output moved: entirely elsewhere.
        assert_eq!(usable_area(out, Some(r(5000, 0, 800, 570))), out);
    }
}
