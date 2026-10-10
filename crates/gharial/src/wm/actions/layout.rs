//! `Action::Layout`: route a (key, args) pair to the settings store and
//! invalidate whatever depends on it.

use super::super::world::World;
use super::Outcome;

pub fn apply_layout(world: &mut World, key: &str, args: &[String]) -> Outcome {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let applied = world.settings.apply(key, &args)?;
    if applied.changed {
        world.mark_layout_dirty();
        if key.starts_with("border-") {
            // Colours/width are part of every window's border; repaint.
            for w in world.windows.iter_mut() {
                w.borders_sent = None;
            }
            // The content rect is the slot inset by the border width.
            for w in world.windows.iter_mut() {
                if w.is_tiled() {
                    w.proposed = None;
                }
            }
        }
    }
    Ok(applied.summary)
}
