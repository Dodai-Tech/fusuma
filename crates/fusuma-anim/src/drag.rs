//! Helpers for dragging: rubber-band overscroll and snapping to pages.

use crate::Fx;

/// How far a position may be pulled past an edge before it stops moving, as a fraction of the
/// visible size: the curve approaches this and never reaches it.
const RESISTANCE: Fx = Fx::from_raw(36044); // 0.55

/// A position that resists being pulled past `min` or `max`: inside the range it is unchanged;
/// outside, the distance past the edge is compressed, so a long pull moves ever less, like the
/// rubber band of a scroll view. `size` is the visible length, which sets how far it can stretch
/// (a pull never goes past `size`).
#[must_use]
pub fn rubber_band(position: Fx, min: Fx, max: Fx, size: Fx) -> Fx {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let past = |d: Fx| -> Fx {
        // d past the edge -> (1 - 1 / (d * 0.55 / size + 1)) * size
        if size <= Fx::ZERO || d <= Fx::ZERO {
            return Fx::ZERO;
        }
        let x = d * RESISTANCE / size;
        (Fx::ONE - Fx::ONE / (x + Fx::ONE)) * size
    };
    if position < lo {
        lo - past(lo - position)
    } else if position > hi {
        hi + past(position - hi)
    } else {
        position
    }
}

/// The page that a flick should settle on: where the motion would stop (`position` plus
/// `velocity / friction`), rounded to the nearest page, but never more than one page from the
/// `current` one, and always inside `0..pages`. Positions are in pixels, page `i` sits at
/// `i * page_size`.
#[must_use]
pub fn snap_page(
    position: Fx,
    velocity: Fx,
    friction: Fx,
    page_size: Fx,
    pages: u32,
    current: u32,
) -> u32 {
    if pages == 0 || page_size <= Fx::ZERO {
        return 0;
    }
    let last = pages - 1;
    let current = current.min(last);
    let friction = friction.max(Fx::from_ratio(1, 10));
    let end = position + velocity / friction;
    // Nearest page index, as a whole number (can be negative or past the end).
    let page = (end / page_size).round();
    let lo = i64::from(current) - 1;
    let hi = i64::from(current) + 1;
    i64::from(page).clamp(lo.max(0), hi.min(i64::from(last))) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Fx = Fx::from_int(400);

    #[test]
    fn inside_the_range_nothing_changes() {
        for p in [0, 1, 250, 999, 1000] {
            let v = Fx::from_int(p);
            assert_eq!(rubber_band(v, Fx::ZERO, Fx::from_int(1000), SIZE), v);
        }
    }

    #[test]
    fn past_an_edge_the_pull_is_compressed_and_bounded() {
        let (lo, hi) = (Fx::ZERO, Fx::from_int(1000));
        let at = |pulled: i32| rubber_band(hi + Fx::from_int(pulled), lo, hi, SIZE) - hi;
        let (a, b, c) = (at(50), at(200), at(2000));
        assert!(
            a > Fx::ZERO && a < Fx::from_int(50),
            "compressed: {}",
            a.to_f32()
        );
        assert!(b > a && c > b, "still grows");
        assert!(c < SIZE, "never reaches the visible size: {}", c.to_f32());
        // The same on the low side, mirrored.
        let low = |pulled: i32| lo - rubber_band(lo - Fx::from_int(pulled), lo, hi, SIZE);
        assert_eq!(low(200), b);
    }

    #[test]
    fn the_first_pixels_follow_at_about_the_resistance_ratio() {
        let hi = Fx::from_int(100);
        let moved = rubber_band(hi + Fx::from_int(2), Fx::ZERO, hi, SIZE) - hi;
        assert!(
            (moved.to_f32() - 1.1).abs() < 0.05,
            "55% of 2 px: {}",
            moved.to_f32()
        );
    }

    #[test]
    fn degenerate_ranges_and_sizes_do_not_panic() {
        assert_eq!(
            rubber_band(Fx::from_int(5), Fx::from_int(10), Fx::ZERO, SIZE),
            Fx::from_int(5),
            "swapped bounds"
        );
        assert_eq!(
            rubber_band(Fx::from_int(50), Fx::ZERO, Fx::from_int(10), Fx::ZERO),
            Fx::from_int(10),
            "no size: a hard stop"
        );
        assert_eq!(
            rubber_band(
                Fx::from_int(50),
                Fx::ZERO,
                Fx::from_int(10),
                Fx::from_int(-3)
            ),
            Fx::from_int(10)
        );
        for p in [Fx::MIN, Fx::MAX, Fx::ZERO] {
            let _ = rubber_band(p, Fx::MIN, Fx::MAX, Fx::MAX);
            let _ = rubber_band(p, Fx::MAX, Fx::MIN, Fx::MIN);
            let _ = rubber_band(p, Fx::ZERO, Fx::ZERO, SIZE);
        }
    }

    #[test]
    fn a_gentle_drag_snaps_to_the_nearest_page_and_a_flick_moves_one() {
        let page = Fx::from_int(400);
        let friction = Fx::from_int(4);
        // Released at rest 30% across: back to page 0; 60% across: on to page 1.
        assert_eq!(
            snap_page(Fx::from_int(120), Fx::ZERO, friction, page, 5, 0),
            0
        );
        assert_eq!(
            snap_page(Fx::from_int(240), Fx::ZERO, friction, page, 5, 0),
            1
        );
        // A short drag with a fast flick carries on to the next page.
        assert_eq!(
            snap_page(Fx::from_int(60), Fx::from_int(1200), friction, page, 5, 0),
            1
        );
        // A flick the other way from page 2.
        assert_eq!(
            snap_page(Fx::from_int(780), Fx::from_int(-1200), friction, page, 5, 2),
            1
        );
    }

    #[test]
    fn a_flick_never_skips_pages_or_leaves_the_range() {
        let page = Fx::from_int(400);
        assert_eq!(
            snap_page(
                Fx::from_int(10),
                Fx::from_int(30_000),
                Fx::from_int(4),
                page,
                5,
                0
            ),
            1,
            "at most one page"
        );
        assert_eq!(
            snap_page(Fx::ZERO, Fx::from_int(-30_000), Fx::from_int(4), page, 5, 0),
            0
        );
        assert_eq!(
            snap_page(
                Fx::from_int(1600),
                Fx::from_int(30_000),
                Fx::from_int(4),
                page,
                5,
                4
            ),
            4
        );
        assert_eq!(
            snap_page(Fx::from_int(5), Fx::ZERO, Fx::from_int(4), page, 1, 0),
            0,
            "one page"
        );
    }

    #[test]
    fn degenerate_input_is_safe() {
        let page = Fx::from_int(400);
        assert_eq!(snap_page(Fx::ZERO, Fx::ZERO, Fx::ONE, page, 0, 0), 0);
        assert_eq!(snap_page(Fx::ZERO, Fx::ZERO, Fx::ONE, Fx::ZERO, 5, 3), 0);
        assert_eq!(
            snap_page(Fx::ZERO, Fx::ZERO, Fx::ONE, Fx::from_int(-5), 5, 3),
            0
        );
        assert!(
            snap_page(Fx::MAX, Fx::MAX, Fx::ZERO, page, 5, 99) <= 4,
            "current is clamped too"
        );
        let _ = snap_page(
            Fx::MIN,
            Fx::MIN,
            Fx::MIN,
            Fx::from_raw(1),
            u32::MAX,
            u32::MAX,
        );
    }
}
