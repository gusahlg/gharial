//! Spatial-direction neighbour picker. Pure function over `(Id, Rect)`
//! pairs so it can be unit-tested without the wayland `World`.
//!
//! "Focus right" must land on what the user sees immediately to the
//! right, even across screens. Candidates are ranked in three classes;
//! the first non-empty class wins:
//!
//! 1. **Adjacent**: entirely beyond the focused window's edge *and*
//!    sharing some extent on the perpendicular axis (same row for
//!    left/right, same column for up/down). Ranked by the gap along the
//!    movement axis — the nearest column wins — then by most recent
//!    focus, then by overlap, then by position for a stable result.
//! 2. **Beyond, offset**: entirely beyond the edge but with no shared
//!    extent (a screen that sits higher or lower). Ranked by edge
//!    distance.
//! 3. **Overlapping**: only the centre lies in that direction (floating
//!    windows stacked over others). Ranked by centre distance.
//!
//! The old "most aligned centre first" rule let a far window on another
//! screen beat the window right next to the focused one, and resolved
//! ties by `HashMap` order — which is why directional focus felt random.

use crate::action::Direction;
use crate::layout::Rect;

pub fn pick_neighbor<Id: Eq + Clone>(
    rects: &[(Id, Rect)],
    focused_id: &Id,
    focused: Rect,
    dir: Direction,
    recency: impl Fn(&Id) -> u64,
) -> Option<Id> {
    debug_assert!(
        dir.is_spatial(),
        "pick_neighbor only handles cardinal directions"
    );
    let f = Span::of(focused, dir);

    // (class, primary, -recency, -overlap, secondary, tertiary) — smaller wins.
    type Key = (u8, i64, i64, i64, i64, i64);
    let mut best: Option<(Key, Id)> = None;

    for (id, rect) in rects {
        if id == focused_id {
            continue;
        }
        let c = Span::of(*rect, dir);
        let gap = c.near - f.far; // >= 0 when entirely beyond the edge
        let overlap = f.cross_end.min(c.cross_end) - f.cross_start.max(c.cross_start);
        let center_ahead = c.center - f.center;
        let key: Key = if gap >= 0 && overlap > 0 {
            (
                0,
                gap,
                -(recency(id) as i64),
                -overlap,
                c.cross_start,
                c.near,
            )
        } else if gap >= 0 {
            let cross_gap = (c.cross_start - f.cross_end).max(f.cross_start - c.cross_end);
            (1, gap + cross_gap.max(0), 0, 0, c.cross_start, c.near)
        } else if center_ahead > 0 {
            let cross = (c.cross_center - f.cross_center).abs();
            (
                2,
                center_ahead + cross,
                -(recency(id) as i64),
                0,
                c.cross_start,
                c.near,
            )
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|(k, _)| key < *k) {
            best = Some((key, id.clone()));
        }
    }
    best.map(|(_, id)| id)
}

/// A rectangle projected onto the movement axis (oriented so "ahead" is
/// always increasing) and the perpendicular axis.
struct Span {
    /// Edge facing the movement direction's origin.
    near: i64,
    /// Edge facing the movement direction.
    far: i64,
    center: i64,
    cross_start: i64,
    cross_end: i64,
    cross_center: i64,
}

impl Span {
    fn of(r: Rect, dir: Direction) -> Self {
        let (x0, x1) = (r.x as i64, r.x as i64 + r.w as i64);
        let (y0, y1) = (r.y as i64, r.y as i64 + r.h as i64);
        // Flip the axis for left/up so "ahead" is always +.
        let (near, far, cross_start, cross_end) = match dir {
            Direction::Right => (x0, x1, y0, y1),
            Direction::Left => (-x1, -x0, y0, y1),
            Direction::Down => (y0, y1, x0, x1),
            Direction::Up => (-y1, -y0, x0, x1),
            Direction::Next | Direction::Prev => (0, 0, 0, 0),
        };
        Self {
            near,
            far,
            center: (near + far) / 2,
            cross_start,
            cross_end,
            cross_center: (cross_start + cross_end) / 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    fn pick(
        rects: &[(&'static str, Rect)],
        from: &'static str,
        dir: Direction,
    ) -> Option<&'static str> {
        let focused = rects.iter().find(|(id, _)| *id == from).unwrap().1;
        pick_neighbor(rects, &from, focused, dir, |_| 0)
    }

    /// ```text
    ///   +-----+ +-----+
    ///   |  A  | |  B  |
    ///   +-----+ +-----+
    ///   +-----+ +-----+
    ///   |  C  | |  D  |
    ///   +-----+ +-----+
    /// ```
    fn grid() -> Vec<(&'static str, Rect)> {
        vec![
            ("A", r(0, 0, 100, 100)),
            ("B", r(200, 0, 100, 100)),
            ("C", r(0, 200, 100, 100)),
            ("D", r(200, 200, 100, 100)),
        ]
    }

    #[test]
    fn grid_moves_are_the_obvious_ones() {
        let g = grid();
        assert_eq!(pick(&g, "A", Direction::Right), Some("B"));
        assert_eq!(pick(&g, "A", Direction::Down), Some("C"));
        assert_eq!(pick(&g, "D", Direction::Left), Some("C"));
        assert_eq!(pick(&g, "D", Direction::Up), Some("B"));
        assert_eq!(pick(&g, "A", Direction::Left), None);
        assert_eq!(pick(&g, "A", Direction::Up), None);
    }

    /// Master-stack: main left, two stacked on the right.
    fn master_stack() -> Vec<(&'static str, Rect)> {
        vec![
            ("main", r(0, 0, 1000, 1080)),
            ("stack1", r(1000, 0, 920, 540)),
            ("stack2", r(1000, 540, 920, 540)),
        ]
    }

    #[test]
    fn master_stack_is_deterministic_and_prefers_the_top() {
        // Both stack windows are equally aligned with main; the old
        // picker chose by HashMap order. Now: topmost, every time.
        let ms = master_stack();
        for _ in 0..10 {
            assert_eq!(pick(&ms, "main", Direction::Right), Some("stack1"));
        }
        assert_eq!(pick(&ms, "stack2", Direction::Left), Some("main"));
        assert_eq!(pick(&ms, "stack1", Direction::Down), Some("stack2"));
        assert_eq!(pick(&ms, "stack2", Direction::Up), Some("stack1"));
    }

    #[test]
    fn recent_focus_breaks_ties_between_equally_adjacent_windows() {
        let ms = master_stack();
        let main = ms[0].1;
        let recency = |id: &&str| if *id == "stack2" { 5 } else { 1 };
        assert_eq!(
            pick_neighbor(&ms, &"main", main, Direction::Right, recency),
            Some("stack2")
        );
    }

    #[test]
    fn adjacent_window_beats_a_better_aligned_one_on_another_screen() {
        // Screen A (0..1920) master-stack; screen B (1920..3840) one
        // big window whose centre lines up with stack1 better than main
        // does. From stack1, "left" must go to main, not wrap to B, and
        // from main "right" must stay on screen A — on the stack window
        // sharing most of main's height.
        let rects = vec![
            ("main", r(0, 0, 1000, 1080)),
            ("stack1", r(1000, 0, 920, 300)),
            ("stack2", r(1000, 300, 920, 780)),
            ("other", r(1920, 0, 1920, 300)),
        ];
        assert_eq!(pick(&rects, "stack1", Direction::Left), Some("main"));
        assert_eq!(pick(&rects, "main", Direction::Right), Some("stack2"));
        assert_eq!(pick(&rects, "stack1", Direction::Right), Some("other"));
        assert_eq!(pick(&rects, "stack2", Direction::Right), Some("other"));
    }

    #[test]
    fn crosses_to_an_offset_screen_when_nothing_shares_a_row() {
        // Right screen sits lower than the left one: no vertical overlap
        // with the focused window, but it is still to the right.
        let rects = vec![
            ("left", r(0, 0, 1920, 500)),
            ("right-top", r(1920, 600, 1000, 400)),
            ("right-low", r(1920, 1100, 1000, 400)),
        ];
        assert_eq!(pick(&rects, "left", Direction::Right), Some("right-top"));
    }

    #[test]
    fn overlapping_floats_fall_back_to_centres() {
        let rects = vec![
            ("tiled", r(0, 0, 1000, 1000)),
            ("float", r(400, 400, 300, 300)),
        ];
        assert_eq!(pick(&rects, "tiled", Direction::Right), Some("float"));
        assert_eq!(pick(&rects, "float", Direction::Left), Some("tiled"));
        // But a properly adjacent window still wins over an overlap.
        let mut with_neighbor = rects.clone();
        with_neighbor.push(("beside", r(1000, 0, 500, 1000)));
        assert_eq!(
            pick(&with_neighbor, "float", Direction::Right),
            Some("beside")
        );
    }

    #[test]
    fn nearest_column_wins_among_overlapping_candidates() {
        let rects = vec![
            ("f", r(0, 0, 100, 1000)),
            ("near-small", r(110, 900, 100, 50)),
            ("far-big", r(500, 0, 100, 1000)),
        ];
        assert_eq!(pick(&rects, "f", Direction::Right), Some("near-small"));
    }

    #[test]
    fn negative_coordinates_work() {
        let rects = vec![("a", r(-1920, 0, 1920, 1080)), ("b", r(0, 0, 1920, 1080))];
        assert_eq!(pick(&rects, "b", Direction::Left), Some("a"));
        assert_eq!(pick(&rects, "a", Direction::Right), Some("b"));
    }
}
