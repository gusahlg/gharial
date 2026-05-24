//! Pure master-stack layout algorithm. No wayland or runtime dependencies —
//! deliberately kept testable in isolation.

use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Main area on the left, stack on the right.
    Left,
    /// Main area on the right, stack on the left.
    Right,
    /// Main area on top, stack on the bottom.
    Top,
    /// Main area on the bottom, stack on top.
    Bottom,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl FromStr for Orientation {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// Number of views that go in the main area (clamped to view_count).
    pub main_count: u32,
    /// Fraction of the long axis given to the main area. Clamped to [0.05, 0.95].
    pub main_ratio: f32,
    /// Gap between adjacent views and between views and the outer padding.
    pub gaps: u32,
    /// Extra padding around the entire usable area.
    pub outer_padding: u32,
    pub orientation: Orientation,
    /// When true and only one view is visible, drop gaps and outer padding.
    pub smart_gaps: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            main_count: 1,
            main_ratio: 0.55,
            gaps: 8,
            outer_padding: 8,
            orientation: Orientation::Left,
            smart_gaps: true,
        }
    }
}

impl Params {
    pub fn clamp(&mut self) {
        self.main_ratio = self.main_ratio.clamp(0.05, 0.95);
        if self.main_count < 1 {
            self.main_count = 1;
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Compute view positions for the given view count and usable area.
pub fn compute(view_count: u32, usable: (u32, u32), p: &Params) -> Vec<Rect> {
    if view_count == 0 {
        return Vec::new();
    }
    let (uw, uh) = (usable.0 as i32, usable.1 as i32);

    let (outer, gap) = if p.smart_gaps && view_count == 1 {
        (0, 0)
    } else {
        (p.outer_padding as i32, p.gaps as i32)
    };

    let ix = outer;
    let iy = outer;
    let iw = uw - 2 * outer;
    let ih = uh - 2 * outer;
    if iw <= 0 || ih <= 0 {
        return vec![Rect { x: 0, y: 0, w: usable.0, h: usable.1 }; view_count as usize];
    }

    let n = view_count;
    let main_count = p.main_count.min(n);
    let stack_count = n - main_count;

    if stack_count == 0 {
        return split(ix, iy, iw, ih, n, gap, p.orientation);
    }
    if main_count == 0 {
        return split(ix, iy, iw, ih, n, gap, p.orientation);
    }

    // Compute the split rectangles for main and stack areas.
    let (main_box, stack_box) = match p.orientation {
        Orientation::Left | Orientation::Right => {
            let split_gap = gap;
            let avail = iw - split_gap;
            let main_w = (avail as f32 * p.main_ratio).round() as i32;
            let stack_w = avail - main_w;
            if matches!(p.orientation, Orientation::Left) {
                (
                    (ix, iy, main_w, ih),
                    (ix + main_w + split_gap, iy, stack_w, ih),
                )
            } else {
                (
                    (ix + stack_w + split_gap, iy, main_w, ih),
                    (ix, iy, stack_w, ih),
                )
            }
        }
        Orientation::Top | Orientation::Bottom => {
            let split_gap = gap;
            let avail = ih - split_gap;
            let main_h = (avail as f32 * p.main_ratio).round() as i32;
            let stack_h = avail - main_h;
            if matches!(p.orientation, Orientation::Top) {
                (
                    (ix, iy, iw, main_h),
                    (ix, iy + main_h + split_gap, iw, stack_h),
                )
            } else {
                (
                    (ix, iy + stack_h + split_gap, iw, main_h),
                    (ix, iy, iw, stack_h),
                )
            }
        }
    };

    let mut out = Vec::with_capacity(n as usize);
    out.extend(split(main_box.0, main_box.1, main_box.2, main_box.3, main_count, gap, p.orientation));
    out.extend(split(stack_box.0, stack_box.1, stack_box.2, stack_box.3, stack_count, gap, p.orientation));
    out
}

/// Divide a rectangle into `n` equally-tall rows (or columns) separated by `gap`.
/// The axis along which we slice depends on `orientation`: for left/right we
/// stack vertically (rows); for top/bottom we stack horizontally (columns).
fn split(x: i32, y: i32, w: i32, h: i32, n: u32, gap: i32, orientation: Orientation) -> Vec<Rect> {
    let mut out = Vec::with_capacity(n as usize);
    if n == 0 {
        return out;
    }
    let stack_vertical = matches!(orientation, Orientation::Left | Orientation::Right);
    if stack_vertical {
        let total = (h - gap * (n as i32 - 1)).max(n as i32);
        let each = total / n as i32;
        let rem = total - each * n as i32;
        let mut cy = y;
        for i in 0..n as i32 {
            let extra = if i < rem { 1 } else { 0 };
            let hh = each + extra;
            out.push(Rect {
                x,
                y: cy,
                w: w.max(0) as u32,
                h: hh.max(0) as u32,
            });
            cy += hh + gap;
        }
    } else {
        let total = (w - gap * (n as i32 - 1)).max(n as i32);
        let each = total / n as i32;
        let rem = total - each * n as i32;
        let mut cx = x;
        for i in 0..n as i32 {
            let extra = if i < rem { 1 } else { 0 };
            let ww = each + extra;
            out.push(Rect {
                x: cx,
                y,
                w: ww.max(0) as u32,
                h: h.max(0) as u32,
            });
            cx += ww + gap;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(main_count: u32, main_ratio: f32) -> Params {
        Params {
            main_count,
            main_ratio,
            gaps: 0,
            outer_padding: 0,
            orientation: Orientation::Left,
            smart_gaps: false,
        }
    }

    #[test]
    fn zero_views_returns_empty() {
        let r = compute(0, (1920, 1080), &p(1, 0.5));
        assert!(r.is_empty());
    }

    #[test]
    fn single_view_fills_area() {
        let r = compute(1, (1920, 1080), &p(1, 0.5));
        assert_eq!(r, vec![Rect { x: 0, y: 0, w: 1920, h: 1080 }]);
    }

    #[test]
    fn two_views_left_orientation_splits_at_ratio() {
        let r = compute(2, (1000, 500), &p(1, 0.5));
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], Rect { x: 0, y: 0, w: 500, h: 500 });
        assert_eq!(r[1], Rect { x: 500, y: 0, w: 500, h: 500 });
    }

    #[test]
    fn three_views_one_main_two_stack() {
        let r = compute(3, (1000, 500), &p(1, 0.6));
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], Rect { x: 0, y: 0, w: 600, h: 500 });
        assert_eq!(r[1], Rect { x: 600, y: 0, w: 400, h: 250 });
        assert_eq!(r[2], Rect { x: 600, y: 250, w: 400, h: 250 });
    }

    #[test]
    fn smart_gaps_disables_padding_when_alone() {
        let mut params = p(1, 0.5);
        params.gaps = 10;
        params.outer_padding = 20;
        params.smart_gaps = true;
        let r = compute(1, (800, 600), &params);
        assert_eq!(r[0], Rect { x: 0, y: 0, w: 800, h: 600 });
    }

    #[test]
    fn gaps_and_padding_are_subtracted() {
        let mut params = p(1, 0.5);
        params.gaps = 10;
        params.outer_padding = 20;
        params.smart_gaps = false;
        // Inner: 1000-40 = 960 wide, 500-40 = 460 tall.
        // Split: (960-10)/2 = 475 each.
        let r = compute(2, (1000, 500), &params);
        assert_eq!(r[0], Rect { x: 20, y: 20, w: 475, h: 460 });
        assert_eq!(r[1], Rect { x: 20 + 475 + 10, y: 20, w: 475, h: 460 });
    }

    #[test]
    fn right_orientation_places_main_on_right() {
        let mut params = p(1, 0.5);
        params.orientation = Orientation::Right;
        let r = compute(2, (1000, 500), &params);
        assert_eq!(r[0], Rect { x: 500, y: 0, w: 500, h: 500 });
        assert_eq!(r[1], Rect { x: 0, y: 0, w: 500, h: 500 });
    }

    #[test]
    fn top_orientation_splits_horizontally() {
        let mut params = p(1, 0.5);
        params.orientation = Orientation::Top;
        let r = compute(3, (1000, 600), &params);
        assert_eq!(r[0], Rect { x: 0, y: 0, w: 1000, h: 300 });
        assert_eq!(r[1], Rect { x: 0, y: 300, w: 500, h: 300 });
        assert_eq!(r[2], Rect { x: 500, y: 300, w: 500, h: 300 });
    }

    #[test]
    fn extra_main_views_get_remainder_pixels() {
        // 3 views stacked into 500 → 167 + 167 + 166 (167*2+166=500)
        let r = compute(3, (1000, 500), &p(3, 0.5));
        let total_h: u32 = r.iter().map(|r| r.h).sum();
        assert_eq!(total_h, 500);
    }

    #[test]
    fn orientation_roundtrip() {
        for o in [Orientation::Left, Orientation::Right, Orientation::Top, Orientation::Bottom] {
            assert_eq!(Orientation::from_str(o.as_str()), Ok(o));
        }
    }
}
