//! How much of each pixel a rounded rectangle, a circle or a rounded ring covers, in integer maths, for anti-aliased
//! edges. Straight edges lie on whole pixels, so a pixel there is in or out; on a corner the coverage is the distance
//! from the pixel's centre to the corner's circle (in 1/256 of a pixel), which is the area inside to within a few
//! percent and needs one integer square root.

/// A rounded rectangle in device pixels, `[x0, x1) x [y0, y1)`, with its corner radius in 1/256 of a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoundRect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
    /// Corner radius in 1/256 px, at most half the shorter side.
    pub rad: i64,
}

impl RoundRect {
    /// The rectangle `[x0, x1) x [y0, y1)` with corners of `rad` (1/256 px), cut to half its shorter side.
    pub(crate) fn new(x0: i64, y0: i64, x1: i64, y1: i64, rad: i64) -> Self {
        let half = (x1 - x0).min(y1 - y0).max(0) * 128;
        Self {
            x0,
            y0,
            x1,
            y1,
            rad: rad.clamp(0, half),
        }
    }

    /// Whether it covers nothing.
    pub(crate) fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    /// How many rows from the top and from the bottom hold part of a corner.
    pub(crate) fn corner_rows(&self) -> i64 {
        (self.rad + 255) / 256
    }

    /// Whether row `y` holds part of a corner (else the row is one plain span).
    pub(crate) fn curved_row(&self, y: i64) -> bool {
        let k = self.corner_rows();
        y < self.y0 + k || y >= self.y1 - k
    }

    /// The share of pixel `(x, y)` inside, 0 to 255.
    pub(crate) fn coverage(&self, x: i64, y: i64) -> u8 {
        if x < self.x0 || x >= self.x1 || y < self.y0 || y >= self.y1 {
            return 0;
        }
        // The pixel's centre and its distance past the straight part, both in 1/256 px.
        let (px, py) = (x * 256 + 128, y * 256 + 128);
        let dx = (self.x0 * 256 + self.rad - px)
            .max(px - (self.x1 * 256 - self.rad))
            .max(0);
        let dy = (self.y0 * 256 + self.rad - py)
            .max(py - (self.y1 * 256 - self.rad))
            .max(0);
        if dx == 0 || dy == 0 {
            return 255;
        }
        // Inside the corner's circle by `rad - d`, plus half a pixel for the centre: the share covered.
        // In 128 bits a corner as far out as an `i64` can say does not overflow.
        let d = (dx as u128 * dx as u128 + dy as u128 * dy as u128).isqrt() as i64;
        let c = (self.rad + 128 - d).clamp(0, 256);
        ((c * 255 + 128) / 256) as u8
    }
}

/// A rounded rectangle, or the ring between two of them (a rounded border).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub outer: RoundRect,
    /// The hole of a ring.
    pub inner: Option<RoundRect>,
}

/// What a row of a shape is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    /// Fully covered from `x0` to `x1`.
    Span(i64, i64),
    /// Two spans with the hole between them.
    Bars(i64, i64, i64, i64),
    /// Partly covered pixels somewhere: work it out pixel by pixel from `x0` to `x1`.
    Curved(i64, i64),
    /// Nothing.
    Empty,
}

impl Shape {
    /// The share of pixel `(x, y)` covered, 0 to 255.
    pub(crate) fn coverage(&self, x: i64, y: i64) -> u8 {
        let outer = self.outer.coverage(x, y);
        self.inner
            .map_or(outer, |i| outer.saturating_sub(i.coverage(x, y)))
    }

    /// For a row through a corner: the columns that hold curved edges, `[x0, a)` and `[b, x1)`; between them every
    /// pixel of the row has the same coverage (the straight top or bottom of the outer shape, less the hole's), so it
    /// is one span.
    pub(crate) fn corner_columns(&self) -> (i64, i64) {
        let o = self.outer;
        let a = (o.x0 + o.corner_rows()).min(o.x1);
        (a, (o.x1 - o.corner_rows()).max(a))
    }

    /// What row `y` is, so that most rows are drawn as spans.
    pub(crate) fn row(&self, y: i64) -> Row {
        let o = self.outer;
        if y < o.y0 || y >= o.y1 || o.is_empty() {
            return Row::Empty;
        }
        let hole = self
            .inner
            .filter(|i| !i.is_empty() && y >= i.y0 && y < i.y1);
        match hole {
            _ if o.curved_row(y) => Row::Curved(o.x0, o.x1),
            None => Row::Span(o.x0, o.x1),
            Some(i) if i.curved_row(y) => Row::Curved(o.x0, o.x1),
            Some(i) => Row::Bars(o.x0, i.x0, i.x1, o.x1),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;

    fn area(s: &Shape) -> f64 {
        let o = s.outer;
        (o.y0..o.y1)
            .flat_map(|y| (o.x0..o.x1).map(move |x| (x, y)))
            .map(|(x, y)| f64::from(s.coverage(x, y)) / 255.0)
            .sum()
    }

    fn rr(x0: i64, y0: i64, x1: i64, y1: i64, r: i64) -> Shape {
        Shape {
            outer: RoundRect::new(x0, y0, x1, y1, r * 256),
            inner: None,
        }
    }

    #[test]
    fn a_square_corner_covers_whole_pixels_and_nothing_outside() {
        let s = rr(2, 3, 12, 8, 0);
        assert_eq!(area(&s), 50.0);
        assert_eq!(
            (
                s.coverage(1, 3),
                s.coverage(2, 3),
                s.coverage(11, 7),
                s.coverage(12, 7)
            ),
            (0, 255, 255, 0)
        );
        assert_eq!(s.row(3), Row::Span(2, 12));
        assert_eq!((s.row(2), s.row(8)), (Row::Empty, Row::Empty));
    }

    #[test]
    fn the_covered_area_is_the_shapes_area() {
        // A circle of radius 20 centred on a pixel: pi r^2, the radius running to the pixel edges (20.5).
        let c = Shape {
            outer: RoundRect::new(-20, -20, 21, 21, 41 * 128),
            inner: None,
        };
        let want = core::f64::consts::PI * 20.5 * 20.5;
        assert!(
            (area(&c) - want).abs() / want < 0.01,
            "{} vs {want}",
            area(&c)
        );
        // A rounded rectangle: the box less the four corners' squares outside their quarter circles.
        let r = rr(0, 0, 60, 30, 10);
        let want = 60.0 * 30.0 - (4.0 - core::f64::consts::PI) * 100.0;
        assert!(
            (area(&r) - want).abs() / want < 0.005,
            "{} vs {want}",
            area(&r)
        );
    }

    #[test]
    fn corner_pixels_are_partly_covered_and_symmetric() {
        let r = rr(0, 0, 40, 40, 12);
        let partial = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter(|&(x, y)| (1..255).contains(&r.coverage(x, y)))
            .count();
        assert!(partial > 20, "{partial} soft pixels");
        for (x, y) in [(0, 0), (2, 5), (7, 1)] {
            let c = r.coverage(x, y);
            assert_eq!(c, r.coverage(39 - x, y));
            assert_eq!(c, r.coverage(x, 39 - y));
            assert_eq!(c, r.coverage(y, x));
        }
        assert_eq!(r.coverage(0, 0), 0, "the very corner is outside");
        assert_eq!(r.coverage(20, 0), 255, "the middle of the top edge is in");
    }

    #[test]
    fn a_ring_is_the_outer_less_the_inner_and_rows_say_how_to_draw_them() {
        let ring = Shape {
            outer: RoundRect::new(0, 0, 50, 30, 8 * 256),
            inner: Some(RoundRect::new(2, 2, 48, 28, 6 * 256)),
        };
        let want = area(&rr(0, 0, 50, 30, 8)) - area(&rr(2, 2, 48, 28, 6));
        assert!((area(&ring) - want).abs() < 1e-6);
        assert_eq!(ring.coverage(25, 15), 0, "the hole");
        assert_eq!(ring.coverage(25, 0), 255, "the top bar");
        assert_eq!(ring.row(0), Row::Curved(0, 50));
        assert_eq!(ring.row(15), Row::Bars(0, 2, 48, 50));
        // Every row's kind agrees with the pixels in it.
        for y in -1..31 {
            let px: Vec<u8> = (0..50).map(|x| ring.coverage(x, y)).collect();
            match ring.row(y) {
                Row::Empty => assert!(px.iter().all(|&c| c == 0), "{y}"),
                Row::Span(a, b) => assert!((a..b).all(|x| px[x as usize] == 255), "{y}"),
                Row::Bars(a, b, c, d) => {
                    assert!((a..b).chain(c..d).all(|x| px[x as usize] == 255), "{y}");
                    assert!((b..c).all(|x| px[x as usize] == 0), "{y}");
                }
                Row::Curved(..) => {
                    let (a, b) = ring.corner_columns();
                    assert!(
                        (a..b).all(|x| px[x as usize] == px[a as usize]),
                        "{y}: the middle is even"
                    );
                }
            }
        }
    }

    #[test]
    fn a_radius_past_half_the_side_is_cut_and_huge_coordinates_do_not_overflow() {
        assert_eq!(RoundRect::new(0, 0, 10, 4, 99 * 256).rad, 2 * 256);
        assert_eq!(RoundRect::new(0, 0, 0, 4, 256).rad, 0);
        let far = RoundRect::new(
            i64::from(i32::MIN),
            i64::from(i32::MIN),
            i64::from(i32::MAX),
            i64::from(i32::MAX),
            1 << 40,
        );
        assert_eq!(far.coverage(0, 0), 255);
        let _ = far.coverage(i64::from(i32::MIN), i64::from(i32::MIN));
    }
}
