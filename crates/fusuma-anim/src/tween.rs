//! A value that moves between two points over time.

use crate::{Easing, Fx};

/// A value moving from `from` to `to` over a duration, after an optional delay, along an easing
/// curve. It holds no clock: pass the time in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tween {
    from: Fx,
    to: Fx,
    start_ms: u32,
    delay_ms: u32,
    duration_ms: u32,
    easing: Easing,
}

/// The delay of item `index` in a staggered list: each starts `step_ms` after the one before.
#[must_use]
pub const fn stagger(index: u32, step_ms: u32) -> u32 {
    index.saturating_mul(step_ms)
}

impl Tween {
    /// A tween that starts at `start_ms` and lasts `duration_ms`.
    #[must_use]
    pub const fn new(from: Fx, to: Fx, start_ms: u32, duration_ms: u32, easing: Easing) -> Self {
        Self {
            from,
            to,
            start_ms,
            delay_ms: 0,
            duration_ms,
            easing,
        }
    }

    /// The same tween, waiting `delay_ms` after its start before it moves.
    #[must_use]
    pub const fn delayed(self, delay_ms: u32) -> Self {
        Self { delay_ms, ..self }
    }

    /// Milliseconds since the start; a clock that reads before the start (or that wrapped the other
    /// way) counts as "not begun".
    fn elapsed(&self, now_ms: u32) -> u32 {
        let e = now_ms.wrapping_sub(self.start_ms);
        if e > u32::MAX / 2 { 0 } else { e }
    }

    /// Linear progress at `now_ms`, 0 to 1, before the easing.
    #[must_use]
    pub fn progress(&self, now_ms: u32) -> Fx {
        let Some(moving) = self.elapsed(now_ms).checked_sub(self.delay_ms) else {
            return Fx::ZERO;
        };
        if self.duration_ms == 0 {
            return Fx::ONE;
        }
        let raw = (u64::from(moving) << 16) / u64::from(self.duration_ms);
        Fx::from_raw(raw.min(1 << 16) as i32)
    }

    /// The value at `now_ms`.
    #[must_use]
    pub fn value(&self, now_ms: u32) -> Fx {
        self.from
            .lerp(self.to, self.easing.apply(self.progress(now_ms)))
    }

    /// Whether the tween has reached its end at `now_ms`.
    #[must_use]
    pub fn is_done(&self, now_ms: u32) -> bool {
        self.progress(now_ms) >= Fx::ONE
    }

    /// Where the tween ends.
    #[must_use]
    pub const fn target(&self) -> Fx {
        self.to
    }

    /// Redirects the tween to `to` from where it is at `now_ms`, for the same duration and easing,
    /// so a change of mind mid-flight does not jump.
    #[must_use]
    pub fn retarget(&self, now_ms: u32, to: Fx) -> Self {
        Self::new(
            self.value(now_ms),
            to,
            now_ms,
            self.duration_ms,
            self.easing,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tween() -> Tween {
        Tween::new(Fx::ZERO, Fx::from_int(100), 1000, 200, Easing::Linear)
    }

    #[test]
    fn moves_from_start_to_end_and_stays() {
        let t = tween();
        assert_eq!(t.value(1000), Fx::ZERO);
        assert_eq!(t.value(1100), Fx::from_int(50));
        assert_eq!(t.value(1200), Fx::from_int(100));
        assert_eq!(t.value(99_999), Fx::from_int(100));
        assert!(!t.is_done(1199));
        assert!(t.is_done(1200));
        assert_eq!(t.target(), Fx::from_int(100));
    }

    #[test]
    fn the_easing_shapes_the_motion() {
        let t = Tween::new(Fx::ZERO, Fx::from_int(100), 0, 100, Easing::CubicOut);
        assert!(
            t.value(50) > Fx::from_int(80),
            "an ease-out is far along at the half"
        );
        let t = Tween::new(Fx::ZERO, Fx::from_int(100), 0, 100, Easing::CubicIn);
        assert!(t.value(50) < Fx::from_int(20));
    }

    #[test]
    fn a_delay_holds_the_start_value() {
        let t = tween().delayed(300);
        assert_eq!(t.value(1000), Fx::ZERO);
        assert_eq!(t.value(1299), Fx::ZERO);
        assert_eq!(t.value(1400), Fx::from_int(50));
        assert!(!t.is_done(1499));
        assert!(t.is_done(1500));
    }

    #[test]
    fn staggering_offsets_each_item() {
        assert_eq!(stagger(0, 40), 0);
        assert_eq!(stagger(3, 40), 120);
        assert_eq!(stagger(u32::MAX, 40), u32::MAX);
    }

    #[test]
    fn before_the_start_and_across_the_clock_wrap() {
        let t = tween();
        assert_eq!(t.value(500), Fx::ZERO, "before the start");
        let wrap = Tween::new(
            Fx::ZERO,
            Fx::from_int(100),
            u32::MAX - 50,
            200,
            Easing::Linear,
        );
        assert_eq!(
            wrap.value(49),
            Fx::from_int(50),
            "100 ms after the start, across the wrap"
        );
        assert_eq!(
            wrap.value(u32::MAX / 2),
            Fx::ZERO,
            "a reading far behind the start"
        );
    }

    #[test]
    fn zero_duration_jumps_at_once_after_the_delay() {
        let t = Tween::new(Fx::ZERO, Fx::from_int(7), 10, 0, Easing::Linear);
        assert_eq!(t.value(10), Fx::from_int(7));
        assert!(t.is_done(10));
        let t = t.delayed(5);
        assert_eq!(t.value(12), Fx::ZERO);
        assert_eq!(t.value(15), Fx::from_int(7));
    }

    #[test]
    fn retargeting_continues_from_where_the_value_is() {
        let t = tween();
        let mid = t.value(1100);
        let r = t.retarget(1100, Fx::from_int(-100));
        assert_eq!(r.value(1100), mid, "no jump");
        assert_eq!(r.value(1300), Fx::from_int(-100));
        assert!(r.value(1200) < mid);
    }

    #[test]
    fn extreme_values_saturate() {
        let t = Tween::new(Fx::MIN, Fx::MAX, 0, 10, Easing::BackOut);
        for now in [0, 3, 5, 10, 11, u32::MAX] {
            let _ = t.value(now);
        }
        assert_eq!(t.value(10), Fx::MAX);
        let long = Tween::new(Fx::ZERO, Fx::ONE, 0, u32::MAX, Easing::Linear);
        assert!(long.value(u32::MAX / 2) <= Fx::HALF);
    }
}
