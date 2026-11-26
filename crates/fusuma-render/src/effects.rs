//! Gradients and shadows: what a renderer can add to flat colour. Every backend can draw them, because the trait has a default for
//! each that uses only rectangles; a backend that can do better (smooth, with real curves) overrides it, and says so in
//! [`Capabilities::effects`](crate::Capabilities).
//!
//! The defaults are meant to look right, not to be fast: a gradient is one fill for each row or column (at most [`MAX_BANDS`]), a
//! shadow is a few stacked, translucent rounded rectangles.

use crate::color::Color;
use crate::geometry::Rect;

/// The most strips a default gradient is cut into: a 2000 pixel gradient is drawn in 256 bands of about 8 pixels.
pub const MAX_BANDS: u32 = 256;

/// The most layers a default shadow is stacked from.
pub const MAX_SHADOW_LAYERS: u32 = 12;

/// Which way a gradient runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// From the top edge (`from`) to the bottom edge (`to`).
    #[default]
    Vertical,
    /// From the left edge (`from`) to the right edge (`to`).
    Horizontal,
}

/// A colour that changes in a straight line from one edge of a box to the opposite one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gradient {
    /// The colour at the start edge.
    pub from: Color,
    /// The colour at the end edge.
    pub to: Color,
    /// The way it runs.
    pub direction: Direction,
}

impl Gradient {
    /// A gradient from the top (`from`) to the bottom (`to`).
    #[must_use]
    pub const fn vertical(from: Color, to: Color) -> Self {
        Self {
            from,
            to,
            direction: Direction::Vertical,
        }
    }

    /// A gradient from the left (`from`) to the right (`to`).
    #[must_use]
    pub const fn horizontal(from: Color, to: Color) -> Self {
        Self {
            from,
            to,
            direction: Direction::Horizontal,
        }
    }

    /// The colour `k` steps along of `steps` (0 is `from`, `steps - 1` is `to`; one step is `from`), each channel mixed in
    /// integers and rounded.
    #[must_use]
    pub const fn at(&self, k: u32, steps: u32) -> Color {
        if steps <= 1 || k == 0 {
            return self.from;
        }
        if k >= steps - 1 {
            return self.to;
        }
        const fn ch(a: u8, b: u8, k: u32, n: u32) -> u8 {
            let (a, b) = (a as i64, b as i64);
            let d = n as i64;
            ((a * (d - k as i64) + b * k as i64 + d / 2) / d) as u8
        }
        let n = steps - 1;
        Color::rgba(
            ch(self.from.r, self.to.r, k, n),
            ch(self.from.g, self.to.g, k, n),
            ch(self.from.b, self.to.b, k, n),
            ch(self.from.a, self.to.a, k, n),
        )
    }
}

/// A soft shadow of a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shadow {
    /// Where the shadow is from the box: right and down are positive.
    pub offset: (i32, i32),
    /// How far the edge is blurred, in pixels (0 is a hard edge).
    pub blur: u32,
    /// How much bigger (positive) or smaller the shadow is than the box before it is blurred.
    pub spread: i32,
    /// The colour at the middle of the shadow; its alpha is how dark it gets.
    pub color: Color,
}

impl Shadow {
    /// A shadow `blur` pixels soft, `dy` pixels below the box, in black at `alpha`.
    #[must_use]
    pub const fn drop(blur: u32, dy: i32, alpha: u8) -> Self {
        Self {
            offset: (0, dy),
            blur,
            spread: 0,
            color: Color::rgba(0, 0, 0, alpha),
        }
    }
}

/// How far in from the edge a row `dy` pixels from the top of a box with corners of `radius` starts, when the corners are round:
/// 0 where the row is past the corner. The chord of the circle at the middle of the row, rounded to cover the pixels it touches.
#[must_use]
pub const fn corner_inset(radius: u32, dy: u32) -> u32 {
    if radius == 0 || dy >= radius {
        return 0;
    }
    // A corner this big is beyond any surface; the cap keeps the squares below inside u64.
    let (radius, dy) = if radius > 1 << 30 {
        (1 << 30, if dy < (1 << 30) { dy } else { (1 << 30) - 1 })
    } else {
        (radius, dy)
    };
    // The distance from the circle's centre to the middle of the row, doubled to stay in integers.
    let d2 = 2 * (radius as u64) - 2 * (dy as u64) - 1;
    let r2 = 2 * (radius as u64);
    let chord2 = r2 * r2 - d2 * d2;
    // Integer square root of chord2: the doubled half-chord.
    let mut lo = 0u64;
    let mut hi = r2;
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if mid * mid <= chord2 {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    // radius - half_chord, rounded up so that a pixel the circle only grazes is left out.
    let inset = (r2 - lo).div_ceil(2);
    if inset > radius as u64 {
        radius
    } else {
        inset as u32
    }
}

/// The strips a default gradient is drawn in, as `(strip, colour)` where the strip is a rectangle inside `rect` with the corners of
/// `radius` cut out of it. Calls `fill` for each.
pub fn bands<E>(
    rect: Rect,
    radius: u32,
    g: &Gradient,
    mut fill: impl FnMut(Rect, Color) -> Result<(), E>,
) -> Result<(), E> {
    if rect.is_empty() {
        return Ok(());
    }
    let r = radius.min(rect.w / 2).min(rect.h / 2);
    let (len, cross) = match g.direction {
        Direction::Vertical => (rect.h, rect.w),
        Direction::Horizontal => (rect.w, rect.h),
    };
    let strips = len.min(MAX_BANDS);
    for k in 0..strips {
        // Strip `k` covers the pixels from `a` to `b` along the gradient.
        let a = (u64::from(len) * u64::from(k) / u64::from(strips)) as u32;
        let b = (u64::from(len) * u64::from(k + 1) / u64::from(strips)) as u32;
        if b == a {
            continue;
        }
        // The colour of the middle of the strip.
        let colour = g.at(k, strips);
        // The corners cut by the circle: the strip is as wide as its most cut row.
        let inset = (a..b)
            .map(|p| corner_inset(r, p).max(corner_inset(r, len - 1 - p)))
            .max()
            .unwrap_or(0)
            .min(cross / 2);
        let strip = match g.direction {
            Direction::Vertical => Rect::new(
                rect.x.saturating_add(inset as i32),
                rect.y.saturating_add(a as i32),
                cross - 2 * inset,
                b - a,
            ),
            Direction::Horizontal => Rect::new(
                rect.x.saturating_add(a as i32),
                rect.y.saturating_add(inset as i32),
                b - a,
                cross - 2 * inset,
            ),
        };
        fill(strip, colour)?;
    }
    Ok(())
}

/// The layers of a default shadow of `rect` with corners of `radius`, from the biggest and faintest to the smallest and darkest:
/// `(rectangle, radius, colour)`. None when the shadow cannot be seen (no alpha, or an empty box).
pub fn shadow_layers<E>(
    rect: Rect,
    radius: u32,
    s: &Shadow,
    mut draw: impl FnMut(Rect, u32, Color) -> Result<(), E>,
) -> Result<(), E> {
    if rect.is_empty() || s.color.a == 0 {
        return Ok(());
    }
    let base = Rect::new(
        rect.x.saturating_add(s.offset.0),
        rect.y.saturating_add(s.offset.1),
        rect.w,
        rect.h,
    );
    let layers = s.blur.clamp(1, MAX_SHADOW_LAYERS);
    // Each layer adds a share of the colour, so the middle ends up near the colour's alpha.
    let alpha = (u32::from(s.color.a) * 2 / (layers + 1)).clamp(1, 255) as u8;
    for k in 0..layers {
        // The first layer reaches `blur` beyond the spread edge, the last one stops `blur / 2` inside it: the edge is soft on both
        // sides, as a blurred box is.
        let grow = i64::from(s.spread)
            + i64::from(s.blur) * i64::from(layers - k) / i64::from(layers)
            - i64::from(s.blur) / 2;
        let r = grown(base, grow);
        if r.is_empty() {
            continue;
        }
        let radius = (i64::from(radius) + grow).clamp(0, i64::from(u32::MAX)) as u32;
        draw(r, radius, s.color.with_alpha(alpha))?;
    }
    Ok(())
}

/// `r` made `by` pixels bigger on every side (smaller when negative), empty rather than inverted.
fn grown(r: Rect, by: i64) -> Rect {
    let w = i64::from(r.w) + 2 * by;
    let h = i64::from(r.h) + 2 * by;
    if w <= 0 || h <= 0 {
        return Rect::new(0, 0, 0, 0);
    }
    Rect::new(
        (i64::from(r.x) - by).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        (i64::from(r.y) - by).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        w.min(i64::from(u32::MAX)) as u32,
        h.min(i64::from(u32::MAX)) as u32,
    )
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;

    #[test]
    fn a_gradient_runs_from_one_colour_to_the_other_and_never_leaves_between() {
        let g = Gradient::vertical(Color::rgb(0, 0, 0), Color::rgb(255, 100, 50));
        assert_eq!(g.at(0, 11), Color::rgb(0, 0, 0));
        assert_eq!(g.at(10, 11), Color::rgb(255, 100, 50));
        assert_eq!(g.at(5, 11), Color::rgb(128, 50, 25));
        assert_eq!(g.at(7, 1), g.from, "one step is the start");
        assert_eq!(g.at(3, 0), g.from);
        assert_eq!(g.at(99, 5), g.to, "past the end is the end");
        for steps in [2u32, 3, 7, 256, 1000] {
            let mut last = g.at(0, steps);
            for k in 1..steps {
                let c = g.at(k, steps);
                assert!(
                    c.r >= last.r && c.g >= last.g && c.b >= last.b,
                    "monotone at {k} of {steps}"
                );
                last = c;
            }
        }
        // Alpha runs too.
        let fade = Gradient::horizontal(Color::rgba(9, 9, 9, 0), Color::rgba(9, 9, 9, 200));
        assert_eq!(
            (fade.at(0, 5).a, fade.at(2, 5).a, fade.at(4, 5).a),
            (0, 100, 200)
        );
    }

    #[test]
    fn the_corner_inset_is_zero_past_the_corner_and_the_radius_at_the_very_top() {
        assert_eq!(corner_inset(0, 0), 0);
        assert_eq!(corner_inset(8, 8), 0);
        assert_eq!(corner_inset(8, 100), 0);
        assert!(
            corner_inset(8, 0) >= 6 && corner_inset(8, 0) <= 8,
            "{}",
            corner_inset(8, 0)
        );
        // Getting less as the row goes down, never negative, never more than the radius.
        for r in [1u32, 2, 5, 16, 100] {
            let mut last = r;
            for dy in 0..r {
                let i = corner_inset(r, dy);
                assert!(i <= last && i <= r, "r {r} dy {dy}: {i}");
                last = i;
            }
            assert!(corner_inset(r, r - 1) <= 1 + r / 10, "r {r}");
        }
        // The huge ones do not overflow.
        let _ = corner_inset(u32::MAX, 0);
        let _ = corner_inset(u32::MAX, u32::MAX - 1);
    }

    #[test]
    fn bands_cover_the_box_exactly_once_in_order_and_cut_the_corners() {
        for (w, h, radius, dir) in [
            (40u32, 30u32, 0u32, Direction::Vertical),
            (40, 30, 10, Direction::Vertical),
            (40, 30, 10, Direction::Horizontal),
            (3, 500, 4, Direction::Vertical),
            (1, 1, 9, Direction::Horizontal),
            (2000, 7, 3, Direction::Horizontal),
        ] {
            let g = Gradient {
                from: Color::BLACK,
                to: Color::WHITE,
                direction: dir,
            };
            let rect = Rect::new(5, 7, w, h);
            let mut strips: Vec<(Rect, Color)> = Vec::new();
            bands::<()>(rect, radius, &g, |r, c| {
                strips.push((r, c));
                Ok(())
            })
            .unwrap();
            assert!(strips.len() as u32 <= MAX_BANDS, "{} strips", strips.len());
            assert!(!strips.is_empty());
            // In order along the gradient, edge to edge, no gap and no overlap.
            let mut next = match dir {
                Direction::Vertical => rect.y,
                Direction::Horizontal => rect.x,
            };
            for (s, _) in &strips {
                let start = match dir {
                    Direction::Vertical => s.y,
                    Direction::Horizontal => s.x,
                };
                assert_eq!(start, next, "{w}x{h} r{radius} {dir:?}");
                next = start
                    + match dir {
                        Direction::Vertical => s.h,
                        Direction::Horizontal => s.w,
                    } as i32;
                // Inside the box.
                assert!(
                    s.x >= rect.x
                        && s.y >= rect.y
                        && s.right() <= rect.right()
                        && s.bottom() <= rect.bottom()
                );
            }
            assert_eq!(
                next,
                match dir {
                    Direction::Vertical => rect.bottom(),
                    Direction::Horizontal => rect.right(),
                },
                "reaches the far edge"
            );
            // The colours run from the start to the end.
            assert_eq!(strips.first().unwrap().1, Color::BLACK);
            assert_eq!(
                strips.last().unwrap().1,
                if strips.len() == 1 {
                    Color::BLACK
                } else {
                    Color::WHITE
                }
            );
            // Round corners: the first strip is narrower than the box, a flat one is the whole box.
            if radius >= 2 && w.min(h) >= 6 {
                let first = strips[0].0;
                let across = match dir {
                    Direction::Vertical => (first.w, w),
                    Direction::Horizontal => (first.h, h),
                };
                assert!(
                    across.0 < across.1,
                    "{w}x{h} r{radius}: the first strip is cut"
                );
            }
        }
        // An empty box draws nothing.
        bands::<()>(
            Rect::new(0, 0, 0, 5),
            3,
            &Gradient::vertical(Color::BLACK, Color::WHITE),
            |_, _| panic!(),
        )
        .unwrap();
    }

    #[test]
    fn a_shadow_is_layers_from_the_widest_and_faintest_to_the_narrowest_and_darkest() {
        let rect = Rect::new(50, 50, 40, 30);
        let s = Shadow::drop(8, 4, 120);
        let mut layers: Vec<(Rect, u32, Color)> = Vec::new();
        shadow_layers::<()>(rect, 6, &s, |r, rad, c| {
            layers.push((r, rad, c));
            Ok(())
        })
        .unwrap();
        assert_eq!(layers.len(), 8);
        // Smaller and smaller, all around the same middle, the offset applied.
        for w in layers.windows(2) {
            assert!(w[1].0.w < w[0].0.w && w[1].0.h < w[0].0.h);
            assert!(w[1].1 <= w[0].1 + 1);
        }
        let mid = |r: Rect| (r.x + r.w as i32 / 2, r.y + r.h as i32 / 2);
        assert!(
            layers.iter().all(|l| mid(l.0) == (70, 69)),
            "{:?}",
            layers.iter().map(|l| mid(l.0)).collect::<Vec<_>>()
        );
        // The biggest reaches past the box by about the blur; the smallest is inside it.
        assert!(layers[0].0.w > rect.w + 4 && layers[7].0.w < rect.w);
        // Translucent, and the colour is the shadow's.
        assert!(
            layers
                .iter()
                .all(|l| l.2.a > 0 && l.2.a < 120 && (l.2.r, l.2.g, l.2.b) == (0, 0, 0))
        );
        // Stacked, the centre is about as dark as asked (within a third, since the layers add up in thirds of the rest).
        let mut a = 0.0f64;
        for l in &layers {
            a = a + (1.0 - a) * f64::from(l.2.a) / 255.0;
        }
        assert!(
            (a * 255.0 - 120.0).abs() < 40.0,
            "centre alpha {}",
            a * 255.0
        );
        // Nothing to draw: invisible colour, empty box.
        shadow_layers::<()>(rect, 6, &Shadow::drop(8, 4, 0), |_, _, _| panic!()).unwrap();
        shadow_layers::<()>(Rect::new(0, 0, 0, 9), 6, &s, |_, _, _| panic!()).unwrap();
        // A hard shadow (no blur) is one layer, as big as the box shifted.
        let mut one = Vec::new();
        shadow_layers::<()>(
            rect,
            0,
            &Shadow {
                offset: (3, 5),
                blur: 0,
                spread: 2,
                color: Color::rgba(0, 0, 0, 90),
            },
            |r, _, _| {
                one.push(r);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(one, [Rect::new(51, 53, 44, 34)]);
        // Absurd values do not panic.
        shadow_layers::<()>(
            Rect::new(i32::MAX, i32::MIN, u32::MAX, u32::MAX),
            u32::MAX,
            &Shadow {
                offset: (i32::MAX, i32::MIN),
                blur: u32::MAX,
                spread: i32::MIN,
                color: Color::BLACK,
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
    }
}
