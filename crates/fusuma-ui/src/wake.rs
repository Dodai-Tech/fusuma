//! When a platform should run the next frame, so a chip can sleep instead of polling.

/// What an app needs from the clock, from most to least urgent (the order of the variants is the order of
/// urgency, so the smaller of two is the one to obey).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wake {
    /// A frame as soon as possible: something is moving or has not been drawn yet.
    Now,
    /// Nothing moves, but time has to pass: wake in this many milliseconds (a toast that hides, a long press
    /// that fires). The platform may sleep until then.
    At(u32),
    /// Nothing at all until an input arrives: sleep as long as you like.
    Never,
}

impl Wake {
    /// The wake for `wants_frame` and an optional `deadline` in milliseconds.
    #[must_use]
    pub const fn of(wants_frame: bool, deadline: Option<u32>) -> Self {
        match (wants_frame, deadline) {
            (true, _) => Self::Now,
            (false, Some(ms)) => Self::At(ms),
            (false, None) => Self::Never,
        }
    }

    /// How long to sleep from now, in milliseconds, at most `cap` (a platform that polls something, such as a touch
    /// panel, never sleeps longer than its polling period). `None` for a frame right now.
    #[must_use]
    pub const fn sleep_ms(self, cap: u32) -> Option<u32> {
        match self {
            Self::Now => None,
            Self::At(ms) => Some(if ms < cap { ms } else { cap }),
            Self::Never => Some(cap),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_more_urgent_wake_is_the_smaller() {
        assert!(Wake::Now < Wake::At(0));
        assert!(Wake::At(5) < Wake::At(9));
        assert!(Wake::At(u32::MAX) < Wake::Never);
        assert_eq!(Wake::At(40).min(Wake::At(12)), Wake::At(12));
        assert_eq!(Wake::Never.min(Wake::Now), Wake::Now);
    }

    #[test]
    fn a_wake_is_made_from_what_the_app_says() {
        assert_eq!(Wake::of(true, Some(9)), Wake::Now);
        assert_eq!(Wake::of(false, Some(9)), Wake::At(9));
        assert_eq!(Wake::of(false, None), Wake::Never);
    }

    #[test]
    fn a_polling_platform_sleeps_no_longer_than_its_period() {
        assert_eq!(Wake::Now.sleep_ms(8), None);
        assert_eq!(Wake::At(3).sleep_ms(8), Some(3));
        assert_eq!(Wake::At(500).sleep_ms(8), Some(8));
        assert_eq!(Wake::Never.sleep_ms(8), Some(8));
        assert_eq!(Wake::Never.sleep_ms(u32::MAX), Some(u32::MAX));
    }
}
