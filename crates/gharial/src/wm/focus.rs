//! Focus policy that is independent from Wayland protocol proxies.
//!
//! The compositor-facing focused window still lives on `SeatEntry`.
//! This module owns the WM policy around remembered focus, so tag
//! switching and fallback selection can be tested without a live river
//! connection.

const TAG_COUNT: usize = 32;

/// Per-tag "last focused window" memory. Backed by a stack-resident
/// fixed-size array: the tag count is a compile-time constant, so a
/// `[Option<Id>; 32]` is both cheaper than a heap `Vec` and a tighter
/// contract — no out-of-bounds at runtime, no allocation on construction.
#[derive(Debug)]
pub struct FocusMemory<Id> {
    by_tag: [Option<Id>; TAG_COUNT],
}

impl<Id> Default for FocusMemory<Id> {
    fn default() -> Self {
        // `[None; 32]` requires Id: Copy. The const-array helper avoids
        // that requirement; we just need Id: Sized.
        Self {
            by_tag: std::array::from_fn(|_| None),
        }
    }
}

impl<Id> FocusMemory<Id>
where
    Id: Clone + Eq,
{
    /// Remember `id` as the focused window for every currently active
    /// tag that the window actually belongs to.
    pub fn remember(&mut self, active_tags: u32, window_tags: u32, id: &Id) {
        for idx in tag_indexes(active_tags & window_tags) {
            self.by_tag[idx] = Some(id.clone());
        }
    }

    /// Drop all references to a removed window.
    pub fn forget(&mut self, id: &Id) {
        for slot in &mut self.by_tag {
            if slot.as_ref() == Some(id) {
                *slot = None;
            }
        }
    }

    /// Focus candidates remembered for the active tags, de-duplicated
    /// in tag-number order.
    pub fn candidates(&self, active_tags: u32) -> Vec<Id> {
        let mut out = Vec::new();
        for idx in tag_indexes(active_tags) {
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

pub fn pick_candidate<Id, F>(
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
    use crate::wm::tags::tag_mask;

    #[test]
    fn remembers_active_member_tags_only() {
        let mut focus = FocusMemory::default();
        focus.remember(tag_mask(1), tag_mask(1) | tag_mask(2), &"a");
        focus.remember(tag_mask(2), tag_mask(1) | tag_mask(2), &"b");

        assert_eq!(focus.candidates(tag_mask(1)), vec!["a"]);
        assert_eq!(focus.candidates(tag_mask(2)), vec!["b"]);
    }

    #[test]
    fn ignores_windows_without_active_tag_membership() {
        let mut focus = FocusMemory::default();
        focus.remember(tag_mask(1), tag_mask(2), &"a");

        assert!(focus.candidates(tag_mask(1)).is_empty());
        assert!(focus.candidates(tag_mask(2)).is_empty());
    }

    #[test]
    fn dedupes_multi_tag_candidates() {
        let mut focus = FocusMemory::default();
        let active = tag_mask(1) | tag_mask(2);
        focus.remember(active, active, &"a");

        assert_eq!(focus.candidates(active), vec!["a"]);
    }

    #[test]
    fn forgets_removed_windows() {
        let mut focus = FocusMemory::default();
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

        assert_eq!(pick_candidate(remembered, ordered, visible), Some("b"));
    }

    #[test]
    fn hidden_remembered_focus_falls_back_to_first_visible() {
        let remembered = vec!["b"];
        let ordered = vec!["a", "b", "c"];
        let visible = |id: &&str| *id == "a";

        assert_eq!(pick_candidate(remembered, ordered, visible), Some("a"));
    }

    #[test]
    fn candidate_tries_all_remembered_focus_before_stack_order() {
        let remembered = vec!["hidden", "remembered"];
        let ordered = vec!["default", "remembered"];
        let visible = |id: &&str| *id != "hidden";

        assert_eq!(
            pick_candidate(remembered, ordered, visible),
            Some("remembered")
        );
    }

    #[test]
    fn candidate_returns_none_when_nothing_visible() {
        let remembered = vec!["hidden"];
        let ordered = vec!["also-hidden"];
        let visible = |_id: &&str| false;

        assert_eq!(pick_candidate(remembered, ordered, visible), None);
    }

    #[test]
    fn focus_memory_covers_all_32_tags_independently() {
        // Per-tag isolation: writes on tag N must not bleed into tag M.
        let mut focus = FocusMemory::default();
        for n in 1..=32u8 {
            focus.remember(tag_mask(n), tag_mask(n), &format!("win-{n}"));
        }
        for n in 1..=32u8 {
            assert_eq!(focus.candidates(tag_mask(n)), vec![format!("win-{n}")]);
        }
    }

    #[test]
    fn focus_memory_default_has_no_remembered_focus() {
        // A brand-new memory must produce no candidates regardless of
        // the active tag set — guards against accidental garbage in the
        // fixed-array default initializer.
        let focus: FocusMemory<&str> = FocusMemory::default();
        for n in 1..=32u8 {
            assert!(focus.candidates(tag_mask(n)).is_empty());
        }
        assert!(focus.candidates(u32::MAX).is_empty());
    }
}
