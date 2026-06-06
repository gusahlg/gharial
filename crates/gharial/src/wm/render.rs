//! Translates internal world state into protocol requests.
//!
//! Split into two flush points that mirror the protocol's manage/render
//! sequence:
//!
//!   * [`flush_manage`] runs inside a manage sequence. It sends one-shot
//!     `use_ssd` to suppress client-side decorations, syncs `set_tiled`
//!     state, and proposes content dimensions for visible tiled windows.
//!   * [`flush_render`] runs inside a render sequence. It reconciles
//!     `show`/`hide`, paints focus-aware `set_borders`, and positions
//!     visible tiled windows.
//!
//! Border layout invariant: the layout slot encloses the *full* window
//! including its border; the content rect is inset by `border_width`
//! on every side. That way each window owns its complete border and
//! neighbouring tiles never share or overlap border pixels.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;

use crate::layout::{self, Params, Rect};
use crate::state::BorderConfig;
use crate::wayland_proto::window_management::river_window_v1::Edges;

use super::world::World;

fn all_edges() -> Edges {
    Edges::Top | Edges::Bottom | Edges::Left | Edges::Right
}

#[derive(Debug)]
pub struct TargetCache {
    targets: HashMap<ObjectId, Rect>,
    dirty: bool,
    recompute_count: u64,
}

impl Default for TargetCache {
    fn default() -> Self {
        Self {
            targets: HashMap::new(),
            dirty: true,
            recompute_count: 0,
        }
    }
}

impl TargetCache {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn snapshot(&self) -> Option<HashMap<ObjectId, Rect>> {
        (!self.dirty).then(|| self.targets.clone())
    }

    fn replace(&mut self, targets: HashMap<ObjectId, Rect>) {
        self.targets = targets;
        self.dirty = false;
        self.recompute_count = self.recompute_count.wrapping_add(1);
    }

    #[cfg(test)]
    fn recompute_count(&self) -> u64 {
        self.recompute_count
    }
}

pub fn targets(world: &mut World) -> HashMap<ObjectId, Rect> {
    if let Some(targets) = world.target_cache.snapshot() {
        return targets;
    }
    let params = world.shared.snapshot();
    let targets = compute_targets(world, &params);
    world.target_cache.replace(targets.clone());
    targets
}

/// Compute target slot rectangles (outer, including border) for each
/// visible tiled window. Hidden (off-tag) and floating windows are
/// excluded. Takes the layout `params` so callers can hold the snapshot
/// stable across surrounding work in the same phase.
pub fn compute_targets(world: &World, params: &Params) -> HashMap<ObjectId, Rect> {
    let Some(output) = world.outputs.primary() else {
        return HashMap::new();
    };
    if !output.has_dimensions() {
        return HashMap::new();
    }

    let tiled = world.windows.visible_tiled_ids();
    if tiled.is_empty() {
        return HashMap::new();
    }

    // Subtract layer-surface exclusive zones (waybar, panels) from the
    // tiling area when river has reported them. Falls back to the full
    // output rectangle until the first non_exclusive_area event fires.
    let area = output.tiling_area();
    let rects = layout::compute(tiled.len() as u32, (area.w, area.h), params);

    let mut out = HashMap::with_capacity(tiled.len());
    for (id, rect) in tiled.into_iter().zip(rects) {
        out.insert(
            id,
            Rect {
                x: rect.x + area.x,
                y: rect.y + area.y,
                w: rect.w,
                h: rect.h,
            },
        );
    }
    out
}

/// Inset a slot rectangle by `border_width` on every side. Used to
/// derive the content rect from the layout slot. Returns `None` if the
/// slot is too small to host a border on both sides.
fn inset(slot: Rect, border: u32) -> Option<Rect> {
    let b = border as i32;
    let inner_w = (slot.w as i32) - 2 * b;
    let inner_h = (slot.h as i32) - 2 * b;
    if inner_w <= 0 || inner_h <= 0 {
        return None;
    }
    Some(Rect {
        x: slot.x + b,
        y: slot.y + b,
        w: inner_w as u32,
        h: inner_h as u32,
    })
}

/// Issue manage-bucket requests. Must be called inside a manage sequence.
/// `borders` is the snapshot the dispatcher captured for this phase.
pub fn flush_manage(world: &mut World, targets: &HashMap<ObjectId, Rect>, borders: &BorderConfig) {
    let border_width = borders.width;

    for id in world.windows.ordered_ids() {
        let Some(entry) = world.windows.get_mut(&id) else {
            continue;
        };

        // One-shot: ask every window to use server-side decorations so
        // they stop drawing their own titlebars/borders.
        if !entry.csd_disabled {
            entry.proxy.use_ssd();
            entry.csd_disabled = true;
        }

        // Tell the window whether it lives in a tiled layout. Tiled
        // windows should drop shadows/rounded corners; floating should
        // restore them.
        let want_tiled = !entry.floating;
        if entry.tiled_edges_sent != Some(want_tiled) {
            let edges = if want_tiled {
                all_edges()
            } else {
                Edges::empty()
            };
            entry.proxy.set_tiled(edges);
            entry.tiled_edges_sent = Some(want_tiled);
        }

        // Dimensions only proposed for visible tiled windows.
        if !entry.visible || entry.floating {
            continue;
        }
        let Some(slot) = targets.get(&id) else {
            continue;
        };
        let Some(content) = inset(*slot, border_width) else {
            continue;
        };
        let want = (content.w as i32, content.h as i32);
        if entry.proposed != Some(want) {
            entry.proxy.propose_dimensions(want.0, want.1);
            entry.proposed = Some(want);
        }
    }
}

/// Issue render-bucket requests. Must be called inside a render or
/// manage sequence.
///
/// Order: visibility → borders → position. set_borders fires for
/// every visible window so colour follows focus changes; positioning
/// only fires for tiled windows that have confirmed dimensions.
pub fn flush_render(world: &mut World, targets: &HashMap<ObjectId, Rect>, borders: &BorderConfig) {
    let focused = world.seats.primary().and_then(|s| s.focused.clone());
    let border_edges = if borders.width == 0 {
        Edges::empty()
    } else {
        all_edges()
    };

    for id in world.windows.ordered_ids() {
        let Some(entry) = world.windows.get_mut(&id) else {
            continue;
        };

        if entry.visible && entry.hidden_on_server {
            entry.proxy.show();
            entry.hidden_on_server = false;
        } else if !entry.visible && !entry.hidden_on_server {
            entry.proxy.hide();
            entry.hidden_on_server = true;
        }

        if !entry.visible {
            continue;
        }

        let is_focused = focused.as_ref() == Some(&id);
        let signature = (borders.width, is_focused);
        if entry.borders_signature != Some(signature) {
            let color = if is_focused {
                &borders.focused
            } else {
                &borders.unfocused
            };
            entry.proxy.set_borders(
                border_edges,
                borders.width as i32,
                color[0],
                color[1],
                color[2],
                color[3],
            );
            entry.borders_signature = Some(signature);
        }

        if entry.actual.is_none() || entry.floating {
            continue;
        }
        if let Some(slot) = targets.get(&id) {
            // Position the content area at the inset origin so the
            // border sits inside the slot, not extending beyond it.
            let inner = inset(*slot, borders.width).unwrap_or(*slot);
            let pos = (inner.x, inner.y);
            if entry.position != Some(pos) {
                entry.node.set_position(pos.0, pos.1);
                entry.position = Some(pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_cache_starts_dirty_and_reuses_clean_snapshot() {
        let mut cache = TargetCache::default();
        assert_eq!(cache.snapshot(), None);

        let targets = HashMap::new();
        cache.replace(targets);
        assert_eq!(cache.recompute_count(), 1);
        assert_eq!(cache.snapshot(), Some(HashMap::new()));
        assert_eq!(cache.recompute_count(), 1);
    }

    #[test]
    fn marking_target_cache_dirty_forces_next_replace_to_count() {
        let mut cache = TargetCache::default();
        cache.replace(HashMap::new());
        cache.mark_dirty();
        assert_eq!(cache.snapshot(), None);
        cache.replace(HashMap::new());

        assert_eq!(cache.recompute_count(), 2);
    }
}
