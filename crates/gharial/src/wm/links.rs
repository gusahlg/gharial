//! Pointer edge links — "portals" between output edges.
//!
//! The user links two output edges (`output link DP-1:right DP-2:left`)
//! and the pointer warps through whenever it hits either side. Links
//! only fire where the compositor has clamped the pointer: if the point
//! just beyond the edge lies inside another output, the pointer crosses
//! naturally and the link stays out of the way. That makes links purely
//! additive — adjacent screens keep their seamless boundary, and links
//! add paths the layout doesn't provide (wrap-around, non-adjacent
//! screens, mismatched physical arrangements).
//!
//! The decision function ([`warp_destination`]) is pure over rectangles
//! so the geometry is testable without a compositor.

use crate::edge::{Edge, EdgeRef};
use crate::layout::Rect;

/// How close (px) the pointer must be to a linked edge to warp through.
pub const EDGE_TRIGGER: i32 = 1;

/// How far (px) inside the destination edge the pointer lands. Must be
/// larger than [`EDGE_TRIGGER`] so a warp never immediately re-triggers
/// the reverse link.
pub const WARP_INSET: i32 = 8;

/// While the pointer is within this many px of a linked edge, the WM
/// asks for another manage sequence to keep pointer samples flowing —
/// river only reports pointer position during manage sequences, so this
/// is what makes edge warping feel immediate. The poll stops as soon as
/// the pointer stops moving or leaves the zone.
pub const POLL_ZONE: i32 = 64;

/// User-configured edge links, stored by the output tokens the user
/// typed (connector name or 1-based index) so config can be applied
/// before outputs exist and survives hotplug.
#[derive(Default, Debug)]
pub struct EdgeLinks {
    links: Vec<(EdgeRef, EdgeRef)>,
}

impl EdgeLinks {
    /// Install a bidirectional link. Any previous link touching either
    /// endpoint's (output, edge) slot is replaced — an edge is a single
    /// portal, not a fan-out.
    pub fn link(&mut self, a: EdgeRef, b: EdgeRef) {
        self.links.retain(|(x, y)| {
            !same_slot(x, &a) && !same_slot(y, &a) && !same_slot(x, &b) && !same_slot(y, &b)
        });
        self.links.push((a, b));
    }

    /// Remove every link touching the given (output, edge) slot.
    /// Returns how many links were removed.
    pub fn unlink(&mut self, at: &EdgeRef) -> usize {
        let before = self.links.len();
        self.links
            .retain(|(x, y)| !same_slot(x, at) && !same_slot(y, at));
        before - self.links.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(EdgeRef, EdgeRef)> {
        self.links.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

fn same_slot(a: &EdgeRef, b: &EdgeRef) -> bool {
    a.output == b.output && a.edge == b.edge
}

/// One direction of a link with both endpoints resolved to live output
/// rectangles (global compositor coordinates).
#[derive(Copy, Clone, Debug)]
pub struct ResolvedLink {
    pub from: Rect,
    pub from_edge: Edge,
    pub to: Rect,
    pub to_edge: Edge,
}

/// Where the pointer should warp to, if anywhere.
///
/// `outputs` is every live output rectangle — used to detect natural
/// adjacency: a link only fires where the pointer cannot cross on its
/// own.
pub fn warp_destination(
    pointer: (i32, i32),
    links: &[ResolvedLink],
    outputs: &[Rect],
) -> Option<(i32, i32)> {
    for link in links {
        if !contains(link.from, pointer) {
            continue;
        }
        if edge_distance(link.from, link.from_edge, pointer) > EDGE_TRIGGER {
            continue;
        }
        // If the point just beyond the edge is on some output, the
        // compositor lets the pointer through naturally — don't warp.
        let beyond = step_beyond(link.from, link.from_edge, pointer);
        if outputs.iter().any(|r| contains(*r, beyond)) {
            continue;
        }
        let dest = map_through(pointer, link);
        if dest != pointer {
            return Some(dest);
        }
    }
    None
}

/// `true` when the pointer is inside `rect` and within [`POLL_ZONE`] of
/// any linked `from` edge — the signal to keep pointer samples flowing.
pub fn near_linked_edge(pointer: (i32, i32), links: &[ResolvedLink]) -> bool {
    links.iter().any(|link| {
        contains(link.from, pointer)
            && edge_distance(link.from, link.from_edge, pointer) <= POLL_ZONE
    })
}

fn contains(r: Rect, p: (i32, i32)) -> bool {
    p.0 >= r.x && p.0 < r.x + r.w as i32 && p.1 >= r.y && p.1 < r.y + r.h as i32
}

/// Distance (px) from the pointer to the given edge of `rect`.
fn edge_distance(r: Rect, edge: Edge, p: (i32, i32)) -> i32 {
    match edge {
        Edge::Left => p.0 - r.x,
        Edge::Right => (r.x + r.w as i32 - 1) - p.0,
        Edge::Top => p.1 - r.y,
        Edge::Bottom => (r.y + r.h as i32 - 1) - p.1,
    }
}

/// The point one pixel past the given edge, straight out from `p`.
fn step_beyond(r: Rect, edge: Edge, p: (i32, i32)) -> (i32, i32) {
    match edge {
        Edge::Left => (r.x - 1, p.1),
        Edge::Right => (r.x + r.w as i32, p.1),
        Edge::Top => (p.0, r.y - 1),
        Edge::Bottom => (p.0, r.y + r.h as i32),
    }
}

/// Map the pointer through a link: preserve the fractional position
/// along the source edge, land [`WARP_INSET`] px inside the destination
/// edge.
fn map_through(p: (i32, i32), link: &ResolvedLink) -> (i32, i32) {
    let t = match link.from_edge {
        Edge::Left | Edge::Right => fraction(p.1, link.from.y, link.from.h),
        Edge::Top | Edge::Bottom => fraction(p.0, link.from.x, link.from.w),
    };
    let to = link.to;
    let (x, y) = match link.to_edge {
        Edge::Left => (to.x + WARP_INSET, along(t, to.y, to.h)),
        Edge::Right => (to.x + to.w as i32 - 1 - WARP_INSET, along(t, to.y, to.h)),
        Edge::Top => (along(t, to.x, to.w), to.y + WARP_INSET),
        Edge::Bottom => (along(t, to.x, to.w), to.y + to.h as i32 - 1 - WARP_INSET),
    };
    // Clamp inside the destination rect for degenerate sizes.
    (
        x.clamp(to.x, to.x + (to.w as i32 - 1).max(0)),
        y.clamp(to.y, to.y + (to.h as i32 - 1).max(0)),
    )
}

fn fraction(v: i32, start: i32, len: u32) -> f64 {
    let span = (len as i32 - 1).max(1) as f64;
    ((v - start) as f64 / span).clamp(0.0, 1.0)
}

fn along(t: f64, start: i32, len: u32) -> i32 {
    let span = (len as i32 - 1).max(0) as f64;
    start + (t * span).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    fn link(from: Rect, from_edge: Edge, to: Rect, to_edge: Edge) -> ResolvedLink {
        ResolvedLink {
            from,
            from_edge,
            to,
            to_edge,
        }
    }

    /// Two 1920x1080 outputs side by side.
    fn side_by_side() -> (Rect, Rect) {
        (r(0, 0, 1920, 1080), r(1920, 0, 1920, 1080))
    }

    // If a warp landed within EDGE_TRIGGER of the destination edge it
    // would immediately bounce back through the reverse link. Checked
    // at compile time so the constants can't drift apart.
    const _: () = assert!(WARP_INSET > EDGE_TRIGGER);

    #[test]
    fn wraparound_link_fires_at_the_far_edge() {
        let (a, b) = side_by_side();
        // Left edge of A wraps to right edge of B.
        let links = [link(a, Edge::Left, b, Edge::Right)];
        let dest = warp_destination((0, 540), &links, &[a, b]).unwrap();
        assert_eq!(dest.0, 1920 + 1920 - 1 - WARP_INSET);
        assert_eq!(dest.1, 540);
    }

    #[test]
    fn linked_edge_does_not_fire_where_screens_are_adjacent() {
        let (a, b) = side_by_side();
        // Linking the shared boundary is redundant — the pointer crosses
        // naturally there, so the link must stay silent.
        let links = [link(a, Edge::Right, b, Edge::Left)];
        assert_eq!(warp_destination((1919, 540), &links, &[a, b]), None);
    }

    #[test]
    fn link_fires_once_the_neighbour_is_gone() {
        let (a, b) = side_by_side();
        let links = [link(a, Edge::Right, b, Edge::Left)];
        // Same link, but B's rect is elsewhere (e.g. stacked above) —
        // now the right edge of A is a real boundary and the link fires.
        let b_moved = r(0, -1080, 1920, 1080);
        let links_moved = [link(a, Edge::Right, b_moved, Edge::Left)];
        let dest = warp_destination((1919, 540), &links_moved, &[a, b_moved]).unwrap();
        assert_eq!(dest.0, WARP_INSET);
        // And with B actually adjacent nothing fires (control).
        assert_eq!(warp_destination((1919, 540), &links, &[a, b]), None);
    }

    #[test]
    fn pointer_away_from_edge_does_not_warp() {
        let (a, b) = side_by_side();
        let links = [link(a, Edge::Left, b, Edge::Right)];
        assert_eq!(warp_destination((100, 540), &links, &[a, b]), None);
    }

    #[test]
    fn fraction_along_edge_is_preserved() {
        // A is 1080 tall, B is 2160 tall: a pointer 25% down A's left
        // edge lands 25% down B's right edge.
        let a = r(0, 0, 1920, 1080);
        let b = r(5000, 0, 1000, 2160);
        let links = [link(a, Edge::Left, b, Edge::Right)];
        let dest = warp_destination((0, 270), &links, &[a, b]).unwrap();
        let expected_y = (0.25f64 * (2160.0 - 1.0)).round() as i32;
        assert_eq!(dest.1, expected_y);
    }

    #[test]
    fn vertical_edge_can_link_to_horizontal_edge() {
        // Right edge of A → top edge of B (screens at right angles).
        let a = r(0, 0, 1000, 1000);
        let b = r(3000, 3000, 2000, 500);
        let links = [link(a, Edge::Right, b, Edge::Top)];
        let dest = warp_destination((999, 500), &links, &[a, b]).unwrap();
        // Fraction down A's right edge carries over to B's top edge.
        assert_eq!(dest.1, 3000 + WARP_INSET);
        let t = 500f64 / 999.0;
        let expected_x = 3000 + (t * (2000.0 - 1.0)).round() as i32;
        assert_eq!(dest.0, expected_x);
    }

    #[test]
    fn destination_is_clamped_into_the_target() {
        let a = r(0, 0, 100, 100);
        let tiny = r(500, 500, 4, 4);
        let links = [link(a, Edge::Right, tiny, Edge::Left)];
        let dest = warp_destination((99, 50), &links, &[a, tiny]).unwrap();
        assert!(dest.0 >= 500 && dest.0 < 504);
        assert!(dest.1 >= 500 && dest.1 < 504);
    }

    #[test]
    fn near_linked_edge_tracks_the_poll_zone() {
        let (a, b) = side_by_side();
        let links = [link(a, Edge::Left, b, Edge::Right)];
        assert!(near_linked_edge((POLL_ZONE, 540), &links));
        assert!(!near_linked_edge((POLL_ZONE + 1, 540), &links));
        // Inside the other output — its own edges aren't linked here.
        assert!(!near_linked_edge((1920 + 5, 540), &links));
    }

    #[test]
    fn edge_links_store_replaces_conflicting_slots() {
        let mut store = EdgeLinks::default();
        store.link(
            EdgeRef::parse("1:left").unwrap(),
            EdgeRef::parse("2:right").unwrap(),
        );
        // Re-linking 1:left elsewhere replaces the old pair entirely.
        store.link(
            EdgeRef::parse("1:left").unwrap(),
            EdgeRef::parse("3:right").unwrap(),
        );
        let pairs: Vec<_> = store.iter().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1.output, "3");
    }

    #[test]
    fn edge_links_unlink_removes_by_either_endpoint() {
        let mut store = EdgeLinks::default();
        store.link(
            EdgeRef::parse("1:left").unwrap(),
            EdgeRef::parse("2:right").unwrap(),
        );
        assert_eq!(store.unlink(&EdgeRef::parse("2:right").unwrap()), 1);
        assert!(store.is_empty());
        assert_eq!(store.unlink(&EdgeRef::parse("2:right").unwrap()), 0);
    }
}
