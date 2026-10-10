//! Translates world state into protocol requests.
//!
//! Split into two flush points that mirror the protocol's manage/render
//! sequences:
//!
//!   * [`manage::flush`] (manage sequence) reconciles fullscreen,
//!     decorations, tiled edges, dimension bounds, and proposes sizes.
//!   * [`draw::flush`] (render sequence) reconciles show/hide, borders,
//!     positions, stacking order, and output presentation modes.
//!
//! Both read layout slots from [`TargetCache`], refreshed once per cycle
//! by [`ensure_targets`] and only when something invalidated it.
//!
//! Border invariant: a layout slot encloses the *whole* window including
//! its border; the content rect is the slot inset by the border width,
//! so neighbouring tiles never share or overlap border pixels.

pub mod draw;
pub mod manage;

use std::collections::HashMap;

use wayland_client::backend::ObjectId;

use crate::layout::{self, Rect};

use super::world::World;

/// Layout slots (outer rect, including border) for every visible tiled
/// window, recomputed only when invalidated — output geometry, the
/// visible window set, or a layout setting changed.
#[derive(Debug)]
pub struct TargetCache {
    targets: HashMap<ObjectId, Rect>,
    dirty: bool,
}

impl Default for TargetCache {
    fn default() -> Self {
        Self {
            targets: HashMap::new(),
            dirty: true,
        }
    }
}

impl TargetCache {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn get(&self, id: &ObjectId) -> Option<Rect> {
        self.targets.get(id).copied()
    }
}

/// Refresh the layout target cache if it has been marked dirty. Cheap to
/// call unconditionally.
pub fn ensure_targets(world: &mut World) {
    if !world.targets.dirty {
        return;
    }
    world.targets.targets = compute_targets(world);
    world.targets.dirty = false;
}

/// Master-stack slots per output over that output's visible tiled
/// windows, inside its usable area (panels' exclusive zones removed).
fn compute_targets(world: &World) -> HashMap<ObjectId, Rect> {
    let params = &world.settings.params;
    let mut out = HashMap::new();
    for output in world.outputs.iter().filter(|o| o.has_dimensions()) {
        let tiled = world.windows.tiled_on(&output.id());
        if tiled.is_empty() {
            continue;
        }
        let area = output.tiling_area();
        let rects = layout::compute(tiled.len() as u32, (area.w, area.h), params);
        for (id, rect) in tiled.into_iter().zip(rects) {
            out.insert(
                id,
                Rect {
                    x: rect.x + area.x,
                    y: rect.y + area.y,
                    ..rect
                },
            );
        }
    }
    out
}

/// Inset a slot by `border` on every side; `None` if nothing is left.
pub fn inset(slot: Rect, border: u32) -> Option<Rect> {
    let b = border as i32;
    let w = slot.w as i32 - 2 * b;
    let h = slot.h as i32 - 2 * b;
    (w > 0 && h > 0).then(|| Rect {
        x: slot.x + b,
        y: slot.y + b,
        w: w as u32,
        h: h as u32,
    })
}

/// Border width a window is drawn with.
pub fn border_of(world: &World, w: &super::windows::WindowEntry) -> u32 {
    if w.border && !w.fullscreen {
        world.settings.borders.width
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_shrinks_on_every_side() {
        let slot = Rect {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
        };
        assert_eq!(
            inset(slot, 3),
            Some(Rect {
                x: 13,
                y: 23,
                w: 94,
                h: 44
            })
        );
        assert_eq!(inset(slot, 0), Some(slot));
        assert_eq!(inset(slot, 25), None);
    }

    #[test]
    fn target_cache_starts_dirty() {
        let cache = TargetCache::default();
        assert!(cache.dirty);
    }
}
