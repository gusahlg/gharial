//! Tag bitmask tracking. Tags 1..32 map to bits 0..31.
//!
//! v0.2 keeps a single global active mask shared across all outputs.
//! Per-output tag sets are an obvious v0.3 extension; the data model
//! here doesn't preclude it.

use wayland_client::backend::ObjectId;

use super::world::World;

const TAG_COUNT: usize = 32;

#[derive(Debug)]
pub struct Tags {
    /// Bitmask of currently-visible tags.
    pub active: u32,
    focus: TagFocus<ObjectId>,
}

impl Default for Tags {
    fn default() -> Self {
        Self {
            active: 1,
            focus: TagFocus::default(),
        }
    }
}

impl Tags {
    /// Remember `window_id` as the focused window for every currently
    /// active tag it belongs to.
    pub fn remember_focus(&mut self, window_id: &ObjectId, window_tags: u32) {
        self.focus.remember(self.active, window_tags, window_id);
    }

    /// Remove a destroyed window from per-tag focus history.
    pub fn forget_window(&mut self, window_id: &ObjectId) {
        self.focus.forget(window_id);
    }

    /// Focus candidates remembered for the currently active tags.
    pub fn focus_candidates(&self) -> Vec<ObjectId> {
        self.focus.candidates(self.active)
    }
}

/// `1 << (n - 1)` with `n` already validated to 1..=32 by the caller.
pub fn tag_mask(n: u8) -> u32 {
    1u32 << (n - 1)
}

/// Recompute every window's `visible` flag from its tag mask vs the
/// active mask. Called after any tag-mutation action.
pub fn set_visibility_targets(world: &mut World) {
    let active = world.tags.active;
    for id in world.windows.ordered_ids() {
        if let Some(entry) = world.windows.get_mut(&id) {
            entry.visible = (entry.tags & active) != 0;
        }
    }
}

#[derive(Debug)]
struct TagFocus<Id> {
    by_tag: Vec<Option<Id>>,
}

impl<Id> Default for TagFocus<Id> {
    fn default() -> Self {
        Self {
            by_tag: std::iter::repeat_with(|| None).take(TAG_COUNT).collect(),
        }
    }
}

impl<Id> TagFocus<Id>
where
    Id: Clone + Eq,
{
    fn remember(&mut self, active: u32, window_tags: u32, id: &Id) {
        for idx in tag_indexes(active & window_tags) {
            self.by_tag[idx] = Some(id.clone());
        }
    }

    fn forget(&mut self, id: &Id) {
        for slot in &mut self.by_tag {
            if slot.as_ref() == Some(id) {
                *slot = None;
            }
        }
    }

    fn candidates(&self, active: u32) -> Vec<Id> {
        let mut out = Vec::new();
        for idx in tag_indexes(active) {
            if let Some(id) = &self.by_tag[idx] {
                if !out.iter().any(|seen| seen == id) {
                    out.push(id.clone());
                }
            }
        }
        out
    }
}

fn tag_indexes(mask: u32) -> impl Iterator<Item = usize> {
    (0..TAG_COUNT).filter(move |idx| (mask & (1u32 << idx)) != 0)
}

pub fn pick_focus_candidate<Id, F>(
    remembered: impl IntoIterator<Item = Id>,
    ordered: impl IntoIterator<Item = Id>,
    is_visible: F,
) -> Option<Id>
where
    F: Fn(&Id) -> bool,
{
    remembered
        .into_iter()
        .find(|id| is_visible(id))
        .or_else(|| ordered.into_iter().find(|id| is_visible(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_mask_is_one_indexed() {
        assert_eq!(tag_mask(1), 0x0000_0001);
        assert_eq!(tag_mask(2), 0x0000_0002);
        assert_eq!(tag_mask(9), 0x0000_0100);
        assert_eq!(tag_mask(32), 0x8000_0000);
    }

    #[test]
    fn default_tags_show_tag_one() {
        assert_eq!(Tags::default().active, 0x1);
    }

    #[test]
    fn tag_focus_remembers_active_member_tags_only() {
        let mut focus = TagFocus::default();
        focus.remember(tag_mask(1), tag_mask(1) | tag_mask(2), &"a");
        focus.remember(tag_mask(2), tag_mask(1) | tag_mask(2), &"b");

        assert_eq!(focus.candidates(tag_mask(1)), vec!["a"]);
        assert_eq!(focus.candidates(tag_mask(2)), vec!["b"]);
    }

    #[test]
    fn tag_focus_dedupes_multi_tag_candidates() {
        let mut focus = TagFocus::default();
        let active = tag_mask(1) | tag_mask(2);
        focus.remember(active, active, &"a");

        assert_eq!(focus.candidates(active), vec!["a"]);
    }

    #[test]
    fn tag_focus_forgets_destroyed_windows() {
        let mut focus = TagFocus::default();
        focus.remember(tag_mask(1), tag_mask(1), &"a");
        focus.remember(tag_mask(2), tag_mask(2), &"a");
        focus.forget(&"a");

        assert!(focus.candidates(tag_mask(1) | tag_mask(2)).is_empty());
    }

    #[test]
    fn remembered_focus_wins_over_first_visible_default() {
        let remembered = vec!["b"];
        let ordered = vec!["a", "b", "c"];
        let visible = |id: &&str| matches!(*id, "a" | "b");

        assert_eq!(
            pick_focus_candidate(remembered, ordered, visible),
            Some("b")
        );
    }

    #[test]
    fn hidden_remembered_focus_falls_back_to_first_visible() {
        let remembered = vec!["b"];
        let ordered = vec!["a", "b", "c"];
        let visible = |id: &&str| *id == "a";

        assert_eq!(
            pick_focus_candidate(remembered, ordered, visible),
            Some("a")
        );
    }
}
