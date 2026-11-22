//! Path flattening and an even-odd scanline fill, in integers and without allocation.

use tokonoma::{PathCmd, Point, Rect};

/// Most edges a path may flatten to.
pub const MAX_EDGES: usize = 160;
/// Most places one scanline may cross edges.
const MAX_CROSSINGS: usize = 64;
/// Segments a quadratic curve is cut into.
const QUAD_STEPS: i64 = 8;
/// Segments a cubic curve is cut into.
const CUBIC_STEPS: i64 = 10;

/// The path has more edges, or crosses a row more often, than fixed storage allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooComplex;

/// A straight piece of a flattened path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edge {
    /// Start.
    pub a: Point,
    /// End.
    pub b: Point,
}

/// A flattened path: a fixed array of edges.
#[derive(Clone, Copy, Debug)]
pub struct Edges {
    list: [Edge; MAX_EDGES],
    len: usize,
}

impl Edges {
    fn new() -> Self {
        Self {
            list: [Edge::default(); MAX_EDGES],
            len: 0,
        }
    }

    fn push(&mut self, a: Point, b: Point) -> Result<(), TooComplex> {
        if a == b {
            return Ok(());
        }
        *self.list.get_mut(self.len).ok_or(TooComplex)? = Edge { a, b };
        self.len += 1;
        Ok(())
    }

    /// The edges.
    #[must_use]
    pub fn as_slice(&self) -> &[Edge] {
        &self.list[..self.len]
    }
}

fn clamp_i32(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn lerp_quad(i: i64, p: [Point; 3]) -> Point {
    let (s, t) = (QUAD_STEPS - i, i);
    let den = QUAD_STEPS * QUAD_STEPS;
    let f = |a: i32, b: i32, c: i32| {
        clamp_i32(
            (s * s * i64::from(a) + 2 * s * t * i64::from(b) + t * t * i64::from(c) + den / 2)
                .div_euclid(den),
        )
    };
    Point::new(f(p[0].x, p[1].x, p[2].x), f(p[0].y, p[1].y, p[2].y))
}

fn lerp_cubic(i: i64, p: [Point; 4]) -> Point {
    let (s, t) = (CUBIC_STEPS - i, i);
    let den = CUBIC_STEPS * CUBIC_STEPS * CUBIC_STEPS;
    let f = |a: i32, b: i32, c: i32, d: i32| {
        clamp_i32(
            (s * s * s * i64::from(a)
                + 3 * s * s * t * i64::from(b)
                + 3 * s * t * t * i64::from(c)
                + t * t * t * i64::from(d)
                + den / 2)
                .div_euclid(den),
        )
    };
    Point::new(
        f(p[0].x, p[1].x, p[2].x, p[3].x),
        f(p[0].y, p[1].y, p[2].y, p[3].y),
    )
}

/// Cuts `path` into straight edges. With `close_open`, every sub-path is closed (what a fill
/// needs); without, only the ones with a `Close` command are (what a stroke needs).
///
/// # Errors
/// [`TooComplex`] when the path flattens to more than [`MAX_EDGES`] edges.
pub fn flatten(path: &[PathCmd], close_open: bool) -> Result<Edges, TooComplex> {
    let mut edges = Edges::new();
    let (mut start, mut cur) = (Point::default(), Point::default());
    let mut open = false;
    let close = |edges: &mut Edges, cur: Point, start: Point| edges.push(cur, start);
    for cmd in path {
        match *cmd {
            PathCmd::MoveTo(p) => {
                if open && close_open {
                    close(&mut edges, cur, start)?;
                }
                (start, cur, open) = (p, p, true);
            }
            PathCmd::LineTo(p) => {
                edges.push(cur, p)?;
                (cur, open) = (p, true);
            }
            PathCmd::QuadTo(c, p) => {
                let mut last = cur;
                for i in 1..=QUAD_STEPS {
                    let next = lerp_quad(i, [cur, c, p]);
                    edges.push(last, next)?;
                    last = next;
                }
                (cur, open) = (p, true);
            }
            PathCmd::CubicTo(c1, c2, p) => {
                let mut last = cur;
                for i in 1..=CUBIC_STEPS {
                    let next = lerp_cubic(i, [cur, c1, c2, p]);
                    edges.push(last, next)?;
                    last = next;
                }
                (cur, open) = (p, true);
            }
            PathCmd::Close => {
                close(&mut edges, cur, start)?;
                (cur, open) = (start, false);
            }
        }
    }
    if open && close_open {
        close(&mut edges, cur, start)?;
    }
    Ok(edges)
}

/// `a / b` rounded up, for `b > 0`.
fn ceil_div(a: i128, b: i128) -> i128 {
    -((-a).div_euclid(b))
}

/// Fills `edges` with the even-odd rule, sampling pixel centers, inside `bounds`. Calls
/// `emit(y, x_start, x_end)` for every run of covered pixels (`x_end` is exclusive).
///
/// # Errors
/// [`TooComplex`] when a row crosses more edges than fixed storage holds.
pub fn fill_spans(
    edges: &Edges,
    bounds: Rect,
    mut emit: impl FnMut(i32, i32, i32),
) -> Result<(), TooComplex> {
    if bounds.is_empty() || edges.len == 0 {
        return Ok(());
    }
    let list = edges.as_slice();
    let top = list
        .iter()
        .map(|e| e.a.y.min(e.b.y))
        .min()
        .unwrap_or(0)
        .max(bounds.y);
    let bottom = list
        .iter()
        .map(|e| e.a.y.max(e.b.y))
        .max()
        .unwrap_or(0)
        .min(bounds.bottom());
    for y in top..bottom {
        let mut xs = [0i32; MAX_CROSSINGS];
        let mut n = 0;
        for e in list {
            let (lo, hi) = if e.a.y <= e.b.y {
                (e.a, e.b)
            } else {
                (e.b, e.a)
            };
            if y < lo.y || y >= hi.y {
                continue;
            }
            // i128: with coordinates near the ends of the i32 range the products exceed i64.
            let (dy, dx) = (
                i128::from(hi.y) - i128::from(lo.y),
                i128::from(hi.x) - i128::from(lo.x),
            );
            let num = 2 * i128::from(lo.x) * dy + (2 * (i128::from(y) - i128::from(lo.y)) + 1) * dx;
            let x =
                ceil_div(num - dy, 2 * dy).clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32;
            *xs.get_mut(n).ok_or(TooComplex)? = x;
            n += 1;
        }
        let row = &mut xs[..n];
        row.sort_unstable();
        let (pairs, _) = row.as_chunks::<2>();
        for &[a, b] in pairs {
            let (x0, x1) = (a.max(bounds.x), b.min(bounds.right()));
            if x1 > x0 {
                emit(y, x0, x1);
            }
        }
    }
    Ok(())
}

/// The ten commands of a rectangle with round corners of radius `r` (at most half of each side),
/// with each corner as one cubic curve.
#[must_use]
pub fn rounded_rect(rect: Rect, r: u32) -> [PathCmd; 10] {
    let r = i64::from(r.min(rect.w / 2).min(rect.h / 2));
    // A quarter circle as a cubic: the control points sit 0.5523 r from the corner.
    let k = (r * 5523 + 5000) / 10000;
    let (x0, y0) = (i64::from(rect.x), i64::from(rect.y));
    let (x1, y1) = (x0 + i64::from(rect.w), y0 + i64::from(rect.h));
    let p = |x: i64, y: i64| Point::new(clamp_i32(x), clamp_i32(y));
    [
        PathCmd::MoveTo(p(x0 + r, y0)),
        PathCmd::LineTo(p(x1 - r, y0)),
        PathCmd::CubicTo(p(x1 - r + k, y0), p(x1, y0 + r - k), p(x1, y0 + r)),
        PathCmd::LineTo(p(x1, y1 - r)),
        PathCmd::CubicTo(p(x1, y1 - r + k), p(x1 - r + k, y1), p(x1 - r, y1)),
        PathCmd::LineTo(p(x0 + r, y1)),
        PathCmd::CubicTo(p(x0 + r - k, y1), p(x0, y1 - r + k), p(x0, y1 - r)),
        PathCmd::LineTo(p(x0, y0 + r)),
        PathCmd::CubicTo(p(x0, y0 + r - k), p(x0 + r - k, y0), p(x0 + r, y0)),
        PathCmd::Close,
    ]
}

/// The integer square root, rounded down.
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The rectangle that a stroke of `width` makes around the segment `a`-`b` (flat ends), as four
/// corners in order. `None` for a zero-length segment. Used for strokes that `embedded-graphics`'
/// own thick lines cannot take without overflowing.
#[must_use]
pub fn stroke_quad(a: Point, b: Point, width: u32) -> Option<[Point; 4]> {
    let (dx, dy) = (
        i128::from(b.x) - i128::from(a.x),
        i128::from(b.y) - i128::from(a.y),
    );
    let len = isqrt((dx * dx + dy * dy) as u128) as i128;
    if len == 0 {
        return None;
    }
    // Half the width along the normal (-dy, dx) / len, rounded to the nearest pixel.
    let w = i128::from(width);
    let round = |n: i128| {
        if n >= 0 {
            (n + len) / (2 * len)
        } else {
            (n - len) / (2 * len)
        }
    };
    let (nx, ny) = (round(-dy * w), round(dx * w));
    let at = |p: Point, sx: i128, sy: i128| {
        Point::new(
            (i128::from(p.x) + sx).clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32,
            (i128::from(p.y) + sy).clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32,
        )
    };
    Some([
        at(a, nx, ny),
        at(b, nx, ny),
        at(b, -nx, -ny),
        at(a, -nx, -ny),
    ])
}

fn outcode(p: Point, r: Rect) -> u8 {
    let (x, y) = (i64::from(p.x), i64::from(p.y));
    let mut code = 0;
    if x < i64::from(r.x) {
        code |= 1;
    } else if x >= i64::from(r.right()) {
        code |= 2;
    }
    if y < i64::from(r.y) {
        code |= 4;
    } else if y >= i64::from(r.bottom()) {
        code |= 8;
    }
    code
}

/// Cuts the segment `a`-`b` to `bounds` (Cohen-Sutherland, in integers). `None` when none of it is
/// inside. A segment that is already inside is returned untouched; a cut one is rounded to whole
/// pixels where it meets the edge.
#[must_use]
pub fn clip_segment(a: Point, b: Point, bounds: Rect) -> Option<(Point, Point)> {
    if bounds.is_empty() {
        return None;
    }
    let (left, top) = (i128::from(bounds.x), i128::from(bounds.y));
    let (right, bottom) = (
        i128::from(bounds.right()) - 1,
        i128::from(bounds.bottom()) - 1,
    );
    let (mut p, mut q) = (a, b);
    // Each pass moves one outside end onto an edge, so four passes per end are enough.
    for _ in 0..8 {
        let (cp, cq) = (outcode(p, bounds), outcode(q, bounds));
        if cp | cq == 0 {
            return Some((p, q));
        }
        if cp & cq != 0 {
            return None;
        }
        let out = if cp != 0 { cp } else { cq };
        let (x0, y0, x1, y1) = (
            i128::from(p.x),
            i128::from(p.y),
            i128::from(q.x),
            i128::from(q.y),
        );
        let (dx, dy) = (x1 - x0, y1 - y0);
        // Where the line meets the edge that this end is beyond; `dx` or `dy` is not 0 there.
        let (x, y) = if out & 8 != 0 {
            (x0 + dx * (bottom - y0) / dy, bottom)
        } else if out & 4 != 0 {
            (x0 + dx * (top - y0) / dy, top)
        } else if out & 2 != 0 {
            (right, y0 + dy * (right - x0) / dx)
        } else {
            (left, y0 + dy * (left - x0) / dx)
        };
        let moved = Point::new(
            x.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32,
            y.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32,
        );
        if cp != 0 {
            p = moved;
        } else {
            q = moved;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;
    use tokonoma::triangle;

    fn covered(edges: &Edges, bounds: Rect) -> usize {
        let mut n = 0;
        fill_spans(edges, bounds, |_, a, b| n += (b - a) as usize).unwrap();
        n
    }
    const BIG: Rect = Rect::new(-1000, -1000, 4000, 4000);

    fn square(x: i32, y: i32, s: i32) -> [PathCmd; 5] {
        [
            PathCmd::MoveTo(Point::new(x, y)),
            PathCmd::LineTo(Point::new(x + s, y)),
            PathCmd::LineTo(Point::new(x + s, y + s)),
            PathCmd::LineTo(Point::new(x, y + s)),
            PathCmd::Close,
        ]
    }

    #[test]
    fn a_square_covers_exactly_its_pixels() {
        let e = flatten(&square(3, 4, 10), true).unwrap();
        assert_eq!(e.as_slice().len(), 4);
        assert_eq!(covered(&e, BIG), 100);
        assert_eq!(
            covered(&e, Rect::new(5, 6, 3, 2)),
            6,
            "limited to the bounds"
        );
        assert_eq!(covered(&e, Rect::new(100, 100, 5, 5)), 0);
        assert_eq!(covered(&e, Rect::new(0, 0, 0, 0)), 0);
    }

    #[test]
    fn spans_are_runs_of_pixel_centers() {
        let e = flatten(&square(0, 0, 4), true).unwrap();
        let mut runs = Vec::new();
        fill_spans(&e, BIG, |y, a, b| runs.push((y, a, b))).unwrap();
        assert_eq!(runs, [(0, 0, 4), (1, 0, 4), (2, 0, 4), (3, 0, 4)]);
    }

    #[test]
    fn a_triangle_covers_about_half_of_its_box() {
        let t = triangle(Point::new(0, 0), Point::new(40, 0), Point::new(0, 40));
        let n = covered(&flatten(&t, true).unwrap(), BIG);
        assert!((780..=820).contains(&n), "{n}");
    }

    #[test]
    fn even_odd_leaves_a_hole() {
        let mut path = Vec::new();
        path.extend_from_slice(&square(0, 0, 20));
        path.extend_from_slice(&square(5, 5, 10));
        let n = covered(&flatten(&path, true).unwrap(), BIG);
        assert_eq!(n, 400 - 100);
    }

    #[test]
    fn open_paths_are_closed_for_fills_and_left_open_for_strokes() {
        let open = [
            PathCmd::MoveTo(Point::new(0, 0)),
            PathCmd::LineTo(Point::new(10, 0)),
            PathCmd::LineTo(Point::new(10, 10)),
        ];
        assert_eq!(flatten(&open, true).unwrap().as_slice().len(), 3);
        assert_eq!(flatten(&open, false).unwrap().as_slice().len(), 2);
        let closed = [
            PathCmd::MoveTo(Point::new(0, 0)),
            PathCmd::LineTo(Point::new(10, 0)),
            PathCmd::LineTo(Point::new(10, 10)),
            PathCmd::Close,
        ];
        assert_eq!(flatten(&closed, false).unwrap().as_slice().len(), 3);
    }

    #[test]
    fn curves_are_cut_into_steps_and_end_on_their_points() {
        let quad = [
            PathCmd::MoveTo(Point::new(0, 0)),
            PathCmd::QuadTo(Point::new(10, 20), Point::new(20, 0)),
        ];
        let e = flatten(&quad, false).unwrap();
        assert_eq!(e.as_slice().len(), 8);
        assert_eq!(e.as_slice().last().map(|e| e.b), Some(Point::new(20, 0)));
        let mid = e.as_slice()[3].b;
        assert!(mid.y >= 8 && mid.y <= 12, "{mid:?}");
        let cubic = [
            PathCmd::MoveTo(Point::new(0, 0)),
            PathCmd::CubicTo(Point::new(0, 30), Point::new(30, 30), Point::new(30, 0)),
        ];
        let c = flatten(&cubic, false).unwrap();
        assert_eq!(c.as_slice().len(), 10);
        assert_eq!(c.as_slice().last().map(|e| e.b), Some(Point::new(30, 0)));
    }

    #[test]
    fn degenerate_and_empty_paths_are_fine() {
        assert_eq!(flatten(&[], true).unwrap().as_slice().len(), 0);
        let dot = [
            PathCmd::MoveTo(Point::new(1, 1)),
            PathCmd::LineTo(Point::new(1, 1)),
            PathCmd::Close,
        ];
        assert_eq!(covered(&flatten(&dot, true).unwrap(), BIG), 0);
        let flat = [
            PathCmd::MoveTo(Point::new(0, 5)),
            PathCmd::LineTo(Point::new(9, 5)),
            PathCmd::Close,
        ];
        assert_eq!(
            covered(&flatten(&flat, true).unwrap(), BIG),
            0,
            "a flat shape has no area"
        );
    }

    #[test]
    fn too_many_edges_is_an_error_not_a_panic() {
        let mut path = Vec::new();
        path.push(PathCmd::MoveTo(Point::new(0, 0)));
        for i in 1..=MAX_EDGES as i32 + 1 {
            path.push(PathCmd::LineTo(Point::new(i, i % 2)));
        }
        assert_eq!(flatten(&path, false).unwrap_err(), TooComplex);
        // 40 squares = 160 edges (the limit) and 80 crossings on one row (over the 64 slots).
        let mut many = Vec::new();
        for i in 0..40 {
            many.extend_from_slice(&square(i, 0, 100));
        }
        let edges = flatten(&many, true).unwrap();
        assert_eq!(fill_spans(&edges, BIG, |_, _, _| {}), Err(TooComplex));
    }

    #[test]
    fn clip_segment_keeps_inside_cuts_crossing_and_drops_outside() {
        let r = Rect::new(0, 0, 100, 100);
        let (a, b) = (Point::new(10, 10), Point::new(90, 40));
        assert_eq!(
            clip_segment(a, b, r),
            Some((a, b)),
            "inside stays untouched"
        );
        assert_eq!(
            clip_segment(Point::new(-50, 50), Point::new(150, 50), r),
            Some((Point::new(0, 50), Point::new(99, 50)))
        );
        assert_eq!(
            clip_segment(Point::new(50, -50), Point::new(50, 150), r),
            Some((Point::new(50, 0), Point::new(50, 99)))
        );
        assert_eq!(
            clip_segment(Point::new(-10, -10), Point::new(-1, 200), r),
            None,
            "wholly left"
        );
        assert!(
            clip_segment(Point::new(-10, 150), Point::new(150, -10), r).is_some(),
            "a diagonal across a corner"
        );
        assert_eq!(
            clip_segment(Point::new(0, 0), Point::new(5, 5), Rect::new(0, 0, 0, 0)),
            None
        );
        let (p, q) = clip_segment(
            Point::new(i32::MIN, i32::MIN),
            Point::new(i32::MAX, i32::MAX),
            r,
        )
        .unwrap();
        assert!(r.contains(p) && r.contains(q), "{p:?} {q:?}");
        assert_eq!(
            clip_segment(Point::new(i32::MIN, 5), Point::new(i32::MIN, 50), r),
            None
        );
    }

    #[test]
    fn a_stroke_quad_surrounds_its_segment() {
        let q = stroke_quad(Point::new(10, 10), Point::new(50, 10), 6).unwrap();
        let path = [
            PathCmd::MoveTo(q[0]),
            PathCmd::LineTo(q[1]),
            PathCmd::LineTo(q[2]),
            PathCmd::LineTo(q[3]),
            PathCmd::Close,
        ];
        let n = covered(&flatten(&path, true).unwrap(), BIG);
        assert_eq!(n, 40 * 6, "a 40 x 6 band: {q:?}");
        let vertical = stroke_quad(Point::new(5, 0), Point::new(5, 20), 4).unwrap();
        let (min_x, max_x) = (
            vertical.iter().map(|p| p.x).min().unwrap(),
            vertical.iter().map(|p| p.x).max().unwrap(),
        );
        assert_eq!((min_x, max_x), (3, 7));
        assert_eq!(stroke_quad(Point::new(1, 1), Point::new(1, 1), 5), None);
        assert!(
            stroke_quad(
                Point::new(i32::MIN, i32::MIN),
                Point::new(i32::MAX, i32::MAX),
                u32::MAX
            )
            .is_some()
        );
    }

    #[test]
    fn a_rounded_rect_is_a_rect_minus_its_corners() {
        let rect = Rect::new(0, 0, 40, 30);
        let area = |r: u32| covered(&flatten(&rounded_rect(rect, r), true).unwrap(), BIG);
        assert_eq!(area(0), 40 * 30, "no radius is a plain rectangle");
        // Four quarter discs of radius 10 are missing from the corners: pi * 100 = 314 pixels
        // minus the part the rectangle keeps (its corners are 400 - 314 = 86 pixels, about).
        let full = 40 * 30;
        let cut = full - area(10);
        assert!((70..=100).contains(&cut), "{cut}");
        assert_eq!(
            area(1000),
            area(15),
            "the radius is clamped to half the shorter side"
        );
        let mut with_hole = Vec::new();
        with_hole.extend_from_slice(&rounded_rect(rect, 6));
        with_hole.extend_from_slice(&rounded_rect(rect.inset(3), 3));
        let ring = covered(&flatten(&with_hole, true).unwrap(), BIG);
        assert!(ring > 0 && ring < area(6), "{ring}");
    }

    #[test]
    fn integer_square_roots_are_exact() {
        for n in [
            0u128,
            1,
            2,
            3,
            4,
            15,
            16,
            17,
            99,
            100,
            1 << 40,
            u128::from(u64::MAX),
        ] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "{n} -> {r}");
        }
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let p = [
            PathCmd::MoveTo(Point::new(i32::MIN, i32::MIN)),
            PathCmd::LineTo(Point::new(i32::MAX, i32::MIN)),
            PathCmd::LineTo(Point::new(i32::MAX, i32::MAX)),
            PathCmd::QuadTo(Point::new(i32::MAX, 0), Point::new(i32::MIN, i32::MAX)),
            PathCmd::Close,
        ];
        let e = flatten(&p, true).unwrap();
        let _ = covered(&e, Rect::new(0, 0, 10, 10));
    }
}
