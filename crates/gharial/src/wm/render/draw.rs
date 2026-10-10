//! Render-sequence flush: show/hide, borders, positions, stacking, and
//! output presentation modes. Legal in manage and render sequences; we
//! run it in render sequences, after the window sizes are known.

use std::collections::HashMap;

use gharial_ipc::{FloatPos, Presentation};
use wayland_client::backend::ObjectId;
use wayland_client::Proxy;

use crate::layout::Rect;
use crate::wayland_proto::window_management::river_output_v1::PresentationMode;
use crate::wayland_proto::window_management::river_window_v1::Edges;

use super::super::seats::Seats;
use super::super::windows::WindowEntry;
use super::super::world::World;
use super::{border_of, inset};

/// `set_presentation_mode` arrived in river_output_v1 version 4.
const PRESENTATION_SINCE: u32 = 4;
/// `river_output_v1.presentation_mode` values.
const MODE_VSYNC: u32 = 0;
const MODE_ASYNC: u32 = 1;

pub fn flush(world: &mut World) {
    let areas: HashMap<ObjectId, Rect> = world
        .outputs
        .iter()
        .map(|o| (o.id(), o.tiling_area()))
        .collect();
    let borders: HashMap<ObjectId, u32> = world
        .windows
        .iter()
        .map(|w| (w.id(), border_of(world, w)))
        .collect();
    let focused = focused(&world.seats);
    let colors = (
        world.settings.borders.focused,
        world.settings.borders.unfocused,
    );
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

        if entry.visible && entry.hidden_on_server {
            entry.proxy.show();
            entry.hidden_on_server = false;
        } else if !entry.visible && !entry.hidden_on_server {
            entry.proxy.hide();
            entry.hidden_on_server = true;
        }
        if !entry.visible {
            entry.rendered = entry.actual.is_some() && !entry.awaiting_dims;
            entry.ever_settled |= entry.is_settled();
            continue;
        }

        let border = borders.get(id).copied().unwrap_or(0);
        if !entry.fullscreen {
            let is_focused = focused.as_ref() == Some(id);
            let signature = (border, is_focused);
            if entry.borders_sent != Some(signature) {
                let c = if is_focused { colors.0 } else { colors.1 };
                let edges = if border == 0 {
                    Edges::empty()
                } else {
                    Edges::Top | Edges::Bottom | Edges::Left | Edges::Right
                };
                entry
                    .proxy
                    .set_borders(edges, border as i32, c[0], c[1], c[2], c[3]);
                entry.borders_sent = Some(signature);
            }

            let unlocked = !(entry.locked && entry.position.is_some());
            let pos = if !unlocked {
                None
            } else if entry.is_tiled() {
                targets
                    .get(id)
                    .map(|slot| inset(slot, border).unwrap_or(slot))
                    .map(|c| (c.x, c.y))
            } else {
                entry
                    .output
                    .as_ref()
                    .and_then(|o| areas.get(o))
                    .and_then(|area| float_position(entry, *area, border))
            };
            if let Some(pos) = pos {
                if entry.position != Some(pos) {
                    entry.node.set_position(pos.0, pos.1);
                    entry.position = Some(pos);
                }
            }
        }
        entry.rendered = entry.actual.is_some()
            && !entry.awaiting_dims
            && (entry.fullscreen || entry.position.is_some());
        entry.ever_settled |= entry.is_settled();
    }

    restack(world);
    flush_presentation(world);
}

fn focused(seats: &Seats) -> Option<ObjectId> {
    seats.primary().and_then(|s| s.focused.clone())
}

/// Where a floating window's content goes, in global coordinates.
/// Positions are relative to the output's usable area; negative offsets
/// count from its right/bottom edge (X11 style), `Center` centres it.
/// A centred window is pinned once its real size is known, so a client
/// that later resizes itself doesn't drift.
fn float_position(entry: &mut WindowEntry, area: Rect, border: u32) -> Option<(i32, i32)> {
    let (w, h) = entry
        .actual
        .filter(|(w, h)| *w > 0 && *h > 0)
        .or(entry.float_size.map(|(w, h)| (w as i32, h as i32)))?;
    let b = border as i32;
    let (area_w, area_h) = (area.w as i32, area.h as i32);
    let pos = entry.float_pos.unwrap_or(FloatPos::Center);
    let (rx, ry) = match pos {
        FloatPos::Center => ((area_w - w) / 2, (area_h - h) / 2),
        FloatPos::At(x, y) => {
            let from_far = |off: i32, extent: i32, size: i32| {
                if off < 0 {
                    extent + off - size
                } else {
                    off
                }
            };
            (
                from_far(x, area_w, w + 2 * b) + b,
                from_far(y, area_h, h + 2 * b) + b,
            )
        }
    };
    if matches!(pos, FloatPos::Center) && entry.actual.is_some() {
        entry.float_pos = Some(FloatPos::At(rx - b, ry - b));
    }
    Some((area.x + rx, area.y + ry))
}

/// Re-send the render order when it may have changed: tiled windows at
/// the bottom, floating above them, fullscreen on top; within a layer,
/// most recently raised highest. The protocol leaves initial node order
/// undefined, so order is always set explicitly.
fn restack(world: &mut World) {
    if !world.stacking_dirty {
        return;
    }
    world.stacking_dirty = false;
    let mut nodes: Vec<(u8, u64, &WindowEntry)> = world
        .windows
        .iter()
        .filter(|w| w.placed)
        .map(|w| {
            let layer = if w.fullscreen {
                2
            } else if w.floating {
                1
            } else {
                0
            };
            (layer, w.raised, w)
        })
        .collect();
    nodes.sort_by_key(|(layer, raised, _)| (*layer, *raised));
    for (_, _, w) in nodes {
        w.node.place_top();
    }
}

/// Reconcile each output's presentation mode. `auto` turns tearing on
/// while the output's topmost visible fullscreen window asks for it.
fn flush_presentation(world: &mut World) {
    let mut wants_async: HashMap<ObjectId, (u64, bool)> = HashMap::new();
    for w in world.windows.iter().filter(|w| w.visible && w.fullscreen) {
        if let Some(out) = &w.output {
            let asks = w.presentation_hint == Some(MODE_ASYNC);
            let slot = wants_async.entry(out.clone()).or_insert((w.raised, asks));
            if w.raised >= slot.0 {
                *slot = (w.raised, asks);
            }
        }
    }
    for output in world.outputs.iter_mut() {
        let mode = match output.presentation {
            Presentation::Vsync => MODE_VSYNC,
            Presentation::Async => MODE_ASYNC,
            Presentation::Auto => {
                if wants_async.get(&output.id()).is_some_and(|(_, a)| *a) {
                    MODE_ASYNC
                } else {
                    MODE_VSYNC
                }
            }
        };
        let current = output.presentation_sent.unwrap_or(MODE_VSYNC);
        if mode != current && output.proxy.version() >= PRESENTATION_SINCE {
            output.proxy.set_presentation_mode(if mode == MODE_ASYNC {
                PresentationMode::Async
            } else {
                PresentationMode::Vsync
            });
            output.presentation_sent = Some(mode);
        }
    }
}
