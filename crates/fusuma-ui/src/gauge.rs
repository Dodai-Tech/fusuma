//! A gauge: an arc of 270 degrees that fills with a value, a number in the middle. Drawn as a stroked path, with the
//! sine from a table so that no float, no libm and no allocation is needed (the path is a short array on the stack).

use fusuma_render::{Color, PathCmd, Point, Rect, Renderer};

/// `sin` of whole degrees 0 to 90, scaled by 32768.
const SIN: [i32; 91] = [
    0, 572, 1144, 1715, 2286, 2856, 3425, 3993, 4560, 5126, 5690, 6252, 6813, 7371, 7927, 8481,
    9032, 9580, 10126, 10668, 11207, 11743, 12275, 12803, 13328, 13848, 14365, 14876, 15384, 15886,
    16384, 16877, 17364, 17847, 18324, 18795, 19261, 19720, 20174, 20622, 21063, 21498, 21926,
    22348, 22763, 23170, 23571, 23965, 24351, 24730, 25102, 25466, 25822, 26170, 26510, 26842,
    27166, 27482, 27789, 28088, 28378, 28660, 28932, 29197, 29452, 29698, 29935, 30163, 30382,
    30592, 30792, 30983, 31164, 31336, 31499, 31651, 31795, 31928, 32052, 32166, 32270, 32365,
    32449, 32524, 32588, 32643, 32688, 32723, 32748, 32763, 32768,
];

/// The sine of `tenths` of a degree (any integer), scaled by 32768, interpolated between the table's degrees.
#[must_use]
pub const fn sin_q15(tenths: i32) -> i32 {
    let t = tenths.rem_euclid(3600);
    // Fold into the first quadrant.
    let (t, neg) = if t >= 1800 {
        (t - 1800, true)
    } else {
        (t, false)
    };
    let t = if t > 900 { 1800 - t } else { t };
    let (d, f) = ((t / 10) as usize, t % 10);
    let a = SIN[d];
    let b = if d < 90 { SIN[d + 1] } else { SIN[90] };
    let v = a + (b - a) * f / 10;
    if neg { -v } else { v }
}

/// The cosine, the same way.
#[must_use]
pub const fn cos_q15(tenths: i32) -> i32 {
    sin_q15(tenths + 900)
}

/// Where the gauge starts (bottom left) and how far it goes round (to the bottom right), in tenths of a degree, clockwise
/// from three o'clock on a screen whose y grows downward.
pub const START: i32 = 1350;
/// The whole sweep of the arc.
pub const SWEEP: i32 = 2700;

/// The most points an arc is made of: a point every 15 degrees, and one for the end. A chord of 15 degrees is within 0.9 percent
/// of the radius of the arc (a third of a pixel on a watch face), and the integer renderer holds only so many edges in a
/// stroked path (160, about 20 segments): more would not draw at all.
const POINTS: usize = 19;

/// Tenths of a degree between two points.
const STEP: i32 = 150;

/// The point of the arc at `tenths` degrees on a circle of `radius` around `c`.
#[must_use]
fn at(c: Point, radius: i32, tenths: i32) -> Point {
    let x = i64::from(radius) * i64::from(cos_q15(tenths)) / 32768;
    let y = i64::from(radius) * i64::from(sin_q15(tenths)) / 32768;
    Point::new(c.x + x as i32, c.y + y as i32)
}

/// Strokes the arc from the start for `sweep` tenths of a degree (0 to [`SWEEP`]) in `rect`, `width` pixels thick.
///
/// # Errors
/// The renderer's.
pub fn stroke_arc<R: Renderer>(
    r: &mut R,
    rect: Rect,
    width: u32,
    sweep: i32,
    color: Color,
) -> Result<(), R::Error> {
    stroke_arc_from(r, rect, width, START, sweep.clamp(0, SWEEP), color)
}

/// A whole ring, as two half arcs (a path is one polyline, and 19 points go round a ring in 20 degree steps).
///
/// # Errors
/// The renderer's.
pub fn stroke_ring<R: Renderer>(
    r: &mut R,
    rect: Rect,
    width: u32,
    color: Color,
) -> Result<(), R::Error> {
    stroke_arc_from(r, rect, width, 0, 1800, color)?;
    stroke_arc_from(r, rect, width, 1800, 1800, color)
}

/// Strokes `sweep` tenths of a degree (up to 3600) of a ring in `rect`, from `start` tenths, `width` pixels thick.
///
/// # Errors
/// The renderer's.
pub fn stroke_arc_from<R: Renderer>(
    r: &mut R,
    rect: Rect,
    width: u32,
    start: i32,
    sweep: i32,
    color: Color,
) -> Result<(), R::Error> {
    let sweep = sweep.clamp(0, 3600);
    if sweep == 0 {
        return Ok(());
    }
    let side = rect.w.min(rect.h) as i32;
    let radius = (side - width as i32) / 2;
    if radius <= 0 {
        return Ok(());
    }
    let c = Point::new(rect.x + rect.w as i32 / 2, rect.y + rect.h as i32 / 2);
    let mut path = [PathCmd::Close; POINTS];
    let steps = ((sweep + STEP - 1) / STEP).clamp(1, (POINTS - 1) as i32);
    let mut n = 0;
    for k in 0..=steps {
        let a = start + sweep * k / steps;
        let p = at(c, radius, a);
        path[n] = if k == 0 {
            PathCmd::MoveTo(p)
        } else {
            PathCmd::LineTo(p)
        };
        n += 1;
    }
    r.stroke_path(&path[..n], width, color)
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::theme::Theme;
    use crate::tree::{Length, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    #[test]
    fn the_sine_table_is_close_to_the_real_one_and_has_the_symmetries() {
        assert_eq!(sin_q15(0), 0);
        assert_eq!(sin_q15(900), 32768);
        assert_eq!(sin_q15(1800), 0);
        assert_eq!(sin_q15(2700), -32768);
        assert_eq!(sin_q15(3600), 0);
        assert_eq!(cos_q15(0), 32768);
        assert_eq!(cos_q15(1800), -32768);
        assert_eq!(cos_q15(900), 0);
        // Every tenth of a degree, against f64: well under a pixel on a radius of 1000 (3 in 32768 is 0.1 px).
        for t in -3700..=3700 {
            let exact = (f64::from(t) / 10.0).to_radians().sin() * 32768.0;
            assert!(
                (f64::from(sin_q15(t)) - exact).abs() < 12.0,
                "{t}: {} against {exact:.1}",
                sin_q15(t)
            );
        }
        // sin(a)^2 + cos(a)^2 = 1 within the table's error.
        for t in (0..3600).step_by(37) {
            let (s, c) = (i64::from(sin_q15(t)), i64::from(cos_q15(t)));
            let sq = (s * s + c * c) as f64 / (32768.0 * 32768.0);
            assert!((sq - 1.0).abs() < 0.001, "{t}: {sq}");
        }
    }

    fn drawn(value: u16) -> (VelloCpu, Rect) {
        let mut t = Tree::<4>::new();
        let root = t
            .column(None, Style::new().padding(10).width(Length::Fill(1)))
            .unwrap();
        let g = t.gauge(Some(root), value, "").unwrap();
        let mut r = fusuma_render_vello_cpu::new(120, 120).unwrap();
        t.update(&r, &THEME, Rect::new(0, 0, 120, 120));
        r.clear(THEME.colors.background).unwrap();
        t.paint(&mut r, &THEME).unwrap();
        let rect = t.rect(g).unwrap();
        (r, rect)
    }

    #[test]
    fn the_arc_fills_clockwise_from_the_bottom_left_and_leaves_the_bottom_open() {
        let c = THEME.colors;
        // The ring's left, top and right points, and the gap at the bottom.
        let probe = |r: &VelloCpu, g: Rect| {
            let (cx, cy) = (g.x + g.w as i32 / 2, g.y + g.h as i32 / 2);
            let radius = g.w.min(g.h) as i32 / 2 - 4;
            [
                r.pixel(cx - radius, cy).unwrap(),
                r.pixel(cx, cy - radius).unwrap(),
                r.pixel(cx + radius, cy).unwrap(),
                r.pixel(cx, cy + radius).unwrap(),
            ]
        };
        let (r, g) = drawn(0);
        let [left, top, right, bottom] = probe(&r, g);
        assert_eq!(
            [left, top, right],
            [c.surface_raised; 3],
            "an empty gauge is all track"
        );
        assert_eq!(bottom, c.background, "the bottom of the ring is open");
        let (r, g) = drawn(500);
        let [left, top, right, _] = probe(&r, g);
        assert_eq!(left, c.primary, "half way is the left side up to the top");
        assert_eq!(top, c.primary);
        assert_eq!(
            right, c.surface_raised,
            "and the right side is still to come"
        );
        let (r, g) = drawn(1000);
        let [left, top, right, bottom] = probe(&r, g);
        assert_eq!([left, top, right], [c.primary; 3]);
        assert_eq!(bottom, c.background);
        // More than full is full.
        let mut t = Tree::<4>::new();
        let g = t.gauge(None, 9999, "").unwrap();
        assert_eq!(t.gauge_value(g), Some(1000));
    }

    #[test]
    fn a_gauge_repaints_when_its_value_changes_and_not_when_it_does_not() {
        let mut t = Tree::<4>::new();
        let g = t.gauge(None, 100, "12").unwrap();
        let mut r = fusuma_render_vello_cpu::new(120, 120).unwrap();
        let b = Rect::new(0, 0, 120, 120);
        t.update(&r, &THEME, b);
        t.paint_frame(&mut r, &THEME, b).unwrap();
        assert!(!t.needs_frame());
        t.set_gauge(g, 100);
        assert!(!t.needs_frame(), "the same value is a no-op");
        t.set_gauge(g, 700);
        assert!(t.needs_frame());
        assert_eq!(t.gauge_value(g), Some(700));
        t.update(&r, &THEME, b);
        assert!(t.paint_frame(&mut r, &THEME, b).unwrap().is_some());
        t.set_text(g, "34");
        assert_eq!(t.text(g), Some("34"));
        assert!(t.needs_frame());
        // Not a gauge: ignored.
        let label = Tree::<2>::new().gauge_value(g);
        assert_eq!(label, None);
    }

    #[test]
    fn an_arc_of_any_value_never_overflows_its_path_or_panics() {
        let mut r = fusuma_render_vello_cpu::new(60, 60).unwrap();
        for sweep in [
            -5,
            0,
            1,
            49,
            50,
            51,
            1350,
            2699,
            2700,
            2701,
            i32::MAX,
            i32::MIN,
        ] {
            for w in [0, 1, 2, 7] {
                stroke_arc(
                    &mut r,
                    Rect::new(0, 0, 60, 60),
                    w,
                    sweep,
                    THEME.colors.primary,
                )
                .unwrap();
            }
        }
        // A degenerate box draws nothing and does not fail.
        stroke_arc(&mut r, Rect::new(0, 0, 0, 0), 4, 1000, THEME.colors.primary).unwrap();
        stroke_arc(&mut r, Rect::new(5, 5, 3, 3), 8, 1000, THEME.colors.primary).unwrap();
    }

    #[test]
    fn a_spinner_turns_while_shown_stands_still_without_motion_and_hidden_ones_do_not_spin() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let a = t.spinner(Some(root)).unwrap();
        let b = t.spinner(Some(root)).unwrap();
        let mut r = fusuma_render_vello_cpu::new(120, 120).unwrap();
        let bounds = Rect::new(0, 0, 120, 120);
        t.update(&r, &THEME, bounds);
        t.paint_frame(&mut r, &THEME, bounds).unwrap();
        assert_eq!(t.spinner_phase(a), Some(0));
        // A quarter of a second is 90 degrees: the caller is told to go on drawing frames.
        assert!(t.advance(&THEME, 250));
        assert_eq!(t.spinner_phase(a), Some(900));
        assert!(t.needs_frame() || t.paint_frame(&mut r, &THEME, bounds).unwrap().is_some());
        // A full turn wraps, and a stalled frame (a long pause) turns at most a second's worth.
        assert!(t.advance(&THEME, 800));
        assert_eq!(t.spinner_phase(a), Some(3600 % 3600 + (900 + 2880) % 3600));
        t.advance(&THEME, 60_000);
        assert!(t.spinner_phase(a).unwrap() < 3600);
        // Hidden: no turning, and so no frames asked for.
        t.set_hidden(b, true);
        t.set_hidden(a, true);
        t.update(&r, &THEME, bounds);
        let before = t.spinner_phase(b);
        assert!(!t.advance(&THEME, 100), "nothing shown turns");
        assert_eq!(t.spinner_phase(b), before);
        t.set_hidden(a, false);
        t.update(&r, &THEME, bounds);
        assert!(t.advance(&THEME, 100));
        // No motion (e-paper): it stands still and asks for nothing.
        let still = THEME.with_motion(crate::theme::MotionBudget::Off);
        let at = t.spinner_phase(a);
        assert!(!t.advance(&still, 500));
        assert_eq!(t.spinner_phase(a), at);
    }

    #[test]
    fn a_spinner_paints_a_quarter_ring_that_moves_with_its_phase_inside_its_box() {
        let c = THEME.colors;
        let pic = |phase_ms: u32| {
            let mut t = Tree::<4>::new();
            let root = t.column(None, Style::new().padding(10)).unwrap();
            let s = t.spinner(Some(root)).unwrap();
            let mut r = fusuma_render_vello_cpu::new(120, 120).unwrap();
            t.update(&r, &THEME, Rect::new(0, 0, 120, 120));
            t.advance(&THEME, phase_ms);
            t.update(&r, &THEME, Rect::new(0, 0, 120, 120));
            r.clear(c.background).unwrap();
            t.paint(&mut r, &THEME).unwrap();
            let g = t.rect(s).unwrap();
            let primary: std::vec::Vec<(i32, i32)> = (g.x..g.right())
                .flat_map(|x| (g.y..g.bottom()).map(move |y| (x, y)))
                .filter(|&(x, y)| r.pixel(x, y) == Some(c.primary))
                .collect();
            (g, primary)
        };
        let (g, a) = pic(0);
        let (_, b) = pic(500);
        assert!(!a.is_empty() && !b.is_empty());
        // About a quarter of the ring is lit (the whole ring would be four times as many).
        let (_, quarter) = pic(0);
        assert!(
            quarter.len() < (g.w * g.h) as usize / 4,
            "{} of {}",
            quarter.len(),
            g.w * g.h
        );
        assert_ne!(a, b, "half a second later the lit part is somewhere else");
        // The lit part is inside the box, and the centroid is on the other side after half a turn.
        let centroid = |p: &[(i32, i32)]| {
            let n = p.len() as i64;
            (
                p.iter().map(|q| i64::from(q.0)).sum::<i64>() / n,
                p.iter().map(|q| i64::from(q.1)).sum::<i64>() / n,
            )
        };
        let (ca, cb) = (centroid(&a), centroid(&b));
        let mid = (
            i64::from(g.x) + i64::from(g.w) / 2,
            i64::from(g.y) + i64::from(g.h) / 2,
        );
        assert!(
            ((ca.0 - mid.0) * (cb.0 - mid.0) + (ca.1 - mid.1) * (cb.1 - mid.1)) < 0,
            "opposite sides: {ca:?} {cb:?} {mid:?}"
        );
        assert!(
            a.iter()
                .chain(&b)
                .all(|&(x, y)| g.contains(fusuma_render::Point::new(x, y)))
        );
    }
}
