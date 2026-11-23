//! Integer pixel geometry. Everything is whole pixels, so the same scene lays out the same on a
//! chip without an FPU and on a desktop.

/// A point in pixels. The origin is the top-left corner; `y` grows downwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    /// Horizontal position.
    pub x: i32,
    /// Vertical position.
    pub y: i32,
}

impl Point {
    /// A point.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A size in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl Size {
    /// A size.
    #[must_use]
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }
}

/// A rectangle: top-left corner plus size. A rectangle with zero width or height is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl Rect {
    /// A rectangle.
    #[must_use]
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// Whether the rectangle covers no pixels.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// One past the last column (`x + w`), saturating at `i32::MAX`.
    #[must_use]
    pub fn right(&self) -> i32 {
        (i64::from(self.x) + i64::from(self.w)).min(i64::from(i32::MAX)) as i32
    }

    /// One past the last row (`y + h`), saturating at `i32::MAX`.
    #[must_use]
    pub fn bottom(&self) -> i32 {
        (i64::from(self.y) + i64::from(self.h)).min(i64::from(i32::MAX)) as i32
    }

    /// Whether the pixel at `p` is inside.
    #[must_use]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right() && p.y < self.bottom()
    }

    /// The overlap of two rectangles; an empty rectangle when they do not touch.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        let (x0, y0) = (self.x.max(other.x), self.y.max(other.y));
        let (x1, y1) = (
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        if x1 <= x0 || y1 <= y0 {
            return Self::new(x0, y0, 0, 0);
        }
        // i64: from i32::MIN to i32::MAX is 2^32 - 1 wide, which does not fit an i32.
        let (w, h) = (i64::from(x1) - i64::from(x0), i64::from(y1) - i64::from(y0));
        Self::new(x0, y0, w as u32, h as u32)
    }

    /// The smallest rectangle that covers both. An empty rectangle covers nothing, so the union with
    /// one is the other; the union of two empty rectangles is the empty rectangle at the origin.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() && other.is_empty() {
            return Self::default();
        }
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let (x0, y0) = (self.x.min(other.x), self.y.min(other.y));
        let (x1, y1) = (
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        );
        // i64: from i32::MIN to i32::MAX is wider than an i32 (as in `intersect`).
        let (w, h) = (i64::from(x1) - i64::from(x0), i64::from(y1) - i64::from(y0));
        Self::new(
            x0,
            y0,
            w.min(i64::from(u32::MAX)) as u32,
            h.min(i64::from(u32::MAX)) as u32,
        )
    }

    /// The rectangle moved by `(dx, dy)`, saturating at the `i32` range.
    #[must_use]
    pub const fn translate(&self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
            ..*self
        }
    }

    /// The rectangle shrunk by `by` on every side (empty when it vanishes).
    #[must_use]
    pub fn inset(&self, by: u32) -> Self {
        let twice = by.saturating_mul(2);
        if twice >= self.w || twice >= self.h {
            return Self::new(self.x, self.y, 0, 0);
        }
        Self::new(
            self.x.saturating_add(by.min(i32::MAX as u32) as i32),
            self.y.saturating_add(by.min(i32::MAX as u32) as i32),
            self.w - twice,
            self.h - twice,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_containment() {
        let r = Rect::new(10, 20, 5, 3);
        assert_eq!((r.right(), r.bottom()), (15, 23));
        assert!(r.contains(Point::new(10, 20)));
        assert!(r.contains(Point::new(14, 22)));
        assert!(!r.contains(Point::new(15, 22)));
        assert!(!r.contains(Point::new(9, 20)));
        assert!(!Rect::new(0, 0, 0, 5).contains(Point::new(0, 0)));
    }

    #[test]
    fn intersect_overlaps_touches_and_misses() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(&Rect::new(5, 5, 10, 10)), Rect::new(5, 5, 5, 5));
        assert!(
            a.intersect(&Rect::new(10, 0, 5, 5)).is_empty(),
            "touching edges overlap nowhere"
        );
        assert!(a.intersect(&Rect::new(-9, -9, 4, 4)).is_empty());
        assert_eq!(a.intersect(&Rect::new(-5, -5, 100, 100)), a);
    }

    #[test]
    fn extremes_do_not_overflow() {
        let huge = Rect::new(i32::MAX - 1, 0, u32::MAX, 1);
        assert_eq!(huge.right(), i32::MAX);
        assert_eq!(Rect::new(i32::MIN, 0, u32::MAX, 1).right(), i32::MAX);
        assert_eq!(huge.translate(10, 0).x, i32::MAX);
        assert!(
            !huge
                .intersect(&Rect::new(0, 0, 5, 5))
                .contains(Point::new(0, 0))
        );
    }

    #[test]
    fn regression_intersect_across_the_whole_i32_range_does_not_overflow() {
        // Found by the geometry fuzz target.
        let wide = Rect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
        let i = wide.intersect(&wide);
        assert_eq!((i.x, i.y), (i32::MIN, i32::MIN));
        assert_eq!((i.w, i.h), (u32::MAX, u32::MAX));
        assert_eq!(
            wide.intersect(&Rect::new(0, 0, 4, 4)),
            Rect::new(0, 0, 4, 4)
        );
    }

    #[test]
    fn union_covers_both_and_ignores_empty_rectangles() {
        let (a, b) = (Rect::new(0, 0, 10, 10), Rect::new(20, 5, 10, 10));
        let u = a.union(&b);
        assert_eq!(u, Rect::new(0, 0, 30, 15));
        assert_eq!(u, b.union(&a), "order does not matter");
        assert_eq!(a.union(&a), a);
        assert_eq!(
            a.union(&Rect::new(99, 99, 0, 5)),
            a,
            "an empty rectangle adds nothing"
        );
        assert_eq!(Rect::new(99, 99, 5, 0).union(&b), b);
        assert_eq!(
            Rect::new(5, 6, 0, 3).union(&Rect::new(7, 8, 2, 0)),
            Rect::default()
        );
        assert_eq!(
            Rect::new(7, 8, 2, 0).union(&Rect::new(5, 6, 0, 3)),
            Rect::default(),
            "symmetric even for two empty ones"
        );
        let inner = Rect::new(2, 2, 3, 3);
        assert_eq!(a.union(&inner), a);
    }

    #[test]
    fn regression_union_across_the_whole_i32_range_does_not_overflow() {
        let (lo, hi) = (
            Rect::new(i32::MIN, i32::MIN, 1, 1),
            Rect::new(i32::MAX - 1, i32::MAX - 1, 1, 1),
        );
        let u = lo.union(&hi);
        assert_eq!((u.x, u.y), (i32::MIN, i32::MIN));
        assert_eq!(
            (u.w, u.h),
            (u32::MAX, u32::MAX),
            "saturates at the widest rectangle"
        );
    }

    #[test]
    fn inset_shrinks_and_vanishes() {
        assert_eq!(Rect::new(0, 0, 10, 8).inset(2), Rect::new(2, 2, 6, 4));
        assert!(Rect::new(0, 0, 10, 8).inset(4).is_empty());
        assert!(Rect::new(0, 0, 10, 8).inset(u32::MAX).is_empty());
        assert_eq!(Rect::new(1, 1, 4, 4).inset(0), Rect::new(1, 1, 4, 4));
    }
}
