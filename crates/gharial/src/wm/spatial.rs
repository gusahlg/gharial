//! Spatial-direction neighbour picker. Pure function over `(Id, Rect)`
//! pairs so it can be unit-tested without dragging in the wayland
//! `World` or any layout assumption.
//!
//! Algorithm for picking the neighbour in a cardinal direction:
//!   * Discard the focused window and everything *not* on the correct
//!     side of it (centre comparison).
//!   * Sort remaining candidates by `(perpendicular_distance,
//!     axial_distance)`. The most aligned wins first; ties broken by
//!     proximity along the movement axis.
//!
//! This matches the "feels right" behaviour of sway/dwm/i3 — pressing
//! `focus down` on a window with two windows below picks the one
//! directly underneath, not the one off to the side.

use crate::action::Direction;
use crate::layout::Rect;

pub fn pick_neighbor<Id: Eq + Clone>(
    rects: &[(Id, Rect)],
    focused_id: &Id,
    focused: Rect,
    dir: Direction,
) -> Option<Id> {
    debug_assert!(dir.is_spatial(), "pick_neighbor only handles cardinal directions");

    let fc = center(focused);
    let mut best: Option<(i64, i64, Id)> = None;

    for (id, rect) in rects {
        if id == focused_id {
            continue;
        }
        let cc = center(*rect);
        let dx = cc.0 - fc.0;
        let dy = cc.1 - fc.1;

        let on_correct_side = match dir {
            Direction::Left => dx < 0,
            Direction::Right => dx > 0,
            Direction::Up => dy < 0,
            Direction::Down => dy > 0,
            _ => return None,
        };
        if !on_correct_side {
            continue;
        }

        // perpendicular distance dominates (alignment), then axial distance.
        let (perp, axial) = match dir {
            Direction::Left | Direction::Right => (dy.unsigned_abs() as i64, dx.unsigned_abs() as i64),
            Direction::Up | Direction::Down => (dx.unsigned_abs() as i64, dy.unsigned_abs() as i64),
            _ => unreachable!(),
        };

        let candidate = (perp, axial, id.clone());
        match &best {
            None => best = Some(candidate),
            Some((p, a, _)) if (perp, axial) < (*p, *a) => best = Some(candidate),
            _ => {}
        }
    }
    best.map(|(_, _, id)| id)
}

fn center(r: Rect) -> (i32, i32) {
    (r.x + r.w as i32 / 2, r.y + r.h as i32 / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Layout:
    ///   +-----+ +-----+
    ///   |  A  | |  B  |
    ///   +-----+ +-----+
    ///   +-----+ +-----+
    ///   |  C  | |  D  |
    ///   +-----+ +-----+
    fn grid() -> Vec<(&'static str, Rect)> {
        vec![
            ("A", r(0, 0, 100, 100)),
            ("B", r(200, 0, 100, 100)),
            ("C", r(0, 200, 100, 100)),
            ("D", r(200, 200, 100, 100)),
        ]
    }

    #[test]
    fn right_picks_the_horizontally_adjacent_window() {
        let g = grid();
        let next = pick_neighbor(&g, &"A", r(0, 0, 100, 100), Direction::Right);
        assert_eq!(next, Some("B"));
    }

    #[test]
    fn down_picks_the_vertically_adjacent_window() {
        let g = grid();
        let next = pick_neighbor(&g, &"A", r(0, 0, 100, 100), Direction::Down);
        assert_eq!(next, Some("C"));
    }

    #[test]
    fn no_neighbor_in_that_direction_returns_none() {
        let g = grid();
        let next = pick_neighbor(&g, &"A", r(0, 0, 100, 100), Direction::Left);
        assert_eq!(next, None);
        let next = pick_neighbor(&g, &"A", r(0, 0, 100, 100), Direction::Up);
        assert_eq!(next, None);
    }

    #[test]
    fn perpendicular_alignment_dominates_axial_distance() {
        // F focused. Candidates X is slightly to the right but far down;
        // Y is far right but perfectly aligned. "Right" picks Y.
        let rects = vec![
            ("F", r(0, 0, 100, 100)),
            ("X", r(50, 800, 100, 100)),   // off-axis but close-ish in x
            ("Y", r(500, 0, 100, 100)),    // far in x, aligned in y
        ];
        let next = pick_neighbor(&rects, &"F", r(0, 0, 100, 100), Direction::Right);
        assert_eq!(next, Some("Y"));
    }

    #[test]
    fn ties_in_perpendicular_break_by_axial_distance() {
        // Two windows perfectly aligned vertically; the closer one wins.
        let rects = vec![
            ("F", r(0, 0, 100, 100)),
            ("Near", r(150, 0, 100, 100)),
            ("Far",  r(500, 0, 100, 100)),
        ];
        let next = pick_neighbor(&rects, &"F", r(0, 0, 100, 100), Direction::Right);
        assert_eq!(next, Some("Near"));
    }

    /// Master-stack with one main + two stack windows. From main,
    /// pressing right should go to the top of the stack (most aligned).
    #[test]
    fn master_stack_right_picks_top_of_stack() {
        let rects = vec![
            ("main",  r(0, 0, 1000, 1080)),
            ("stack1", r(1000, 0, 920, 540)),
            ("stack2", r(1000, 540, 920, 540)),
        ];
        let next = pick_neighbor(&rects, &"main", r(0, 0, 1000, 1080), Direction::Right);
        // Both stack windows have the same horizontal distance from main,
        // but stack1's centre y (270) is closer to main's centre y (540)
        // than stack2's (810). So stack1 wins.
        assert_eq!(next, Some("stack1"));
    }

    /// Within the stack column, j/k cycles between adjacent stack windows.
    #[test]
    fn within_stack_down_picks_next_in_column() {
        let rects = vec![
            ("stack1", r(1000, 0, 920, 540)),
            ("stack2", r(1000, 540, 920, 540)),
        ];
        let next = pick_neighbor(&rects, &"stack1", r(1000, 0, 920, 540), Direction::Down);
        assert_eq!(next, Some("stack2"));
        let prev = pick_neighbor(&rects, &"stack2", r(1000, 540, 920, 540), Direction::Up);
        assert_eq!(prev, Some("stack1"));
    }
}
