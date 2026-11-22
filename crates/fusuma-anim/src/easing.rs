//! Easing curves: how progress from 0 to 1 is shaped.

use crate::Fx;

/// A CSS-style cubic Bezier curve through (0, 0) and (1, 1) with two control points. The x values
/// are limited to 0..=1, which keeps the curve a function of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bezier {
    x1: Fx,
    y1: Fx,
    x2: Fx,
    y2: Fx,
}

impl Bezier {
    /// A curve with control points `(x1, y1)` and `(x2, y2)`; the y values may leave 0..=1 for an
    /// overshoot.
    #[must_use]
    pub fn new(x1: Fx, y1: Fx, x2: Fx, y2: Fx) -> Self {
        Self {
            x1: x1.clamp01(),
            y1,
            x2: x2.clamp01(),
            y2,
        }
    }

    /// CSS `ease`: (0.25, 0.1, 0.25, 1).
    pub const EASE: Self = Self {
        x1: Fx::from_raw(16384),
        y1: Fx::from_raw(6554),
        x2: Fx::from_raw(16384),
        y2: Fx::ONE,
    };

    fn point(a: Fx, b: Fx, u: Fx) -> Fx {
        // 3(1-u)^2 u a + 3(1-u) u^2 b + u^3
        let v = Fx::ONE - u;
        let three = Fx::from_int(3);
        three * v * v * u * a + three * v * u * u * b + u * u * u
    }

    fn at(self, t: Fx) -> Fx {
        if t <= Fx::ZERO {
            return Fx::ZERO;
        }
        if t >= Fx::ONE {
            return Fx::ONE;
        }
        // x(u) rises with u, so find u by halving the interval; 17 steps reach the precision of Fx.
        let (mut lo, mut hi) = (Fx::ZERO, Fx::ONE);
        let mut u = Fx::HALF;
        for _ in 0..17 {
            if Self::point(self.x1, self.x2, u) < t {
                lo = u;
            } else {
                hi = u;
            }
            u = Fx::from_raw(((i64::from(lo.raw()) + i64::from(hi.raw())) / 2) as i32);
        }
        // y(u) has the same form with the y control points; the end points are fixed at 0 and 1.
        let v = Fx::ONE - u;
        let three = Fx::from_int(3);
        three * v * v * u * self.y1 + three * v * u * u * self.y2 + u * u * u
    }
}

/// An easing curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Starts slow, speeds up (t²).
    QuadIn,
    /// Starts fast, slows down.
    QuadOut,
    /// Slow at both ends.
    QuadInOut,
    /// Starts slow (t³).
    CubicIn,
    /// Starts fast, slows down: the usual choice for a slide.
    CubicOut,
    /// Slow at both ends.
    CubicInOut,
    /// Starts slow (t⁴).
    QuartIn,
    /// Starts fast, a long gentle stop.
    QuartOut,
    /// Slow at both ends.
    QuartInOut,
    /// `3t² - 2t³`: a smooth start and stop.
    Smooth,
    /// `6t⁵ - 15t⁴ + 10t³`: smoother still.
    Smoother,
    /// Pulls back a little before moving.
    BackIn,
    /// Overshoots the end a little and settles.
    BackOut,
    /// Pulls back, then overshoots.
    BackInOut,
    /// Bounces at the end like a dropped ball.
    BounceOut,
    /// Bounces at the start.
    BounceIn,
    /// Bounces at both ends.
    BounceInOut,
    /// A custom Bezier curve.
    Bezier(Bezier),
}

const BACK: i32 = 111_514; // 1.70158 * 65536

fn pow2(t: Fx) -> Fx {
    t * t
}

fn pow3(t: Fx) -> Fx {
    t * t * t
}

fn pow4(t: Fx) -> Fx {
    pow2(t) * pow2(t)
}

fn in_out(t: Fx, f_in: impl Fn(Fx) -> Fx) -> Fx {
    // The first half is the "in" curve at double speed and half height; the second is its mirror.
    if t < Fx::HALF {
        f_in(t * Fx::from_int(2)) / Fx::from_int(2)
    } else {
        Fx::ONE - f_in((Fx::ONE - t) * Fx::from_int(2)) / Fx::from_int(2)
    }
}

fn back_in(t: Fx, s: Fx) -> Fx {
    (s + Fx::ONE) * pow3(t) - s * pow2(t)
}

fn bounce_out(t: Fx) -> Fx {
    let k = Fx::from_ratio(75625, 10000);
    let part = |c: i32, d: i32, add: i32, add_d: i32| {
        let x = t - Fx::from_ratio(c, d);
        k * x * x + Fx::from_ratio(add, add_d)
    };
    if t < Fx::from_ratio(10, 27) {
        k * t * t
    } else if t < Fx::from_ratio(20, 27) {
        part(15, 27, 3, 4)
    } else if t < Fx::from_ratio(25, 27) {
        part(225, 270, 15, 16)
    } else {
        part(2625, 2750, 63, 64)
    }
}

impl Easing {
    /// The eased progress for `t` (limited to 0..=1). 0 maps to 0 and 1 to 1; `Back` and `Bezier`
    /// curves may leave 0..=1 in between.
    #[must_use]
    pub fn apply(self, t: Fx) -> Fx {
        let t = t.clamp01();
        let one = Fx::ONE;
        let s = Fx::from_raw(BACK);
        match self {
            Self::Linear => t,
            Self::QuadIn => pow2(t),
            Self::QuadOut => one - pow2(one - t),
            Self::QuadInOut => in_out(t, pow2),
            Self::CubicIn => pow3(t),
            Self::CubicOut => one - pow3(one - t),
            Self::CubicInOut => in_out(t, pow3),
            Self::QuartIn => pow4(t),
            Self::QuartOut => one - pow4(one - t),
            Self::QuartInOut => in_out(t, pow4),
            Self::Smooth => pow2(t) * (Fx::from_int(3) - Fx::from_int(2) * t),
            Self::Smoother => {
                pow3(t) * (t * (t * Fx::from_int(6) - Fx::from_int(15)) + Fx::from_int(10))
            }
            Self::BackIn => back_in(t, s),
            Self::BackOut => one - back_in(one - t, s),
            Self::BackInOut => in_out(t, |x| back_in(x, s * Fx::from_ratio(1525, 1000))),
            Self::BounceOut => bounce_out(t),
            Self::BounceIn => one - bounce_out(one - t),
            Self::BounceInOut => in_out(t, |x| one - bounce_out(one - x)),
            Self::Bezier(b) => b.at(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 19] = [
        Easing::Linear,
        Easing::QuadIn,
        Easing::QuadOut,
        Easing::QuadInOut,
        Easing::CubicIn,
        Easing::CubicOut,
        Easing::CubicInOut,
        Easing::QuartIn,
        Easing::QuartOut,
        Easing::QuartInOut,
        Easing::Smooth,
        Easing::Smoother,
        Easing::BackIn,
        Easing::BackOut,
        Easing::BackInOut,
        Easing::BounceOut,
        Easing::BounceIn,
        Easing::BounceInOut,
        Easing::Bezier(Bezier::EASE),
    ];

    fn near(a: Fx, b: f32, tol: f32) -> bool {
        (a.to_f32() - b).abs() <= tol
    }

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for e in ALL {
            assert!(
                near(e.apply(Fx::ZERO), 0.0, 0.002),
                "{e:?} at 0: {}",
                e.apply(Fx::ZERO).to_f32()
            );
            assert!(
                near(e.apply(Fx::ONE), 1.0, 0.002),
                "{e:?} at 1: {}",
                e.apply(Fx::ONE).to_f32()
            );
        }
    }

    #[test]
    fn input_outside_the_range_is_clamped() {
        for e in ALL {
            assert_eq!(e.apply(Fx::from_int(-3)), e.apply(Fx::ZERO));
            assert_eq!(e.apply(Fx::from_int(9)), e.apply(Fx::ONE));
            assert_eq!(e.apply(Fx::MIN), e.apply(Fx::ZERO));
            assert_eq!(e.apply(Fx::MAX), e.apply(Fx::ONE));
        }
    }

    #[test]
    fn curves_without_overshoot_never_go_backwards_or_leave_the_range() {
        let plain = [
            Easing::Linear,
            Easing::QuadIn,
            Easing::QuadOut,
            Easing::QuadInOut,
            Easing::CubicIn,
            Easing::CubicOut,
            Easing::CubicInOut,
            Easing::QuartIn,
            Easing::QuartOut,
            Easing::QuartInOut,
            Easing::Smooth,
            Easing::Smoother,
            Easing::Bezier(Bezier::EASE),
        ];
        for e in plain {
            let mut last = Fx::ZERO;
            for i in 0..=1000 {
                let v = e.apply(Fx::from_ratio(i, 1000));
                assert!(
                    v >= last - Fx::from_raw(4),
                    "{e:?} goes backwards at {i}/1000"
                );
                assert!(
                    v >= Fx::from_raw(-4) && v <= Fx::ONE + Fx::from_raw(4),
                    "{e:?} leaves 0..1 at {i}/1000"
                );
                last = v;
            }
        }
    }

    #[test]
    fn known_values() {
        assert!(near(Easing::QuadIn.apply(Fx::HALF), 0.25, 0.001));
        assert!(near(Easing::QuadOut.apply(Fx::HALF), 0.75, 0.001));
        assert!(near(Easing::CubicIn.apply(Fx::HALF), 0.125, 0.001));
        assert!(near(Easing::CubicOut.apply(Fx::HALF), 0.875, 0.001));
        assert!(near(Easing::Smooth.apply(Fx::HALF), 0.5, 0.001));
        assert!(near(
            Easing::Smoother.apply(Fx::from_ratio(1, 4)),
            0.103516,
            0.002
        ));
        for e in [
            Easing::QuadInOut,
            Easing::CubicInOut,
            Easing::QuartInOut,
            Easing::Smooth,
            Easing::Smoother,
            Easing::BackInOut,
            Easing::BounceInOut,
        ] {
            assert!(
                near(e.apply(Fx::HALF), 0.5, 0.01),
                "{e:?} is symmetric around the middle: {}",
                e.apply(Fx::HALF).to_f32()
            );
        }
    }

    #[test]
    fn out_curves_lead_in_curves() {
        for i in 1..10 {
            let t = Fx::from_ratio(i, 10);
            assert!(Easing::CubicOut.apply(t) > Easing::Linear.apply(t));
            assert!(Easing::CubicIn.apply(t) < Easing::Linear.apply(t));
        }
    }

    #[test]
    fn back_curves_overshoot_and_bounce_curves_bounce() {
        let peak = (0..=100)
            .map(|i| Easing::BackOut.apply(Fx::from_ratio(i, 100)))
            .max()
            .unwrap();
        assert!(
            peak.to_f32() > 1.05 && peak.to_f32() < 1.15,
            "back out peaks at {}",
            peak.to_f32()
        );
        let dip = (0..=100)
            .map(|i| Easing::BackIn.apply(Fx::from_ratio(i, 100)))
            .min()
            .unwrap();
        assert!(
            dip.to_f32() < -0.05 && dip.to_f32() > -0.15,
            "back in dips to {}",
            dip.to_f32()
        );
        // A bounce touches 1 several times: count the times the curve comes back down.
        let mut dips = 0;
        let mut last = Fx::ZERO;
        let mut falling = false;
        for i in 0..=1000 {
            let v = Easing::BounceOut.apply(Fx::from_ratio(i, 1000));
            if v < last - Fx::from_raw(8) && !falling {
                dips += 1;
                falling = true;
            }
            if v > last {
                falling = false;
            }
            last = v;
        }
        assert!(dips >= 3, "a ball bounces more than twice, got {dips}");
        assert!(Easing::BounceOut.apply(Fx::from_ratio(999, 1000)) <= Fx::ONE);
    }

    #[test]
    fn bezier_matches_the_css_ease_curve_and_the_identity_curve() {
        // CSS ease at the middle is about 0.8025.
        assert!(near(
            Easing::Bezier(Bezier::EASE).apply(Fx::HALF),
            0.8025,
            0.01
        ));
        let linear = Bezier::new(Fx::ZERO, Fx::ZERO, Fx::ONE, Fx::ONE);
        for i in 0..=10 {
            let t = Fx::from_ratio(i, 10);
            assert!(
                near(Easing::Bezier(linear).apply(t), t.to_f32(), 0.01),
                "{i}"
            );
        }
        // The x control points are limited, so the curve stays a function of time.
        let wild = Bezier::new(
            Fx::from_int(-4),
            Fx::from_int(9),
            Fx::from_int(7),
            Fx::from_int(-9),
        );
        let _ = Easing::Bezier(wild).apply(Fx::HALF);
        assert_eq!(wild.x1, Fx::ZERO);
        assert_eq!(wild.x2, Fx::ONE);
        // A y above 1 overshoots.
        let over = Bezier::new(
            Fx::from_ratio(1, 4),
            Fx::from_ratio(3, 2),
            Fx::from_ratio(3, 4),
            Fx::from_ratio(3, 2),
        );
        assert!((0..=100).any(|i| Easing::Bezier(over).apply(Fx::from_ratio(i, 100)) > Fx::ONE));
    }

    #[test]
    fn no_curve_panics_on_any_raw_input() {
        for e in ALL {
            for raw in [
                i32::MIN,
                -65537,
                -1,
                0,
                1,
                32768,
                65535,
                65536,
                65537,
                i32::MAX,
            ] {
                let _ = e.apply(Fx::from_raw(raw));
            }
        }
    }
}
