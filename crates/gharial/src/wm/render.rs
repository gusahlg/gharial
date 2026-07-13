//! Translates internal world state into protocol requests.
//!
//! Split into two flush points that mirror the protocol's manage/render
//! sequence:
//!
//!   * [`flush_manage`] runs inside a manage sequence. It reconciles
//!     fullscreen state, sends one-shot `use_ssd` to suppress client-side
//!     decorations, syncs `set_tiled` state, and proposes content
//!     dimensions for visible tiled windows.
//!   * [`flush_render`] runs inside a render sequence. It reconciles
//!     `show`/`hide`, paints focus-aware `set_borders`, and positions
//!     visible tiled windows.
//!
//! Both flush points read their layout target rectangles from
//! [`TargetCache`], which the dispatcher refreshes once per manage/render
//! cycle via [`ensure_targets`]. The cache is borrowed in place during a
//! flush — no per-frame `HashMap` copy — and the per-window walk borrows
//! the insertion order and the entry map disjointly, so neither hot path
//! allocates when nothing about the window set has changed.
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

/// Cache of computed layout target rectangles (outer slot, including
/// border) for each visible tiled window. Recomputed only when something
/// invalidates the layout — output geometry, the visible window set, or a
/// layout-param change — so steady-state manage/render cycles reuse the
/// last result without recomputing or copying it.
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

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Borrow the current targets. Valid only after [`ensure_targets`]
    /// has run for this cycle; callers in the dispatch layer always
    /// refresh first.
    fn targets(&self) -> &HashMap<ObjectId, Rect> {
        &self.targets
    }

    fn store(&mut self, targets: HashMap<ObjectId, Rect>) {
        self.targets = targets;
        self.dirty = false;
        self.recompute_count = self.recompute_count.wrapping_add(1);
    }

    #[cfg(test)]
    fn recompute_count(&self) -> u64 {
        self.recompute_count
    }
}

/// Refresh the layout target cache if it has been marked dirty. Cheap to
/// call unconditionally — a clean cache returns immediately. Run once at
/// the top of each manage/render sequence before flushing.
pub fn ensure_targets(world: &mut World) {
    if !world.target_cache.is_dirty() {
        return;
    }
    let params = world.shared.snapshot();
    let targets = compute_targets(world, &params);
    world.target_cache.store(targets);
}

/// Owned snapshot of the current targets, refreshing the cache first.
/// Used by the infrequent, user-input-driven spatial focus/swap paths
/// that need to mutate `World` while consulting geometry; the per-frame
/// flush paths borrow the cache in place instead.
pub fn targets_snapshot(world: &mut World) -> HashMap<ObjectId, Rect> {
    ensure_targets(world);
    world.target_cache.targets().clone()
}

/// Compute target slot rectangles (outer, including border) for each
/// visible tiled window, output by output — every screen runs its own
/// master-stack over the windows assigned to it. Hidden (off-tag),
/// floating, and fullscreen windows are excluded. Takes the layout
/// `params` so callers can hold the snapshot stable across surrounding
/// work in the same phase.
pub fn compute_targets(world: &World, params: &Params) -> HashMap<ObjectId, Rect> {
    let mut out = HashMap::new();
    for output in world.outputs.iter() {
        if !output.has_dimensions() {
            continue;
        }
        let tiled = world.windows.visible_tiled_ids_on(&output.id());
        if tiled.is_empty() {
            continue;
        }

        // Subtract layer-surface exclusive zones (waybar, panels) from
        // the tiling area when river has reported them. Falls back to
        // the full output rectangle until the first non_exclusive_area
        // event fires.
        let area = output.tiling_area();
        let rects = layout::compute(tiled.len() as u32, (area.w, area.h), params);

        out.reserve(tiled.len());
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

/// Issue manage-bucket requests. Must be called inside a manage sequence,
/// after [`ensure_targets`]. `borders` is the snapshot the dispatcher
/// captured for this phase.
pub fn flush_manage(world: &mut World, borders: &BorderConfig) {
    let border_width = borders.width;
    let World {
        windows,
        target_cache,
        outputs,
        ..
    } = &mut *world;
    let targets = target_cache.targets();
    // A window goes fullscreen on its own output. Proxies cloned once
    // so the loop below can mutate windows without holding an outputs
    // borrow; the first output doubles as the fallback home.
    let output_proxies: HashMap<ObjectId, crate::wayland_proto::RiverOutputV1> =
        outputs.iter().map(|o| (o.id(), o.proxy.clone())).collect();
    let fallback_output = outputs.first().map(|o| o.proxy.clone());
    let (order, by_id) = windows.split_mut();

    for id in order {
        let Some(entry) = by_id.get_mut(id) else {
            continue;
        };

        // One-shot: ask every window to use server-side decorations so
        // they stop drawing their own titlebars/borders.
        if !entry.csd_disabled {
            entry.proxy.use_ssd();
            entry.csd_disabled = true;
        }

        // Reconcile fullscreen state against the server, only on a real
        // edge. Entering fullscreen also raises the window to the top of
        // the render list so it covers the tiled stack underneath it.
        match (entry.fullscreen, entry.fullscreen_on_server) {
            (true, false) => {
                let fullscreen_output = entry
                    .output
                    .as_ref()
                    .and_then(|id| output_proxies.get(id))
                    .or(fallback_output.as_ref());
                if let Some(output) = fullscreen_output {
                    entry.proxy.fullscreen(output);
                    entry.proxy.inform_fullscreen();
                    entry.node.place_top();
                    entry.fullscreen_on_server = true;
                }
            }
            (false, true) => {
                entry.proxy.exit_fullscreen();
                entry.proxy.inform_not_fullscreen();
                entry.fullscreen_on_server = false;
                // Restore normal stacking: floating windows sit on top,
                // tiled windows at the bottom — matching toggle_float.
                if entry.floating {
                    entry.node.place_top();
                } else {
                    entry.node.place_bottom();
                }
            }
            _ => {}
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

        // Dimensions only proposed for visible tiled windows. A
        // fullscreen window is sized by the server to its output; our
        // proposal would be discarded, so skip it.
        if !entry.visible || entry.floating || entry.fullscreen {
            continue;
        }
        let Some(slot) = targets.get(id) else {
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
/// manage sequence, after [`ensure_targets`].
///
/// Order: visibility → borders → position. set_borders fires for
/// every visible tiled window so colour follows focus changes;
/// positioning only fires for tiled windows that have confirmed
/// dimensions. Fullscreen windows are shown/hidden like any other but
/// otherwise left to the server (it suppresses their borders and ignores
/// set_position while fullscreen).
pub fn flush_render(world: &mut World, borders: &BorderConfig) {
    let border_edges = if borders.width == 0 {
        Edges::empty()
    } else {
        all_edges()
    };
    let World {
        windows,
        target_cache,
        seats,
        ..
    } = &mut *world;
    let focused = seats.primary().and_then(|s| s.focused.clone());
    let targets = target_cache.targets();
    let (order, by_id) = windows.split_mut();

    for id in order {
        let Some(entry) = by_id.get_mut(id) else {
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

        // The server owns a fullscreen window's geometry and borders;
        // leave them alone so we don't fight it or churn the wire.
        if entry.fullscreen {
            continue;
        }

        let is_focused = focused.as_ref() == Some(id);
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
        if let Some(slot) = targets.get(id) {
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
        assert!(cache.is_dirty());

        cache.store(HashMap::new());
        assert_eq!(cache.recompute_count(), 1);
        assert!(!cache.is_dirty());
        assert_eq!(cache.targets(), &HashMap::new());
        // A clean cache is not recomputed again.
        assert_eq!(cache.recompute_count(), 1);
    }

    #[test]
    fn marking_target_cache_dirty_forces_next_store_to_count() {
        let mut cache = TargetCache::default();
        cache.store(HashMap::new());
        cache.mark_dirty();
        assert!(cache.is_dirty());
        cache.store(HashMap::new());

        assert_eq!(cache.recompute_count(), 2);
    }
}
