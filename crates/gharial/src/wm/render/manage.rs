//! Manage-sequence flush: requests that change window-management state.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::layout::Rect;
use crate::wayland_proto::window_management::river_window_v1::Edges;
use crate::wayland_proto::RiverOutputV1;

use super::super::world::World;
use super::{border_of, inset};

/// `set_dimension_bounds` arrived in river_window_v1 version 4.
const DIMENSION_BOUNDS_SINCE: u32 = 4;

fn all_edges() -> Edges {
    Edges::Top | Edges::Bottom | Edges::Left | Edges::Right
}

/// Issue manage-bucket requests for every placed window. Must run inside
/// a manage sequence, after `ensure_targets`.
pub fn flush(world: &mut World) {
    let outputs: HashMap<ObjectId, (RiverOutputV1, Rect)> = world
        .outputs
        .iter()
        .map(|o| (o.id(), (o.proxy.clone(), o.tiling_area())))
        .collect();
    let borders: HashMap<ObjectId, u32> = world
        .windows
        .iter()
        .map(|w| (w.id(), border_of(world, w)))
        .collect();
    let mut restack = false;
    let World {
        windows, targets, ..
    } = &mut *world;
    let (order, by_id) = windows.split_mut();

    for id in order {
        let Some(entry) = by_id.get_mut(id) else {
            continue;
        };
        if !entry.placed {
            continue;
        }
        let border = borders.get(id).copied().unwrap_or(0);
        let home = entry.output.as_ref().and_then(|o| outputs.get(o));

        // One-shot: ask for server-side decorations so clients drop
        // their own titlebars where they can.
        if !entry.ssd_sent {
            entry.proxy.use_ssd();
            entry.ssd_sent = true;
        }

        // Fullscreen, on the window's own output; re-issued if the
        // window moved to another output while fullscreen.
        let want_fs = if entry.fullscreen {
            entry.output.clone().filter(|o| outputs.contains_key(o))
        } else {
            None
        };
        if want_fs != entry.fullscreen_on {
            match (&want_fs, &entry.fullscreen_on) {
                (Some(out), was) => {
                    entry.proxy.fullscreen(&outputs[out].0);
                    if was.is_none() {
                        entry.proxy.inform_fullscreen();
                    }
                }
                (None, Some(_)) => {
                    entry.proxy.exit_fullscreen();
                    entry.proxy.inform_not_fullscreen();
                    // Size and position are undefined until we propose
                    // them again — in this same sequence, for frame
                    // perfection.
                    entry.reset_geometry();
                }
                (None, None) => {}
            }
            entry.fullscreen_on = want_fs;
            entry.awaiting_dims = true;
            entry.rendered = false;
            restack = true;
        }

        // Tiled windows drop shadows/rounded corners; floating restore them.
        let want_tiled = entry.is_tiled();
        if entry.tiled_sent != Some(want_tiled) {
            entry.proxy.set_tiled(if want_tiled {
                all_edges()
            } else {
                Edges::empty()
            });
            entry.tiled_sent = Some(want_tiled);
        }

        // Tell clients how much room their output has, so a window that
        // picks its own size doesn't open larger than the screen. Each
        // update reaches the client as a configure, so a locked or
        // fullscreen window gets it at most once.
        let bounds_frozen = (entry.locked || entry.fullscreen) && entry.bounds_sent.is_some();
        if let Some((_, area)) = home.filter(|_| !bounds_frozen) {
            let bounds = (area.w as i32, area.h as i32);
            if entry.bounds_sent != Some(bounds) && entry.proxy.version() >= DIMENSION_BOUNDS_SINCE
            {
                entry.proxy.set_dimension_bounds(bounds.0, bounds.1);
                entry.bounds_sent = Some(bounds);
            }
        }

        if entry.fullscreen {
            continue; // the server sizes fullscreen windows
        }
        if entry.locked && entry.proposed.is_some() {
            continue;
        }
        let want = if entry.floating {
            match (entry.float_size, entry.proposed) {
                (Some((w, h)), _) => Some((w as i32, h as i32)),
                // First proposal: let the client choose its size.
                (None, None) => Some((0, 0)),
                (None, Some(_)) => None,
            }
        } else if let Some(slot) = targets.get(id).filter(|_| entry.visible) {
            inset(slot, border).map(|c| (c.w as i32, c.h as i32))
        } else if entry.proposed.is_none() {
            // Hidden tiled window that was never sized (opened onto a
            // tag nobody is viewing): give it its output's usable area
            // so the client can start; it gets its real slot when shown.
            home.and_then(|(_, area)| inset(*area, border))
                .map(|c| (c.w as i32, c.h as i32))
        } else {
            None
        };
        if let Some(want) = want {
            if entry.proposed != Some(want) {
                entry.proxy.propose_dimensions(want.0.max(0), want.1.max(0));
                entry.proposed = Some(want);
                entry.awaiting_dims = true;
                entry.rendered = false;
            }
        }
    }
    if restack {
        world.stacking_dirty = true;
    }
}
