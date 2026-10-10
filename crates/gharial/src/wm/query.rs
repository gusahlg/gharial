//! `window list` and `output list`: snapshot world state into the shared
//! [`gharial_ipc::info`] records.

use gharial_ipc::info::{render_outputs, render_windows, Geometry};
use gharial_ipc::{ListFormat, OutputInfo, TagSet, WindowInfo, WindowSelector};

use crate::layout::Rect;

use super::actions::focus::focused_window;
use super::actions::window::select;
use super::windows::WindowEntry;
use super::world::World;

fn geometry(r: Rect) -> Geometry {
    Geometry {
        x: r.x,
        y: r.y,
        width: r.w,
        height: r.h,
    }
}

pub fn window_info(world: &World, w: &WindowEntry, focused: bool) -> WindowInfo {
    let output = w.output.as_ref();
    let rect = if w.fullscreen && w.fullscreen_on.is_some() {
        output.and_then(|o| world.outputs.get(o)).map(|o| o.rect())
    } else {
        w.content_rect()
    };
    WindowInfo {
        id: w.uid,
        app_id: w.app_id.clone(),
        title: w.title.clone(),
        pid: w.pid,
        output: output.and_then(|o| world.outputs.name_of(o)),
        tags: TagSet::from_mask(w.tags).unwrap_or(TagSet::ALL),
        visible: w.visible,
        floating: w.floating,
        fullscreen: w.fullscreen,
        sticky: w.sticky,
        locked: w.locked,
        focused,
        geometry: rect.filter(|_| w.actual.is_some()).map(geometry),
    }
}

pub fn windows(world: &World, selector: Option<&WindowSelector>, format: ListFormat) -> String {
    let focused = focused_window(world);
    let ids = match selector {
        Some(sel) => select(world, sel),
        None => world.windows.ids(),
    };
    let infos: Vec<WindowInfo> = ids
        .iter()
        .filter_map(|id| world.windows.get(id))
        .filter(|w| w.placed)
        .map(|w| window_info(world, w, focused.as_ref() == Some(&w.id())))
        .collect();
    render_windows(&infos, format)
}

pub fn outputs(world: &World, format: ListFormat) -> String {
    let focused = world.outputs.focused_id();
    let infos: Vec<OutputInfo> = world
        .outputs
        .iter()
        .map(|o| OutputInfo {
            name: world.outputs.name_of(&o.id()).unwrap_or_default(),
            geometry: geometry(o.rect()),
            tags: TagSet::from_mask(o.active_tags).unwrap_or(TagSet::ALL),
            focused: focused.as_ref() == Some(&o.id()),
            presentation: o.presentation,
            usable: geometry(o.tiling_area()),
        })
        .collect();
    render_outputs(&infos, format)
}
