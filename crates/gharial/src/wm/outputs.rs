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
        match self.non_exclusive_area {
            Some(area) if area.w > 0 && area.h > 0 => area,
            _ => self.rect(),
        }
    }
}

#[derive(Default)]
pub struct Outputs {
    by_id: HashMap<ObjectId, OutputEntry>,
    order: Vec<ObjectId>,
    /// The output that receives new windows, tag commands, and keyboard
    /// focus restoration. `None` only while no outputs exist; healed by
    /// `ensure_focused` at the top of every manage sequence.
    focused: Option<ObjectId>,
}

impl Outputs {
    pub fn get(&self, id: &ObjectId) -> Option<&OutputEntry> {
        self.by_id.get(id)
    }

    pub fn get_mut(&mut self, id: &ObjectId) -> Option<&mut OutputEntry> {
        self.by_id.get_mut(id)
    }

    /// First output the compositor advertised. Fallback home for
    /// windows and layer-shell defaults when nothing better is known.
    pub fn first(&self) -> Option<&OutputEntry> {
        self.order.first().and_then(|id| self.by_id.get(id))
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

    /// Advertisement-order neighbour of the focused output. `forward`
    /// is `next`; wraps around.
    pub fn cycle_from_focused(&self, forward: bool) -> Option<ObjectId> {
        if self.order.is_empty() {
            return None;
        }
        let current = self
            .focused
            .as_ref()
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

    pub fn iter_ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.order.iter().cloned()
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
        self.by_id.remove(id)
    }
}

#[cfg(test)]
mod tests {
    //! `OutputEntry` carries the wayland proxy, which is impractical to
    //! construct in a unit test. The fields we care about for layout
    //! (position, dimensions, non_exclusive_area, tiling_area) live on
    //! plain data, so we verify them via a stand-in helper that mirrors
    //! the production geometry math.
    use super::*;

    fn compute_tiling_area(
        position: (i32, i32),
        dimensions: (i32, i32),
        non_exclusive_area: Option<Rect>,
    ) -> Rect {
        match non_exclusive_area {
            Some(area) if area.w > 0 && area.h > 0 => area,
            _ => Rect {
                x: position.0,
                y: position.1,
                w: dimensions.0.max(0) as u32,
                h: dimensions.1.max(0) as u32,
            },
        }
    }

    #[test]
    fn tiling_area_falls_back_to_output_rect_until_layer_event_arrives() {
        let area = compute_tiling_area((10, 20), (1920, 1080), None);
        assert_eq!(
            area,
            Rect {
                x: 10,
                y: 20,
                w: 1920,
                h: 1080
            }
        );
    }

    #[test]
    fn tiling_area_uses_non_exclusive_when_present() {
        let nea = Rect {
            x: 0,
            y: 30,
            w: 1920,
            h: 1050,
        };
        let area = compute_tiling_area((0, 0), (1920, 1080), Some(nea));
        assert_eq!(area, nea);
    }

    #[test]
    fn tiling_area_falls_back_when_non_exclusive_is_zero_sized() {
        // River may report a zero-w/h area transiently; we should not
        // hand the layout an empty box, just fall back to the output.
        let nea = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        let area = compute_tiling_area((0, 0), (800, 600), Some(nea));
        assert_eq!(
            area,
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600
            }
        );
    }

    #[test]
    fn tiling_area_clamps_negative_output_dimensions() {
        let area = compute_tiling_area((0, 0), (-1, -1), None);
        assert_eq!(
            area,
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0
            }
        );
    }
}
