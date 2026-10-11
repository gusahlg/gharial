//! `river_window_v1` event handler — records what the window tells us.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::wayland_proto::window_management::river_window_v1 as iface;
use crate::wayland_proto::RiverWindowV1;

use super::super::actions::focus::forget_window;
use super::super::placement;
use super::super::windows::SizeHint;
use super::super::world::World;

impl Dispatch<RiverWindowV1, ()> for World {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowV1,
        event: iface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            iface::Event::Closed => {
                // Destroy must follow `closed`, per protocol.
                if let Some(entry) = state.windows.remove(&id) {
                    forget_window(state, &id);
                    state.pending_focus.retain(|p| *p != id);
                    state.mark_layout_dirty();
                    entry.node.destroy();
                    entry.proxy.destroy();
                }
                return;
            }
            iface::Event::Dimensions { width, height } => {
                if let Some(entry) = state.windows.get_mut(&id) {
                    entry.actual = Some((width, height));
                    entry.awaiting_dims = false;
                }
                return;
            }
            // A client asked for fullscreen, optionally on a given output;
            // the next manage sequence reconciles with the server.
            iface::Event::FullscreenRequested { output } => {
                let Some(entry) = state.windows.get_mut(&id) else {
                    return;
                };
                // A locked window's geometry is policy's alone: a game
                // that asks for fullscreen (from its own settings, say)
                // must not undo a rule's fixed size.
                if entry.locked {
                    return;
                }
                entry.fullscreen = true;
                entry.reset_geometry();
                // Moving to the hinted output adopts its visible tags, so
                // the window can't land hidden on a tag nobody views there.
                let hint = output.map(|o| o.id());
                if let Some(output) = hint {
                    if state.outputs.get(&output).is_some() {
                        placement::move_to_output(state, &id, &output, false);
                    }
                }
                state.mark_layout_dirty();
                state.stacking_dirty = true;
                return;
            }
            iface::Event::PresentationHint { hint } => {
                if let Some(entry) = state.windows.get_mut(&id) {
                    entry.presentation_hint = Some(hint.into());
                }
                return;
            }
            _ => {}
        }

        let Some(entry) = state.windows.get_mut(&id) else {
            return;
        };
        match event {
            iface::Event::DimensionsHint {
                min_width,
                min_height,
                max_width,
                max_height,
            } => {
                entry.hint = SizeHint {
                    min: (min_width.max(0), min_height.max(0)),
                    max: (max_width.max(0), max_height.max(0)),
                };
            }
            iface::Event::AppId { app_id } => {
                let changed = entry.app_id != app_id;
                entry.app_id = app_id;
                if changed {
                    maybe_replace(state, &id);
                }
            }
            iface::Event::Title { title } => {
                let changed = entry.title != title;
                entry.title = title;
                if changed {
                    maybe_replace(state, &id);
                }
            }
            iface::Event::Parent { parent } => {
                entry.parent = parent.map(|p| p.id());
            }
            iface::Event::UnreliablePid { unreliable_pid } => {
                entry.pid = u32::try_from(unreliable_pid).ok().filter(|p| *p > 0);
            }
            // Likewise a locked fullscreen window stays fullscreen.
            iface::Event::ExitFullscreenRequested if entry.locked => {}
            iface::Event::ExitFullscreenRequested => {
                entry.fullscreen = false;
                entry.reset_geometry();
                state.mark_layout_dirty();
                state.stacking_dirty = true;
            }
            _ => {}
        }
    }
}

/// A window that changes its app_id/title before it was ever sized gets
/// its placement redone, so rules can match clients that name themselves
/// late. Once it is on screen, identity changes no longer move it.
fn maybe_replace(state: &mut World, id: &wayland_client::backend::ObjectId) {
    let no_rules = state.rules.is_empty();
    let Some(entry) = state.windows.get_mut(id) else {
        return;
    };
    if !entry.placed || entry.actual.is_some() || (no_rules && entry.consumed_rules.is_empty()) {
        return;
    }
    state.pending_focus.retain(|p| p != id);
    entry.placed = false;
    entry.no_auto_focus = false;
    entry.floating = false;
    entry.sticky = false;
    entry.locked = false;
    entry.border = true;
    entry.float_size = None;
    entry.float_pos = None;
}
