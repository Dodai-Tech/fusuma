//! Smooth filled paths and strokes in integer maths, with no allocation.
//!
//! A path is flattened into straight edges in 1/16 px. A stroke is built from those same edges: a quad for each
//! segment, a join where two segments meet (miter up to Vello's limit of 4, else bevel) and a round cap at each open
//! end, every piece wound the same way so that a non-zero fill gives their union. The filler takes four samples a
//! pixel row (the middle of each quarter) and measures the horizontal coverage of each exactly to 1/16 px, which is
//! enough to hold a tick or a chevron to Vello's picture.

use fusuma_render::{PathCmd, Point};

/// Units of a pixel: coordinates are 1/16 px.
const SUB: i64 = 16;
/// Most edges a path or stroke may hold.
pub(crate) const MAX_EDGES: usize = 160;
/// Samples taken in each pixel row.
const SAMPLES: i64 = 4;
/// Most places one sample line may cross edges.
const MAX_CROSSINGS: usize = 64;
/// Columns worked at once.
const CHUNK: usize = 256;
/// Segments a quadratic and a cubic curve are cut into, finely, and when that is too many edges, coarsely.
const FINE: (i64, i64) = (12, 16);
const COARSE: (i64, i64) = (6, 8);
/// The sharpest turn that is mitred, as 1000 x its cosine: Vello's miter limit of 4 means `1 + cos >= 1/8`.
const MITER_COS: i64 = -875;
/// A turn gentler than this cosine (in 1/1000) needs only a bevel: its miter tip is a hair from the edge.
const GENTLE: i64 = 900;

/// The path or stroke has more edges, or crosses a sample line more often, than fixed storage allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooComplex;

/// A point in 1/16 px.
type P = (i64, i64);

/// How overlapping or nested outlines count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rule {
    /// Inside when crossed an odd number of times.
    EvenOdd,
    /// Inside when the outlines wind around it at all (a stroke's overlapping pieces).
    NonZero,
}

#[derive(Clone, Copy, Debug, Default)]
struct Seg {
    ax: i32,
    ay: i32,
    bx: i32,
    by: i32,
}

/// A set of straight edges in 1/16 px, in device pixels.
pub(crate) struct Outline {
    list: [Seg; MAX_EDGES],
    len: usize,
}

fn clamp32(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

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

/// One step of a flattened path.
enum Ev {
    Move(P),
    Line(P),
    Close,
}

/// Walks `path` as straight steps (curves cut into pieces), moved by `(tx, ty)` pixels and `shift` (1/16 px).
fn walk(
    path: &[PathCmd],
    (tx, ty): (i64, i64),
    shift: i64,
    (quad_steps, cubic_steps): (i64, i64),
    f: &mut impl FnMut(Ev) -> Result<(), TooComplex>,
) -> Result<(), TooComplex> {
    let pt = |p: Point| {
        (
            (i64::from(p.x) + tx) * SUB + shift,
            (i64::from(p.y) + ty) * SUB + shift,
        )
    };
    let mut cur = pt(Point::new(0, 0));
    let mut start = cur;
    let mut open = false;
    for cmd in path {
        if !open && !matches!(cmd, PathCmd::MoveTo(_) | PathCmd::Close) {
            f(Ev::Move(cur))?;
            open = true;
        }
        match *cmd {
            PathCmd::MoveTo(p) => {
                cur = pt(p);
                start = cur;
                open = true;
                f(Ev::Move(cur))?;
            }
            PathCmd::LineTo(p) => {
                cur = pt(p);
                f(Ev::Line(cur))?;
            }
            PathCmd::QuadTo(c, p) => {
                let (a, b, e) = (cur, pt(c), pt(p));
                let den = quad_steps * quad_steps;
                for i in 1..=quad_steps {
                    let (s, t) = (quad_steps - i, i);
                    let q = |u: i64, v: i64, w: i64| {
                        (s * s * u + 2 * s * t * v + t * t * w + den / 2).div_euclid(den)
                    };
                    f(Ev::Line((q(a.0, b.0, e.0), q(a.1, b.1, e.1))))?;
                }
                cur = e;
            }
            PathCmd::CubicTo(c1, c2, p) => {
                let (a, b, c, e) = (cur, pt(c1), pt(c2), pt(p));
                let den = cubic_steps * cubic_steps * cubic_steps;
                for i in 1..=cubic_steps {
                    let (s, t) = (cubic_steps - i, i);
                    let q = |u: i64, v: i64, w: i64, z: i64| {
                        (s * s * s * u
                            + 3 * s * s * t * v
                            + 3 * s * t * t * w
                            + t * t * t * z
                            + den / 2)
                            .div_euclid(den)
                    };
                    f(Ev::Line((q(a.0, b.0, c.0, e.0), q(a.1, b.1, c.1, e.1))))?;
                }
                cur = e;
            }
            PathCmd::Close => {
                f(Ev::Close)?;
                cur = start;
                open = false;
            }
        }
    }
    Ok(())
}

impl Outline {
    fn new() -> Self {
        Self {
            list: [Seg::default(); MAX_EDGES],
            len: 0,
        }
    }

    /// Whether it holds no edge.
    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn edge(&mut self, a: P, b: P) -> Result<(), TooComplex> {
        if a == b {
            return Ok(());
        }
        *self.list.get_mut(self.len).ok_or(TooComplex)? = Seg {
            ax: clamp32(a.0),
            ay: clamp32(a.1),
            bx: clamp32(b.0),
            by: clamp32(b.1),
        };
        self.len += 1;
        Ok(())
    }

    /// A closed polygon, wound the same way whatever order it came in, so that pieces add up under a non-zero fill.
    fn polygon(&mut self, pts: &[P]) -> Result<(), TooComplex> {
        let n = pts.len();
        let area: i128 = (0..n)
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                i128::from(a.0) * i128::from(b.1) - i128::from(b.0) * i128::from(a.1)
            })
            .sum();
        if area == 0 {
            return Ok(());
        }
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            if area > 0 {
                self.edge(a, b)?;
            } else {
                self.edge(b, a)?;
            }
        }
        Ok(())
    }

    /// The outline of filling `path` moved by `(tx, ty)` pixels: every sub-path closed.
    pub(crate) fn fill(path: &[PathCmd], t: (i64, i64)) -> Result<Self, TooComplex> {
        Self::fill_with(path, t, FINE).or_else(|_| Self::fill_with(path, t, COARSE))
    }

    fn fill_with(path: &[PathCmd], t: (i64, i64), steps: (i64, i64)) -> Result<Self, TooComplex> {
        let mut o = Self::new();
        let (mut start, mut cur, mut open) = ((0, 0), (0, 0), false);
        walk(path, t, 0, steps, &mut |ev| {
            match ev {
                Ev::Move(p) => {
                    if open {
                        o.edge(cur, start)?;
                    }
                    (start, cur, open) = (p, p, true);
                }
                Ev::Line(p) => {
                    o.edge(cur, p)?;
                    cur = p;
                }
                Ev::Close => {
                    o.edge(cur, start)?;
                    cur = start;
                    open = false;
                }
            }
            Ok(())
        })?;
        if open {
            o.edge(cur, start)?;
        }
        Ok(o)
    }

    /// The outline of stroking `path` `width` px wide, moved by `(tx, ty)` pixels, through pixel centres (as Vello
    /// does): miter joins, round caps.
    pub(crate) fn stroke(path: &[PathCmd], width: u32, t: (i64, i64)) -> Result<Self, TooComplex> {
        Self::stroke_with(path, width, t, (10, 14))
            .or_else(|_| Self::stroke_with(path, width, t, COARSE))
    }

    fn stroke_with(
        path: &[PathCmd],
        width: u32,
        t: (i64, i64),
        steps: (i64, i64),
    ) -> Result<Self, TooComplex> {
        let mut s = Stroker {
            o: Self::new(),
            hw: i64::from(width) * SUB / 2,
            start: (0, 0),
            prev: (0, 0),
            first: None,
            last: None,
            closed: false,
        };
        walk(path, t, SUB / 2, steps, &mut |ev| match ev {
            Ev::Move(p) => {
                s.finish()?;
                (s.start, s.prev) = (p, p);
                (s.first, s.last, s.closed) = (None, None, false);
                Ok(())
            }
            Ev::Line(p) => s.line(p),
            Ev::Close => s.close(),
        })?;
        s.finish()?;
        Ok(s.o)
    }
}

struct Stroker {
    o: Outline,
    /// Half the width, in 1/16 px.
    hw: i64,
    start: P,
    prev: P,
    /// The left normals (length `hw`) of the first and the last segment of this sub-path.
    first: Option<P>,
    last: Option<P>,
    closed: bool,
}

/// The sine and cosine (x1024) of -90 to 90 degrees in steps of 30: a half circle.
const HALF: [(i64, i64); 7] = [
    (-1024, 0),
    (-887, 512),
    (-512, 887),
    (0, 1024),
    (512, 887),
    (887, 512),
    (1024, 0),
];

impl Stroker {
    /// The left normal of `a -> b`, of length `hw`.
    fn normal(&self, a: P, b: P) -> P {
        let (dx, dy) = (i128::from(b.0 - a.0), i128::from(b.1 - a.1));
        let len = isqrt((dx * dx + dy * dy) as u128) as i128;
        let hw = i128::from(self.hw);
        let round = |n: i128| (2 * n + len).div_euclid(2 * len);
        (round(-dy * hw) as i64, round(dx * hw) as i64)
    }

    fn line(&mut self, p: P) -> Result<(), TooComplex> {
        if p == self.prev || self.closed {
            return Ok(());
        }
        let (a, b) = (self.prev, p);
        let n = self.normal(a, b);
        let add = |u: P, v: P| (u.0 + v.0, u.1 + v.1);
        let sub = |u: P, v: P| (u.0 - v.0, u.1 - v.1);
        self.o
            .polygon(&[add(a, n), add(b, n), sub(b, n), sub(a, n)])?;
        if let Some(prev) = self.last {
            self.join(a, prev, n)?;
        } else {
            self.first = Some(n);
        }
        self.last = Some(n);
        self.prev = b;
        Ok(())
    }

    /// The outer side of a turn at `p` from normal `n0` to `n1`.
    fn join(&mut self, p: P, n0: P, n1: P) -> Result<(), TooComplex> {
        let cross = i128::from(n0.0) * i128::from(n1.1) - i128::from(n0.1) * i128::from(n1.0);
        let dot = i128::from(n0.0) * i128::from(n1.0) + i128::from(n0.1) * i128::from(n1.1);
        let hw2 = i128::from(self.hw) * i128::from(self.hw);
        if cross == 0 {
            if dot < 0 {
                // Straight back on itself: a round end.
                self.cap(p, n0)?;
            }
            return Ok(());
        }
        let s: i64 = if cross > 0 { -1 } else { 1 };
        let at = |n: P| (p.0 + s * n.0, p.1 + s * n.1);
        let (a, b) = (at(n0), at(n1));
        if a == b {
            return Ok(());
        }
        // 1000 * cos of the turn, and 1000 * (1 + cos).
        let cos = (dot * 1000 / hw2.max(1)) as i64;
        if (MITER_COS..GENTLE).contains(&cos) {
            // The miter tip: (n0 + n1) / (1 + cos), outward.
            let den = hw2 + dot;
            if den > 0 {
                let tip = (
                    p.0 + (i128::from(s) * i128::from(n0.0 + n1.0) * hw2 / den) as i64,
                    p.1 + (i128::from(s) * i128::from(n0.1 + n1.1) * hw2 / den) as i64,
                );
                return self.o.polygon(&[p, a, tip, b]);
            }
        }
        self.o.polygon(&[p, a, b])
    }

    /// A half circle at `p` bulging along the direction that `n` (a left normal) turns into: `(n.y, -n.x)`.
    fn cap(&mut self, p: P, n: P) -> Result<(), TooComplex> {
        // Direction (n.y, -n.x), at length hw; rotated through -90..90 degrees.
        let (ux, uy) = (n.1, -n.0);
        let mut pts = [(0i64, 0i64); 7];
        for (slot, &(sin, cos)) in pts.iter_mut().zip(&HALF) {
            // Rotate by the angle whose sine is `sin / 1024` and cosine `cos / 1024`.
            let x = (ux * cos - uy * sin) / 1024;
            let y = (ux * sin + uy * cos) / 1024;
            *slot = (p.0 + x, p.1 + y);
        }
        self.o.polygon(&pts)
    }

    fn close(&mut self) -> Result<(), TooComplex> {
        if self.last.is_some() {
            self.line(self.start)?;
            if let (Some(a), Some(b)) = (self.last, self.first) {
                self.join(self.start, a, b)?;
            }
        }
        self.closed = true;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), TooComplex> {
        if self.closed {
            return Ok(());
        }
        if let (Some(f), Some(l)) = (self.first, self.last) {
            // The start's cap bulges backwards: the direction of the first segment, reversed.
            self.cap(self.start, (-f.0, -f.1))?;
            self.cap(self.prev, l)?;
        }
        (self.first, self.last) = (None, None);
        Ok(())
    }
}

/// Adds the part of `[xa, xb)` (1/16 px) inside the chunk starting at column `cx` to `acc`, in 1/16 px of coverage.
fn add_span(acc: &mut [u8; CHUNK], cx: i64, xa: i64, xb: i64) {
    let lo = (xa - cx * SUB).max(0);
    let hi = (xb - cx * SUB).min(CHUNK as i64 * SUB);
    if hi <= lo {
        return;
    }
    let (pa, pb) = ((lo / SUB) as usize, (hi / SUB) as usize);
    if pa == pb {
        acc[pa] += (hi - lo) as u8;
        return;
    }
    acc[pa] += (SUB - lo % SUB) as u8;
    for v in &mut acc[pa + 1..pb] {
        *v += SUB as u8;
    }
    if pb < CHUNK {
        acc[pb] += (hi % SUB) as u8;
    }
}

/// Fills `outline` inside `[cx0, cx1) x [cy0, cy1)` (device pixels): calls `emit(y, x, alpha)` for each run of
/// pixels with some coverage, `alpha` 0 to 255 for each pixel from column `x`.
///
/// # Errors
/// [`TooComplex`] when a sample line crosses more edges than fixed storage holds.
pub(crate) fn rasterize(
    outline: &Outline,
    rule: Rule,
    (cx0, cy0, cx1, cy1): (i64, i64, i64, i64),
    mut emit: impl FnMut(i64, i64, &[u8]),
) -> Result<(), TooComplex> {
    let list = &outline.list[..outline.len];
    if list.is_empty() || cx1 <= cx0 || cy1 <= cy0 {
        return Ok(());
    }
    let (mut x_lo, mut x_hi, mut y_lo, mut y_hi) = (i64::MAX, i64::MIN, i64::MAX, i64::MIN);
    for e in list {
        x_lo = x_lo.min(i64::from(e.ax.min(e.bx)));
        x_hi = x_hi.max(i64::from(e.ax.max(e.bx)));
        y_lo = y_lo.min(i64::from(e.ay.min(e.by)));
        y_hi = y_hi.max(i64::from(e.ay.max(e.by)));
    }
    let (bx0, bx1) = (
        x_lo.div_euclid(SUB).max(cx0),
        (x_hi + SUB - 1).div_euclid(SUB).min(cx1),
    );
    let (by0, by1) = (
        y_lo.div_euclid(SUB).max(cy0),
        (y_hi + SUB - 1).div_euclid(SUB).min(cy1),
    );
    let mut out = [0u8; CHUNK];
    for y in by0..by1 {
        let mut chunk = bx0;
        while chunk < bx1 {
            let mut acc = [0u8; CHUNK];
            let mut any = false;
            for k in 0..SAMPLES {
                let sy = y * SUB + SUB / (2 * SAMPLES) + k * SUB / SAMPLES;
                let mut xs = [(0i64, 0i8); MAX_CROSSINGS];
                let mut n = 0;
                for e in list {
                    let (ay, by) = (i64::from(e.ay), i64::from(e.by));
                    let (lo, hi) = (ay.min(by), ay.max(by));
                    if sy < lo || sy >= hi {
                        continue;
                    }
                    let (ax, bx) = (i64::from(e.ax), i64::from(e.bx));
                    let (num, den) = if by > ay {
                        ((sy - ay) * (bx - ax), by - ay)
                    } else {
                        ((ay - sy) * (bx - ax), ay - by)
                    };
                    let x = ax + (2 * num + den).div_euclid(2 * den);
                    *xs.get_mut(n).ok_or(TooComplex)? = (x, if by > ay { 1 } else { -1 });
                    n += 1;
                }
                let row = &mut xs[..n];
                row.sort_unstable_by_key(|c| c.0);
                let mut wind = 0i32;
                for i in 0..n {
                    let before = wind;
                    wind = match rule {
                        Rule::NonZero => wind + i32::from(row[i].1),
                        Rule::EvenOdd => 1 - wind,
                    };
                    if before != 0 && i > 0 {
                        any = true;
                        add_span(&mut acc, chunk, row[i - 1].0, row[i].0);
                    }
                }
            }
            if any {
                let len = (bx1 - chunk).min(CHUNK as i64) as usize;
                for (o, &a) in out[..len].iter_mut().zip(&acc) {
                    *o = ((u32::from(a) * 255 + 32) / (SAMPLES as u32 * SUB as u32)).min(255) as u8;
                }
                emit(y, chunk, &out[..len]);
            }
            chunk += CHUNK as i64;
        }
    }
    Ok(())
}
