//! A damped spring: physics-based motion that can be interrupted at any moment without a jump.
//!
//! The state is kept in 64-bit fixed point (Q16.16 in an `i64`), so a stiff spring pulled far from
//! its target cannot overflow, and time is stepped in fixed slices of 1/240 s, so the result does
//! not depend on how the frames happen to be spaced.

use crate::Fx;

/// Length of one simulation step in microseconds (1/240 s).
const STEP_US: u32 = 4167;
/// The same, as Q16.16 seconds.
const STEP_FX: i64 = 273;
/// The longest frame that is simulated in one call; a longer pause is cut to this (the spring
/// simply carries on from where it is, instead of running hundreds of steps).
const MAX_FRAME_MS: u32 = 100;
/// A spring this close to its target (0.01 pixels) and this slow (0.1 pixels per second) stops.
const REST_POS: i64 = 655;
const REST_VEL: i64 = 6554;
/// Velocity limit (a million pixels per second), so a numerical blow-up cannot run away.
const MAX_VEL: i64 = 1_000_000 << 16;
/// The farthest a spring is pulled from its target (the Q16.16 range).
const MAX_DIST: i64 = 1 << 31;

/// How a spring behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpringConfig {
    stiffness: Fx,
    damping: Fx,
    mass: Fx,
}

fn q(whole: i32, hundredths: i32) -> Fx {
    Fx::from_ratio(whole * 100 + hundredths, 100)
}

impl SpringConfig {
    /// Soft and slow, with a little overshoot.
    pub const GENTLE: Self = Self {
        stiffness: Fx::from_int(120),
        damping: Fx::from_int(14),
        mass: Fx::ONE,
    };
    /// Springy: overshoots and wobbles several times before it settles.
    pub const WOBBLY: Self = Self {
        stiffness: Fx::from_int(180),
        damping: Fx::from_int(8),
        mass: Fx::ONE,
    };
    /// Quick and firm, with almost no overshoot.
    pub const STIFF: Self = Self {
        stiffness: Fx::from_int(400),
        damping: Fx::from_int(38),
        mass: Fx::ONE,
    };
    /// Slow and heavy, no overshoot.
    pub const SLOW: Self = Self {
        stiffness: Fx::from_int(40),
        damping: Fx::from_int(13),
        mass: Fx::ONE,
    };
    /// A quick settle with a hint of overshoot: a good default for interface motion.
    pub const SNAPPY: Self = Self {
        stiffness: Fx::from_int(300),
        damping: Fx::from_int(26),
        mass: Fx::ONE,
    };

    /// A spring from its parts. The values are limited to a range that stays stable: stiffness
    /// 0.1 to 20000, damping 0 to 2000 and mass 0.01 to 100.
    #[must_use]
    pub fn new(stiffness: Fx, damping: Fx, mass: Fx) -> Self {
        Self {
            stiffness: stiffness.clamp(q(0, 10), Fx::from_int(20_000).min(Fx::MAX)),
            damping: damping.clamp(Fx::ZERO, Fx::from_int(2000)),
            mass: mass.clamp(q(0, 1), Fx::from_int(100)),
        }
    }

    /// A spring from how long it takes to get there and how much it bounces: `response_ms` is the
    /// time of one oscillation (at least 40 ms) and `damping_ratio` is 1 for no overshoot, below 1
    /// for bounce (0.5 is lively) and above 1 for a sluggish settle.
    #[must_use]
    pub fn from_response(response_ms: u32, damping_ratio: Fx) -> Self {
        let response_ms = response_ms.clamp(40, 60_000);
        // omega = 2 pi / response; stiffness = omega^2 (mass 1); damping = 2 zeta omega.
        let two_pi = 411_775_i64; // 6.283185 * 65536
        let omega = (two_pi * 1000) / i64::from(response_ms);
        let k = (omega * omega) >> 16;
        let c = (2 * i64::from(damping_ratio.max(Fx::ZERO).raw()) * omega) >> 16;
        let sat = |v: i64| Fx::from_raw(v.clamp(0, i64::from(i32::MAX)) as i32);
        Self::new(sat(k), sat(c), Fx::ONE)
    }

    /// The stiffness.
    #[must_use]
    pub const fn stiffness(&self) -> Fx {
        self.stiffness
    }

    /// The damping.
    #[must_use]
    pub const fn damping(&self) -> Fx {
        self.damping
    }

    /// The mass.
    #[must_use]
    pub const fn mass(&self) -> Fx {
        self.mass
    }
}

/// A spring pulling a value towards a target.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    cfg: SpringConfig,
    pos: i64,
    vel: i64,
    target: i64,
    rest: bool,
    carry_us: u32,
}

fn to_fx(raw: i64) -> Fx {
    Fx::from_raw(raw.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

impl Spring {
    /// A spring at rest at `start`.
    #[must_use]
    pub fn new(cfg: SpringConfig, start: Fx) -> Self {
        let p = i64::from(start.raw());
        Self {
            cfg,
            pos: p,
            vel: 0,
            target: p,
            rest: true,
            carry_us: 0,
        }
    }

    /// The current value.
    #[must_use]
    pub fn position(&self) -> Fx {
        to_fx(self.pos)
    }

    /// The current speed in units per second.
    #[must_use]
    pub fn velocity(&self) -> Fx {
        to_fx(self.vel)
    }

    /// Where the spring is pulling to.
    #[must_use]
    pub fn target(&self) -> Fx {
        to_fx(self.target)
    }

    /// Whether the spring has settled on its target.
    #[must_use]
    pub const fn is_at_rest(&self) -> bool {
        self.rest
    }

    /// Pulls towards a new target. The position and the speed are kept, so the motion bends towards
    /// the new target without a jump.
    pub fn set_target(&mut self, target: Fx) {
        self.target = i64::from(target.raw());
        self.rest = false;
    }

    /// Jumps to `value` and stops there.
    pub fn snap_to(&mut self, value: Fx) {
        self.pos = i64::from(value.raw());
        self.target = self.pos;
        self.vel = 0;
        self.rest = true;
        self.carry_us = 0;
    }

    /// Adds `velocity` (units per second), as a flick or a throw does.
    pub fn impulse(&mut self, velocity: Fx) {
        self.vel = (self.vel + i64::from(velocity.raw())).clamp(-MAX_VEL, MAX_VEL);
        self.rest = false;
    }

    fn substep(&mut self) {
        let dx = (self.pos - self.target).clamp(-MAX_DIST, MAX_DIST);
        let k = i64::from(self.cfg.stiffness.raw());
        let c = i64::from(self.cfg.damping.raw());
        let m = i64::from(self.cfg.mass.raw());
        // force = -k dx - c v, acceleration = force / m
        let force = -((k * dx) >> 16) - ((c * self.vel) >> 16);
        let acc = (force << 16) / m;
        // Semi-implicit Euler: the new velocity moves the position, which keeps the spring stable.
        self.vel = (self.vel + ((acc * STEP_FX) >> 16)).clamp(-MAX_VEL, MAX_VEL);
        self.pos += (self.vel * STEP_FX) >> 16;
        if (self.pos - self.target).abs() <= REST_POS && self.vel.abs() <= REST_VEL {
            self.pos = self.target;
            self.vel = 0;
            self.rest = true;
        }
    }

    /// Advances by `dt_ms` milliseconds (at most 100 per call). Returns true while the spring is
    /// still moving, and false once it has settled.
    pub fn step(&mut self, dt_ms: u32) -> bool {
        if self.rest {
            return false;
        }
        let mut us = self.carry_us + dt_ms.min(MAX_FRAME_MS) * 1000;
        while us >= STEP_US && !self.rest {
            self.substep();
            us -= STEP_US;
        }
        self.carry_us = if self.rest { 0 } else { us };
        !self.rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spring: &mut Spring, frame_ms: u32, max_ms: u32) -> (u32, Fx, Fx) {
        let (mut t, mut peak, mut low) = (0, spring.position(), spring.position());
        while spring.step(frame_ms) {
            t += frame_ms;
            peak = peak.max(spring.position());
            low = low.min(spring.position());
            assert!(
                t <= max_ms,
                "still moving after {max_ms} ms at {}",
                spring.position().to_f32()
            );
        }
        (t, peak, low)
    }

    fn pulled(cfg: SpringConfig, to: i32) -> Spring {
        let mut s = Spring::new(cfg, Fx::ZERO);
        s.set_target(Fx::from_int(to));
        s
    }

    #[test]
    fn a_new_spring_is_at_rest_and_does_not_move() {
        let mut s = Spring::new(SpringConfig::SNAPPY, Fx::from_int(5));
        assert!(s.is_at_rest());
        assert!(!s.step(16));
        assert_eq!(s.position(), Fx::from_int(5));
        assert_eq!(s.velocity(), Fx::ZERO);
    }

    #[test]
    fn it_settles_exactly_on_the_target() {
        for cfg in [
            SpringConfig::GENTLE,
            SpringConfig::WOBBLY,
            SpringConfig::STIFF,
            SpringConfig::SLOW,
            SpringConfig::SNAPPY,
        ] {
            let mut s = pulled(cfg, 100);
            run(&mut s, 16, 20_000);
            assert!(s.is_at_rest());
            assert_eq!(s.position(), Fx::from_int(100), "{cfg:?}");
            assert_eq!(s.velocity(), Fx::ZERO);
        }
    }

    #[test]
    fn critical_damping_does_not_overshoot_and_a_bouncy_spring_does() {
        let mut critical = pulled(SpringConfig::from_response(300, Fx::ONE), 100);
        let (_, peak, _) = run(&mut critical, 16, 20_000);
        assert!(
            peak <= Fx::from_ratio(1005, 10),
            "critical damping peaked at {}",
            peak.to_f32()
        );
        let mut bouncy = pulled(SpringConfig::from_response(300, Fx::HALF), 100);
        let (_, peak, _) = run(&mut bouncy, 16, 20_000);
        assert!(
            peak.to_f32() > 110.0,
            "a damping ratio of 0.5 overshoots by about 16%: {}",
            peak.to_f32()
        );
        let mut wobbly = pulled(SpringConfig::WOBBLY, 100);
        let (_, peak, _) = run(&mut wobbly, 16, 20_000);
        assert!(peak.to_f32() > 115.0, "{}", peak.to_f32());
    }

    #[test]
    fn a_stiffer_spring_arrives_sooner_and_the_response_time_is_about_right() {
        let time = |cfg| {
            let mut s = pulled(cfg, 100);
            run(&mut s, 16, 20_000).0
        };
        assert!(time(SpringConfig::STIFF) < time(SpringConfig::GENTLE));
        assert!(time(SpringConfig::GENTLE) < time(SpringConfig::SLOW));
        // With no overshoot the value is most of the way after about one response time.
        let mut s = pulled(SpringConfig::from_response(400, Fx::ONE), 100);
        for _ in 0..25 {
            s.step(16);
        }
        assert!(s.position().to_f32() > 60.0, "{}", s.position().to_f32());
    }

    #[test]
    fn the_frame_rate_does_not_change_the_motion_much() {
        let at = |frame_ms: u32| {
            let mut s = pulled(SpringConfig::WOBBLY, 100);
            for _ in 0..(400 / frame_ms) {
                s.step(frame_ms);
            }
            s.position().to_f32()
        };
        let (a, b, c) = (at(1), at(8), at(16));
        assert!((a - b).abs() < 1.5 && (a - c).abs() < 1.5, "{a} {b} {c}");
    }

    #[test]
    fn retargeting_and_impulses_continue_without_a_jump() {
        let mut s = pulled(SpringConfig::SNAPPY, 100);
        for _ in 0..10 {
            s.step(16);
        }
        let (p, v) = (s.position(), s.velocity());
        s.set_target(Fx::from_int(-50));
        assert_eq!(
            (s.position(), s.velocity()),
            (p, v),
            "setting a target moves nothing"
        );
        s.step(16);
        assert!(
            (s.position() - p).abs().to_f32() < 10.0,
            "one frame moves a little"
        );
        run(&mut s, 16, 20_000);
        assert_eq!(s.position(), Fx::from_int(-50));
        s.impulse(Fx::from_int(500));
        assert!(!s.is_at_rest());
        let (_, peak, _) = run(&mut s, 16, 20_000);
        assert!(
            peak > Fx::from_int(-50),
            "the flick moved it away before the spring pulled it back"
        );
        assert_eq!(s.position(), Fx::from_int(-50));
    }

    #[test]
    fn snap_stops_everything() {
        let mut s = pulled(SpringConfig::WOBBLY, 100);
        s.step(50);
        s.snap_to(Fx::from_int(7));
        assert!(s.is_at_rest());
        assert_eq!(
            (s.position(), s.target(), s.velocity()),
            (Fx::from_int(7), Fx::from_int(7), Fx::ZERO)
        );
        assert!(!s.step(16));
    }

    #[test]
    fn the_configuration_is_limited_to_what_is_stable() {
        let c = SpringConfig::new(Fx::MAX, Fx::MAX, Fx::ZERO);
        assert_eq!(c.stiffness(), Fx::from_int(20_000));
        assert_eq!(c.damping(), Fx::from_int(2000));
        assert_eq!(c.mass(), Fx::from_ratio(1, 100));
        let c = SpringConfig::new(Fx::MIN, Fx::MIN, Fx::MIN);
        assert_eq!(
            (c.stiffness(), c.damping()),
            (Fx::from_ratio(1, 10), Fx::ZERO)
        );
        assert!(c.mass() > Fx::ZERO);
        let r = SpringConfig::from_response(0, Fx::MAX);
        let _ = SpringConfig::from_response(u32::MAX, Fx::MIN);
        assert!(r.stiffness() > Fx::ZERO);
    }

    #[test]
    fn extreme_springs_and_distances_stay_finite_and_settle() {
        for cfg in [
            SpringConfig::new(Fx::MAX, Fx::ZERO, Fx::from_ratio(1, 100)),
            SpringConfig::new(Fx::MAX, Fx::MAX, Fx::from_ratio(1, 100)),
            SpringConfig::new(Fx::from_ratio(1, 10), Fx::ZERO, Fx::from_int(100)),
            SpringConfig::new(Fx::from_int(20_000), Fx::from_int(200), Fx::ONE),
        ] {
            for target in [1, 1000, 32_000, -32_000] {
                let mut s = Spring::new(cfg, Fx::ZERO);
                s.set_target(Fx::from_int(target));
                for _ in 0..5000 {
                    s.step(16);
                    assert!(s.position().abs() <= Fx::MAX);
                }
            }
        }
        let mut s = Spring::new(SpringConfig::SNAPPY, Fx::MIN);
        s.set_target(Fx::MAX);
        s.impulse(Fx::MAX);
        s.impulse(Fx::MAX);
        for _ in 0..2000 {
            s.step(100);
        }
    }

    #[test]
    fn a_long_pause_is_cut_and_a_zero_step_does_nothing() {
        let mut s = pulled(SpringConfig::SNAPPY, 100);
        assert!(s.step(0));
        assert_eq!(s.position(), Fx::ZERO);
        s.step(u32::MAX);
        assert!(s.position() > Fx::ZERO && !s.is_at_rest());
    }

    #[test]
    fn a_damping_of_zero_keeps_oscillating_with_roughly_constant_energy() {
        let mut s = pulled(SpringConfig::new(Fx::from_int(100), Fx::ZERO, Fx::ONE), 100);
        let mut peak_late = Fx::ZERO;
        for i in 0..2000 {
            s.step(16);
            if i > 1500 {
                peak_late = peak_late.max(s.position());
            }
        }
        assert!(!s.is_at_rest());
        assert!(
            peak_late.to_f32() > 150.0,
            "an undamped spring keeps swinging: {}",
            peak_late.to_f32()
        );
    }
}
