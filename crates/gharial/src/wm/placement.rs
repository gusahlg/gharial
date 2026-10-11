//! Placement: deciding where a window goes and how it is treated.
//!
//! [`place_new_windows`] runs at the top of every manage sequence for
//! windows river announced since the last one. Defaults come first:
//!
//! - a window with a parent (dialog, file picker) joins its parent's
//!   output and tags, floating and centred;
//! - a window that can only be one size (min == max) floats, centred;
//! - anything else tiles on the focused output's visible tags.
//!
//! Matching rules (and placed `spawn`s) then override any of that, and
//! [`apply`] — the same code `window` commands use — carries it out.
//! Because this all happens before the window's first configure, the
//! client's first frame is already at its final size and place.

use gharial_ipc::{BoolValue, FloatPos, OutputTarget, Placement};
use wayland_client::backend::ObjectId;

use crate::action::Direction;
use crate::layout::Rect;

use super::actions::focus;
use super::procs;
use super::rules::{self, Subject};
use super::spatial::pick_neighbor;
use super::tags::lowest_tag;
use super::world::World;

/// Who asked for a placement — decides how strict to be.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A rule or spawn flag, applied as the window appears. An output
    /// that isn't connected right now is skipped, not an error.
    Rule,
    /// An explicit `window` command; every problem is reported.
    Command,
}

/// Apply initial placement to every window that hasn't had it.
pub fn place_new_windows(world: &mut World) {
    let fresh: Vec<ObjectId> = world
        .windows
        .iter()
        .filter(|w| !w.placed)
        .map(|w| w.id())
        .collect();
    for id in fresh {
        place_initial(world, &id);
    }
}

fn place_initial(world: &mut World, id: &ObjectId) {
    let Some(entry) = world.windows.get(id) else {
        return;
    };
    let parent = entry
        .parent
        .as_ref()
        .and_then(|p| world.windows.get(p))
        .map(|p| (p.output.clone(), p.tags));
    let auto_float = parent.is_some() || entry.hint.is_fixed();
    let (output, tags) = match parent {
        Some((Some(output), tags)) => (Some(output), tags),
        _ => {
            let focused = world.outputs.focused();
            (
                focused.map(|o| o.id()),
                focused.map_or(1, |o| o.active_tags),
            )
        }
    };
    let subject = Subject {
        app_id: entry.app_id.as_deref(),
        title: entry.title.as_deref(),
        pid: entry.pid,
    };
    // Oneshot rules this window already consumed (it is being re-placed
    // after a late app_id/title) still apply, then the live rules.
    let mut ruled: Option<Placement> = None;
    for rule in &entry.consumed_rules {
        if rules::matches(rule, &subject, procs::is_related) {
            ruled
                .get_or_insert_with(Placement::default)
                .merge(&rule.placement);
        }
    }
    let (live, consumed) = world.rules.resolve(&subject, procs::is_related);
    if let Some(live) = live {
        ruled.get_or_insert_with(Placement::default).merge(&live);
    }

    let serial = world.next_serial();
    let entry = world.windows.get_mut(id).expect("checked above");
    entry.output = output;
    entry.tags = tags;
    if auto_float {
        entry.floating = true;
        entry.float_pos = Some(FloatPos::Center);
    }
    entry.raised = serial;
    entry.placed = true;
    entry.consumed_rules.extend(consumed);

    let mut focus_wanted = None;
    if let Some(placement) = ruled {
        match apply(world, id, &placement, Origin::Rule) {
            Ok(focus) => focus_wanted = focus,
            Err(e) => eprintln!("gharial: rule placement: {e}"),
        }
        // A rule that locks the window decides its fullscreen state too: a
        // fullscreen request the client made before it mapped (a game
        // created with fullscreen from its settings) doesn't override it.
        if let Some(entry) = world.windows.get_mut(id) {
            if entry.locked && placement.fullscreen.is_none() && entry.fullscreen {
                entry.fullscreen = false;
                entry.reset_geometry();
            }
        }
    }

    super::tags::set_visibility_targets(world);
    world.mark_layout_dirty();
    world.stacking_dirty = true;

    // Focus a new window if a rule says so, or by default when it opens
    // visibly on the screen the user is working on — never yank focus to
    // another screen or a hidden tag unasked.
    let Some(entry) = world.windows.get(id) else {
        return;
    };
    let default = entry.visible && entry.output == world.outputs.focused_id();
    if focus_wanted.unwrap_or(default) {
        world.push_pending_focus(id.clone());
    }
}

/// Resolve an output token. Directions are relative to `from` (the
/// window's output, or the focused output).
pub fn resolve_output(
    world: &World,
    target: &OutputTarget,
    from: Option<&ObjectId>,
) -> Option<ObjectId> {
    let from = from.cloned().or_else(|| world.outputs.focused_id());
    match target {
        OutputTarget::Named(token) => world.outputs.resolve_named(token),
        OutputTarget::Direction(Direction::Next) => world.outputs.cycle_from(from.as_ref(), true),
        OutputTarget::Direction(Direction::Prev) => world.outputs.cycle_from(from.as_ref(), false),
        OutputTarget::Direction(dir) => {
            let from = from?;
            let rect = world.outputs.get(&from)?.rect();
            let rects: Vec<(ObjectId, Rect)> = world
                .outputs
                .iter()
                .filter(|o| o.has_dimensions())
                .map(|o| (o.id(), o.rect()))
                .collect();
            pick_neighbor(&rects, &from, rect, *dir, |_| 0)
        }
    }
}

fn resolve_bool(current: bool, value: BoolValue) -> bool {
    match value {
        BoolValue::On => true,
        BoolValue::Off => false,
        BoolValue::Toggle => !current,
    }
}

/// Move a window to `output`. Unless `keep_tags`, it adopts the output's
/// visible tags so "send to that screen" means "visible on that screen".
pub fn move_to_output(world: &mut World, id: &ObjectId, output: &ObjectId, keep_tags: bool) {
    let active = world.outputs.get(output).map_or(1, |o| o.active_tags);
    let Some(entry) = world.windows.get_mut(id) else {
        return;
    };
    if entry.output.as_ref() == Some(output) {
        return;
    }
    entry.output = Some(output.clone());
    entry.evacuated = None;
    if !keep_tags {
        entry.tags = active;
    }
    entry.reset_geometry();
    world.mark_layout_dirty();
}

/// Apply `p` to window `id`. Returns the focus request (`focus`/`nofocus`)
/// for the caller to act on — at map time it decides focus-on-map, for a
/// command it is carried out immediately by [`apply_command`].
pub fn apply(
    world: &mut World,
    id: &ObjectId,
    p: &Placement,
    origin: Origin,
) -> Result<Option<bool>, String> {
    let Some(current_output) = world.windows.get(id).map(|w| w.output.clone()) else {
        return Err("window vanished".into());
    };

    if let Some(target) = &p.output {
        match resolve_output(world, target, current_output.as_ref()) {
            Some(output) => move_to_output(world, id, &output, p.tags.is_some()),
            None if origin == Origin::Rule => {
                eprintln!("gharial: rule output {target} is not connected; using the default")
            }
            None => return Err(format!("no output {target}")),
        }
    }

    let entry = world.windows.get_mut(id).ok_or("window vanished")?;
    let mut geometry_changed = false;

    if let Some(tags) = p.tags {
        entry.tags = tags.mask();
    }
    if let Some(v) = p.sticky {
        entry.sticky = resolve_bool(entry.sticky, v);
    }
    if let Some(v) = p.border {
        entry.border = resolve_bool(entry.border, v);
        entry.borders_sent = None;
    }

    let float_implied = p.size.is_some() || p.position.is_some();
    let want_float = match p.floating {
        Some(v) => resolve_bool(entry.floating, v),
        None => entry.floating || float_implied,
    };
    if want_float != entry.floating {
        if want_float {
            // Float in place: keep the size and spot it had while tiled,
            // so toggling doesn't make the window jump.
            if entry.float_size.is_none() {
                entry.float_size = entry
                    .actual
                    .filter(|(w, h)| *w > 0 && *h > 0)
                    .map(|(w, h)| (w as u32, h as u32));
            }
            if entry.float_pos.is_none() {
                entry.float_pos = Some(FloatPos::Center);
            }
        }
        entry.floating = want_float;
        geometry_changed = true;
    }
    if let Some(size) = p.size {
        entry.float_size = Some(size);
        geometry_changed = true;
    }
    if let Some(pos) = p.position {
        entry.float_pos = Some(pos);
        geometry_changed = true;
    }
    if let Some(v) = p.fullscreen {
        let want = resolve_bool(entry.fullscreen, v);
        if want != entry.fullscreen {
            entry.fullscreen = want;
            geometry_changed = true;
        }
    }
    if let Some(v) = p.locked {
        entry.locked = resolve_bool(entry.locked, v);
    }
    if geometry_changed {
        // Explicit geometry beats a lock: forget what was sent so the
        // next manage/render issue it once.
        entry.reset_geometry();
    }

    let focus = p.focus.map(|v| resolve_bool(!entry.no_auto_focus, v));
    if let Some(focus) = focus {
        entry.no_auto_focus = !focus;
    }
    world.mark_layout_dirty();
    world.stacking_dirty = true;
    if geometry_changed {
        world.raise(id);
    }
    Ok(focus)
}

/// `window SELECTOR <placement>`: apply now, then honour `focus` /
/// `nofocus`. Focusing a window on a hidden tag first shows that tag on
/// its output.
pub fn apply_command(world: &mut World, id: &ObjectId, p: &Placement) -> Result<(), String> {
    let focus = apply(world, id, p, Origin::Command)?;
    super::tags::set_visibility_targets(world);
    match focus {
        Some(true) => reveal_and_focus(world, id),
        Some(false) if focus::focused_window(world).as_ref() == Some(id) => {
            focus::refocus_elsewhere(world, id);
        }
        _ => {}
    }
    Ok(())
}

/// Make sure `id` is on a visible tag of its output, then focus it.
pub fn reveal_and_focus(world: &mut World, id: &ObjectId) {
    let Some((output, tags, visible)) = world
        .windows
        .get(id)
        .map(|w| (w.output.clone(), w.tags, w.visible))
    else {
        return;
    };
    if !visible {
        if let Some(o) = output.as_ref().and_then(|o| world.outputs.get_mut(o)) {
            o.active_tags = lowest_tag(tags).max(1);
        }
        super::tags::set_visibility_targets(world);
    }
    focus::focus_window(world, id);
}
