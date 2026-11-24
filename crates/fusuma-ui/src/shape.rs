//! The shape of a screen and the part of it a layout can use. A watch has rounded corners, a round watch is a circle,
//! a phone may have a notch: content placed in a corner is cut off. A board says its [`Shape`]; the toolkit works out
//! the [`Insets`] that keep a rectangle of content inside it, and an app lays out in what is left.

/// The outline of the visible screen inside its pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// Square corners: everything on the rectangle is visible.
    #[default]
    Rect,
    /// A rectangle with rounded corners of this radius in pixels (a smartwatch, a phone).
    Rounded(u32),
    /// A circle, as wide as the screen is wide.
    Circle,
}

/// How far each edge of the screen is unusable, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    /// From the top edge.
    pub top: u32,
    /// From the right edge.
    pub right: u32,
    /// From the bottom edge.
    pub bottom: u32,
    /// From the left edge.
    pub left: u32,
}

impl Insets {
    /// The same on every side.
    #[must_use]
    pub const fn all(v: u32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// The largest of the four.
    #[must_use]
    pub const fn max(&self) -> u32 {
        let a = if self.top > self.bottom {
            self.top
        } else {
            self.bottom
        };
        let b = if self.left > self.right {
            self.left
        } else {
            self.right
        };
        if a > b { a } else { b }
    }

    /// These insets scaled by `percent` (a display scale of 200 doubles them).
    #[must_use]
    pub const fn scaled(&self, percent: u32) -> Self {
        const fn s(v: u32, p: u32) -> u32 {
            let r = v as u64 * p as u64 / 100;
            if r > u32::MAX as u64 {
                u32::MAX
            } else {
                r as u32
            }
        }
        Self {
            top: s(self.top, percent),
            right: s(self.right, percent),
            bottom: s(self.bottom, percent),
            left: s(self.left, percent),
        }
    }
}

impl Shape {
    /// The corner radius in pixels on a screen `width` x `height`: 0 for a rectangle, the radius itself for a rounded
    /// one (never more than half the short side), half the width for a circle.
    #[must_use]
    pub const fn radius(self, width: u32, height: u32) -> u32 {
        let half = (if width < height { width } else { height }) / 2;
        match self {
            Self::Rect => 0,
            Self::Rounded(r) => {
                if r > half {
                    half
                } else {
                    r
                }
            }
            Self::Circle => half,
        }
    }

    /// The insets that keep a rectangle of content inside the visible area: a square corner of the content must
    /// not poke out of a corner of radius `r`, which it does not once it is `r (1 - 1/sqrt 2)` (about 0.293 r) in
    /// from both edges. The same on all four sides, so content stays centred. A circle is the case `r` = width / 2.
    #[must_use]
    pub const fn safe_insets(self, width: u32, height: u32) -> Insets {
        let r = self.radius(width, height) as u64;
        // 0.293 r, rounded up so the corner is clear by a pixel.
        let v = (r * 293).div_ceil(1000);
        Insets::all(if v > u32::MAX as u64 {
            u32::MAX
        } else {
            v as u32
        })
    }

    /// How far in from the left and right edges the visible area begins at height `y` (0 at the top), for a layout
    /// that wants to use the room beside a header instead of a single margin. 0 on a rectangle.
    #[must_use]
    pub const fn edge_at(self, width: u32, height: u32, y: u32) -> u32 {
        let r = self.radius(width, height) as i64;
        let y = y as i64;
        let from_edge = if y < height as i64 - y {
            y
        } else {
            height as i64 - y
        };
        if r == 0 || from_edge >= r {
            return 0;
        }
        let from_edge = if from_edge < 0 { 0 } else { from_edge };
        // The circle of radius r has its centre r in and r down: x = r - sqrt(r^2 - (r - y)^2).
        let dy = r - from_edge;
        let inside = r * r - dy * dy;
        // Integer square root by bisection (no float on a chip without an FPU, and `no_std`).
        let (mut lo, mut hi) = (0i64, r);
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            if mid * mid <= inside {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        (r - lo) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_has_no_corner_and_needs_no_inset() {
        assert_eq!(Shape::Rect.radius(410, 502), 0);
        assert_eq!(Shape::Rect.safe_insets(410, 502), Insets::all(0));
        assert_eq!(Shape::Rect.edge_at(410, 502, 0), 0);
    }

    #[test]
    fn a_rounded_screen_is_inset_by_about_a_third_of_its_radius() {
        assert_eq!(
            Shape::Rounded(60).safe_insets(410, 502),
            Insets::all(18),
            "0.293 * 60 = 17.6, up"
        );
        assert_eq!(Shape::Rounded(100).safe_insets(410, 502), Insets::all(30));
        assert_eq!(Shape::Rounded(1).safe_insets(410, 502), Insets::all(1));
    }

    #[test]
    fn a_radius_larger_than_the_screen_allows_is_held_to_half_the_short_side() {
        assert_eq!(Shape::Rounded(9999).radius(410, 502), 205);
        assert_eq!(
            Shape::Rounded(9999).safe_insets(100, 40),
            Insets::all(6),
            "half of 40 is 20; 0.293 * 20 = 5.9"
        );
    }

    #[test]
    fn a_circle_is_a_rounded_rectangle_with_half_the_width_as_its_radius() {
        assert_eq!(Shape::Circle.radius(240, 240), 120);
        assert_eq!(
            Shape::Circle.safe_insets(240, 240),
            Shape::Rounded(120).safe_insets(240, 240)
        );
        assert_eq!(
            Shape::Circle.safe_insets(240, 240),
            Insets::all(36),
            "0.146 of the diameter"
        );
    }

    #[test]
    fn the_inset_really_keeps_a_content_corner_inside_the_arc() {
        for r in [8u32, 20, 41, 60, 120] {
            let i = i64::from(Shape::Rounded(r).safe_insets(1000, 1000).top);
            // The content corner is at (i, i); the arc's centre is (r, r): the corner must be within r of it.
            let (dx, dy) = (i64::from(r) - i, i64::from(r) - i);
            assert!(
                dx * dx + dy * dy <= i64::from(r) * i64::from(r),
                "radius {r}, inset {i}"
            );
            // And one pixel less is not enough, so the inset is not wasteful.
            let j = i - 2;
            if j >= 0 {
                let (dx, dy) = (i64::from(r) - j, i64::from(r) - j);
                assert!(
                    dx * dx + dy * dy > i64::from(r) * i64::from(r),
                    "radius {r}, inset {j} is already enough"
                );
            }
        }
    }

    #[test]
    fn the_edge_is_furthest_in_at_the_top_and_bottom_and_zero_past_the_corner() {
        let s = Shape::Rounded(60);
        assert_eq!(
            s.edge_at(410, 502, 0),
            60,
            "at the very top the visible area starts a radius in"
        );
        assert_eq!(
            s.edge_at(410, 502, 501),
            s.edge_at(410, 502, 1),
            "the last row mirrors the second"
        );
        assert_eq!(s.edge_at(410, 502, 60), 0);
        assert_eq!(s.edge_at(410, 502, 250), 0);
        let mut last = u32::MAX;
        for y in 0..=60 {
            let e = s.edge_at(410, 502, y);
            assert!(
                e <= last,
                "never moves out again going down: row {y}: {e} after {last}"
            );
            last = e;
        }
    }

    #[test]
    fn odd_sizes_do_not_panic() {
        for s in [
            Shape::Rect,
            Shape::Rounded(0),
            Shape::Rounded(u32::MAX),
            Shape::Circle,
        ] {
            for (w, h) in [(0, 0), (1, 1), (u32::MAX, 3), (3, u32::MAX)] {
                let _ = (s.radius(w, h), s.safe_insets(w, h));
                for y in [0, 1, h / 2, h.saturating_sub(1), u32::MAX] {
                    let _ = s.edge_at(w, h, y);
                }
            }
        }
    }

    #[test]
    fn insets_scale_and_report_their_largest_side() {
        let i = Insets {
            top: 10,
            right: 4,
            bottom: 6,
            left: 2,
        };
        assert_eq!(i.max(), 10);
        assert_eq!(
            i.scaled(200),
            Insets {
                top: 20,
                right: 8,
                bottom: 12,
                left: 4
            }
        );
        assert_eq!(Insets::all(u32::MAX).scaled(800), Insets::all(u32::MAX));
    }
}
