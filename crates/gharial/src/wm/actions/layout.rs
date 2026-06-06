//! `Action::Layout` execution: route a parsed (key, args) tuple to the
//! shared parameter store, then flag the layout cache dirty so the
//! current manage sequence's flush picks up the new value.

use super::super::world::World;

pub(in crate::wm) fn apply_layout(world: &mut World, key: &str, args: &[String]) {
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match world.shared.apply(key, &arg_refs) {
        Ok(applied) if applied.changed => {
            world.mark_layout_dirty();
            // We're already inside a manage sequence; flush_manage right
            // after this drain reads the new params directly. Clear the
            // IPC-thread dirty flag so the ping handler doesn't request
            // a redundant manage round-trip.
            world.shared.take_dirty();
        }
        Ok(_) => {}
        Err(e) => eprintln!("gharial: layout {key}: {e}"),
    }
}
