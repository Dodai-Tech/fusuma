//! A fling: a velocity that fades with friction, as when a list is flicked.

use crate::Fx;

const STEP_US: u32 = 4167;
const STEP_FX: i64 = 273;
const MAX_FRAME_MS: u32 = 100;
/// Below 5 pixels per second the motion stops.
const STOP_VEL: i64 = 5 << 16;
const MAX_VEL: i64 = 1_000_000 << 16;

/// A value moving with a velocity that decays exponentially, `v' = -friction * v`.
#[derive(Clone, Copy, Debug)]
pub struct Decay {
    pos: i64,
    vel: i64,
    friction: i64,
    rest: bool,
    carry_us: u32,
}

impl Decay {
    /// A value at `start` thrown with `velocity` (units per second). `friction` is how fast the
    /// speed fades per second: 2 is a long glide, 4 a typical scroll fling, 10 a quick stop. It is
    /// limited to 0.1 to 100.
    #[must_use]
    pub fn new(start: Fx, velocity: Fx, friction: Fx) -> Self {
        let vel = i64::from(velocity.raw()).clamp(-MAX_VEL, MAX_VEL);
        Self {
            pos: i64::from(start.raw()),
            vel,
            friction: i64::from(
                friction
                    .clamp(Fx::from_ratio(1, 10), Fx::from_int(100))
                    .raw(),
            ),
            rest: vel.abs() < STOP_VEL,
            carry_us: 0,
        }
    }

    /// The current value.
    #[must_use]
    pub fn position(&self) -> Fx {
        Fx::from_raw(self.pos.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }

    /// The current speed in units per second.
    #[must_use]
    pub fn velocity(&self) -> Fx {
        Fx::from_raw(self.vel.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }

    /// Whether the motion has stopped.
    #[must_use]
    pub const fn is_at_rest(&self) -> bool {
        self.rest
    }

    /// Where the value will stop: `position + velocity / friction` (exact for the continuous
    /// motion, and a good guide for snapping to the nearest page or item).
    #[must_use]
    pub fn projected_end(&self) -> Fx {
        let end = self.pos + (self.vel << 16) / self.friction;
        Fx::from_raw(end.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }

    /// Stops the motion where it is, as a touch on the list does.
    pub fn stop(&mut self) {
        self.vel = 0;
        self.rest = true;
    }

    /// Advances by `dt_ms` milliseconds (at most 100 per call). Returns true while still moving.
    pub fn step(&mut self, dt_ms: u32) -> bool {
        if self.rest {
            return false;
        }
        let mut us = self.carry_us + dt_ms.min(MAX_FRAME_MS) * 1000;
        // The fraction of the speed kept in one step: 1 - friction * dt.
        let keep = (1 << 16) - ((self.friction * STEP_FX) >> 16).min(1 << 16);
        while us >= STEP_US && !self.rest {
            self.pos += (self.vel * STEP_FX) >> 16;
            self.vel = (self.vel * keep) >> 16;
            if self.vel.abs() < STOP_VEL {
                self.vel = 0;
                self.rest = true;
            }
            us -= STEP_US;
        }
        self.carry_us = if self.rest { 0 } else { us };
        !self.rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Decay, frame_ms: u32) -> u32 {
        let mut t = 0;
        while d.step(frame_ms) {
            t += frame_ms;
            assert!(t < 120_000, "never stops");
        }
        t
    }

    #[test]
    fn it_travels_about_velocity_over_friction_and_stops() {
        let mut d = Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(4));
        let predicted = d.projected_end();
        assert!((predicted.to_f32() - 250.0).abs() < 0.01);
        run(&mut d, 16);
        assert!(d.is_at_rest());
        assert_eq!(d.velocity(), Fx::ZERO);
        // The slow tail below the stop speed is cut, so it ends a little short of the prediction.
        let end = d.position().to_f32();
        assert!(end > 240.0 && end <= 250.5, "{end}");
    }

    #[test]
    fn a_negative_velocity_goes_the_other_way() {
        let mut d = Decay::new(Fx::from_int(100), Fx::from_int(-800), Fx::from_int(4));
        run(&mut d, 16);
        assert!(d.position().to_f32() < -90.0, "{}", d.position().to_f32());
    }

    #[test]
    fn more_friction_stops_sooner_and_shorter() {
        let go = |f| {
            let mut d = Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(f));
            let t = run(&mut d, 16);
            (t, d.position())
        };
        let (t_low, x_low) = go(2);
        let (t_high, x_high) = go(10);
        assert!(t_low > t_high && x_low > x_high);
    }

    #[test]
    fn a_slow_start_does_not_move_and_stop_freezes_it() {
        let mut d = Decay::new(Fx::from_int(5), Fx::from_int(3), Fx::from_int(4));
        assert!(d.is_at_rest());
        assert!(!d.step(16));
        assert_eq!(d.position(), Fx::from_int(5));
        let mut d = Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(4));
        d.step(50);
        let p = d.position();
        d.stop();
        assert!(!d.step(50));
        assert_eq!(d.position(), p);
    }

    #[test]
    fn the_frame_rate_does_not_change_the_distance_much() {
        let go = |frame_ms| {
            let mut d = Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(4));
            run(&mut d, frame_ms);
            d.position().to_f32()
        };
        assert!((go(1) - go(16)).abs() < 3.0);
    }

    #[test]
    fn extreme_inputs_are_limited_and_never_panic() {
        let mut d = Decay::new(Fx::MAX, Fx::MAX, Fx::MIN);
        for _ in 0..1000 {
            d.step(u32::MAX);
        }
        let _ = d.projected_end();
        let mut d = Decay::new(Fx::MIN, Fx::MIN, Fx::MAX);
        run(&mut d, 100);
        let d2 = Decay::new(Fx::ZERO, Fx::from_int(100), Fx::ZERO);
        assert!(
            d2.projected_end() <= Fx::from_int(1001),
            "the friction is limited to 0.1"
        );
        let mut d = Decay::new(Fx::ZERO, Fx::from_int(500), Fx::from_int(4));
        assert!(d.step(0));
        assert_eq!(d.position(), Fx::ZERO);
    }
}
