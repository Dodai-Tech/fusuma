//! A fixed number of timed values with an easing on each segment.

use crate::{Easing, Fx};

/// One keyframe: at `at_ms` the value is `value`. The easing shapes the segment that *ends* at
/// this keyframe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keyframe {
    /// Time from the start of the animation.
    pub at_ms: u32,
    /// The value at that time.
    pub value: Fx,
    /// How the value gets here from the previous keyframe.
    pub easing: Easing,
}

impl Keyframe {
    /// A keyframe.
    #[must_use]
    pub const fn new(at_ms: u32, value: Fx, easing: Easing) -> Self {
        Self {
            at_ms,
            value,
            easing,
        }
    }
}

/// `N` keyframes, sorted by time. Before the first the value is the first one's, after the last it
/// is the last one's. Two keyframes at the same time make a jump: the first value holds up to that
/// instant and the second from the next millisecond on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keyframes<const N: usize> {
    frames: [Keyframe; N],
}

impl<const N: usize> Keyframes<N> {
    /// Sorts `frames` by time (keyframes at the same time keep their order).
    #[must_use]
    pub fn new(mut frames: [Keyframe; N]) -> Self {
        // Insertion sort: stable, needs no allocator, and a list of keyframes is short.
        for i in 1..N {
            let mut j = i;
            while j > 0 && frames[j - 1].at_ms > frames[j].at_ms {
                frames.swap(j - 1, j);
                j -= 1;
            }
        }
        Self { frames }
    }

    /// The time of the last keyframe.
    #[must_use]
    pub fn duration_ms(&self) -> u32 {
        self.frames.last().map_or(0, |f| f.at_ms)
    }

    /// The value `t_ms` after the start.
    #[must_use]
    pub fn value(&self, t_ms: u32) -> Fx {
        let (Some(first), Some(last)) = (self.frames.first(), self.frames.last()) else {
            return Fx::ZERO;
        };
        if t_ms <= first.at_ms {
            return first.value;
        }
        if t_ms >= last.at_ms {
            return last.value;
        }
        for pair in self.frames.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t_ms < b.at_ms {
                let span = b.at_ms - a.at_ms;
                let t = Fx::from_raw((((t_ms - a.at_ms) << 16) / span).min(1 << 16) as i32);
                return a.value.lerp(b.value, b.easing.apply(t));
            }
        }
        last.value
    }

    /// Whether `t_ms` is at or past the last keyframe.
    #[must_use]
    pub fn is_done(&self, t_ms: u32) -> bool {
        t_ms >= self.duration_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn k(at: u32, v: i32) -> Keyframe {
        Keyframe::new(at, Fx::from_int(v), Easing::Linear)
    }

    #[test]
    fn interpolates_between_keyframes() {
        let kf = Keyframes::new([k(0, 0), k(100, 100), k(200, 0)]);
        assert_eq!(kf.value(0), Fx::ZERO);
        assert_eq!(kf.value(50), Fx::from_int(50));
        assert_eq!(kf.value(100), Fx::from_int(100));
        assert_eq!(kf.value(150), Fx::from_int(50));
        assert_eq!(kf.value(200), Fx::ZERO);
        assert_eq!(kf.duration_ms(), 200);
        assert!(!kf.is_done(199));
        assert!(kf.is_done(200));
    }

    #[test]
    fn holds_the_first_and_last_values_outside_the_range() {
        let kf = Keyframes::new([k(100, 10), k(200, 20)]);
        assert_eq!(kf.value(0), Fx::from_int(10));
        assert_eq!(kf.value(99), Fx::from_int(10));
        assert_eq!(kf.value(5000), Fx::from_int(20));
    }

    #[test]
    fn keyframes_are_sorted_and_each_segment_has_its_own_easing() {
        let kf = Keyframes::new([
            Keyframe::new(200, Fx::from_int(100), Easing::CubicOut),
            k(0, 0),
        ]);
        assert_eq!(kf.value(0), Fx::ZERO);
        assert!(
            kf.value(100) > Fx::from_int(80),
            "the segment ending at 200 eases out"
        );
    }

    #[test]
    fn degenerate_lists_are_fine() {
        let empty = Keyframes::<0>::new([]);
        assert_eq!(empty.value(5), Fx::ZERO);
        assert_eq!(empty.duration_ms(), 0);
        assert!(empty.is_done(0));
        let one = Keyframes::new([k(50, 7)]);
        assert_eq!(one.value(0), Fx::from_int(7));
        assert_eq!(one.value(99), Fx::from_int(7));
        // Two keyframes at the same time make a jump: the first value holds up to that instant and
        // the second from the next millisecond on.
        let same = Keyframes::new([k(10, 1), k(10, 2)]);
        assert_eq!(same.value(9), Fx::from_int(1));
        assert_eq!(same.value(10), Fx::from_int(1));
        assert_eq!(same.value(11), Fx::from_int(2));
    }

    #[test]
    fn huge_times_do_not_overflow() {
        let kf = Keyframes::new([k(0, 0), k(u32::MAX, 100)]);
        let _ = kf.value(u32::MAX / 2);
        let _ = kf.value(u32::MAX - 1);
        assert_eq!(kf.value(u32::MAX), Fx::from_int(100));
    }
}
