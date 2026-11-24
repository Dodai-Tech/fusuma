//! Composable clips: a short timeline built by chaining small pieces, with no allocation and no closures, so it
//! is a `Copy` value that fits a chip.
//!
//! ```
//! use fusuma_anim::{Clip, Fx};
//! // Fade in over 200 ms, hold, then pop; start 80 ms late; play twice.
//! let enter = Clip::fade(200).then(Clip::hold(100, Fx::ONE)).then(Clip::pop(250)).delay(80).repeat(2);
//! let value = enter.value(130);
//! assert!(value > Fx::ZERO);
//! ```

use crate::{Easing, Fx};

/// The most segments a clip holds. A clip built from more drops the extra ones: it is a limit of the type, so keep
/// a clip short (a transition is one to three segments).
pub const MAX_SEGMENTS: usize = 8;

/// The longest a single segment is, in milliseconds (so that progress fits the fixed-point maths).
const MAX_SEGMENT_MS: u32 = 1 << 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seg {
    dur_ms: u32,
    ease: Easing,
    from: Fx,
    to: Fx,
}

const EMPTY: Seg = Seg {
    dur_ms: 0,
    ease: Easing::Linear,
    from: Fx::ZERO,
    to: Fx::ZERO,
};

/// A value over time, made of up to [`MAX_SEGMENTS`] pieces played one after the other, and options that change how
/// they play: a delay, repeats, back-and-forth, reversed, faster or slower.
///
/// The pieces are tweens from one value to another (`0` to `1` for [`tween`](Self::tween)). A clip says what the
/// value is at a time since it started ([`value`](Self::value)) and whether it is over ([`is_done`](Self::is_done)),
/// so it works with any clock. To animate two properties together, read two clips at the same time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    seg: [Seg; MAX_SEGMENTS],
    len: u8,
    delay_ms: u32,
    /// How many times it plays; 0 is for ever.
    plays: u16,
    ping_pong: bool,
    reverse: bool,
    /// 100 is normal speed.
    speed_pct: u16,
}

impl Clip {
    /// One piece: `from` to `to` over `duration_ms` (at least 1) with `easing`.
    #[must_use]
    pub const fn segment(duration_ms: u32, easing: Easing, from: Fx, to: Fx) -> Self {
        let mut seg = [EMPTY; MAX_SEGMENTS];
        seg[0] = Seg {
            dur_ms: if duration_ms == 0 {
                1
            } else if duration_ms > MAX_SEGMENT_MS {
                MAX_SEGMENT_MS
            } else {
                duration_ms
            },
            ease: easing,
            from,
            to,
        };
        Self {
            seg,
            len: 1,
            delay_ms: 0,
            plays: 1,
            ping_pong: false,
            reverse: false,
            speed_pct: 100,
        }
    }

    /// 0 to 1 over `duration_ms` with `easing`.
    #[must_use]
    pub const fn tween(duration_ms: u32, easing: Easing) -> Self {
        Self::segment(duration_ms, easing, Fx::ZERO, Fx::ONE)
    }

    /// Stays at `value` for `duration_ms`: a pause between pieces.
    #[must_use]
    pub const fn hold(duration_ms: u32, value: Fx) -> Self {
        Self::segment(duration_ms, Easing::Linear, value, value)
    }

    /// 0 to 1 smoothly: for opacity.
    #[must_use]
    pub const fn fade(duration_ms: u32) -> Self {
        Self::tween(duration_ms, Easing::Smooth)
    }

    /// 0 to 1 starting fast and slowing down: for something that moves into place.
    #[must_use]
    pub const fn slide(duration_ms: u32) -> Self {
        Self::tween(duration_ms, Easing::CubicOut)
    }

    /// 0 to 1 with a small overshoot and settle: for something that appears.
    #[must_use]
    pub const fn pop(duration_ms: u32) -> Self {
        Self::tween(duration_ms, Easing::BackOut)
    }

    /// A shake, for an entry that is not valid: the value swings between about -1 and 1 and dies away to 0. Lasts
    /// 360 ms.
    #[must_use]
    pub fn shake() -> Self {
        let q = |n: i32| Fx::from_ratio(n, 10);
        let mut c = Self::segment(60, Easing::QuadInOut, Fx::ZERO, q(10));
        for (from, to) in [(10, -8), (-8, 6), (6, -4), (-4, 2), (2, 0)] {
            c = c.then(Self::segment(60, Easing::QuadInOut, q(from), q(to)));
        }
        c
    }

    /// `next` played after this one. Only the pieces of `next` are used: set options (delay, repeat, speed) on the
    /// result. Pieces past [`MAX_SEGMENTS`] are dropped.
    #[must_use]
    pub fn then(mut self, next: Self) -> Self {
        for s in &next.seg[..usize::from(next.len)] {
            if usize::from(self.len) < MAX_SEGMENTS {
                self.seg[usize::from(self.len)] = *s;
                self.len += 1;
            }
        }
        self
    }

    /// Starts `ms` later (added to any delay already set): the value is the first one until then.
    #[must_use]
    pub const fn delay(mut self, ms: u32) -> Self {
        self.delay_ms = self.delay_ms.saturating_add(ms);
        self
    }

    /// Plays `times` times in all (at least once).
    #[must_use]
    pub const fn repeat(mut self, times: u16) -> Self {
        self.plays = if times == 0 { 1 } else { times };
        self
    }

    /// Plays for ever.
    #[must_use]
    pub const fn forever(mut self) -> Self {
        self.plays = 0;
        self
    }

    /// Each play goes forward and then back to the start.
    #[must_use]
    pub const fn ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self
    }

    /// Plays backwards. Doing it twice plays forwards again.
    #[must_use]
    pub const fn reverse(mut self) -> Self {
        self.reverse = !self.reverse;
        self
    }

    /// Plays at `percent` of the normal speed (100 is normal, 200 twice as fast), between 10 and 1000.
    #[must_use]
    pub const fn speed(mut self, percent: u16) -> Self {
        self.speed_pct = if percent < 10 {
            10
        } else if percent > 1000 {
            1000
        } else {
            percent
        };
        self
    }

    /// For item `index` of a list: starts `index * step_ms` later than the first, so a list appears one by one.
    #[must_use]
    pub const fn stagger(self, index: u32, step_ms: u32) -> Self {
        self.delay(index.saturating_mul(step_ms))
    }

    /// The length of one pass through the pieces, in milliseconds at normal speed.
    fn total_ms(&self) -> i64 {
        self.seg[..usize::from(self.len)]
            .iter()
            .map(|s| i64::from(s.dur_ms))
            .sum()
    }

    /// The length of one play: a back-and-forth play is twice the pieces.
    fn play_ms(&self) -> i64 {
        self.total_ms() * if self.ping_pong { 2 } else { 1 }
    }

    /// How long the clip takes in all, in milliseconds, counting the delay, the repeats and the speed; `None` for ever.
    #[must_use]
    pub fn duration_ms(&self) -> Option<u32> {
        if self.plays == 0 {
            return None;
        }
        let run = self.play_ms() * i64::from(self.plays) * 100 / i64::from(self.speed_pct);
        Some(u32::try_from(i64::from(self.delay_ms) + run).unwrap_or(u32::MAX))
    }

    /// Whether the clip is over at `t_ms` after it started. Never, for a clip that plays for ever.
    #[must_use]
    pub fn is_done(&self, t_ms: u32) -> bool {
        self.duration_ms().is_some_and(|d| t_ms >= d)
    }

    /// The value `t_ms` after the clip started. Before the delay it is the first value; after the end it stays at
    /// the last one.
    #[must_use]
    pub fn value(&self, t_ms: u32) -> Fx {
        let total = self.total_ms();
        if self.len == 0 || total == 0 {
            return Fx::ZERO;
        }
        let ts =
            (i64::from(t_ms) - i64::from(self.delay_ms)).max(0) * i64::from(self.speed_pct) / 100;
        let play = self.play_ms();
        let over = self.plays > 0 && ts >= play * i64::from(self.plays);
        // Where in the pieces we are: a finished clip is at the end of its last play.
        let mut u = if over { play } else { ts % play };
        if self.ping_pong && u > total {
            u = 2 * total - u;
        }
        if self.reverse {
            u = total - u;
        }
        self.at(u.clamp(0, total))
    }

    /// The value `u` milliseconds into the pieces (0 to the total).
    fn at(&self, u: i64) -> Fx {
        let mut start = 0i64;
        for s in &self.seg[..usize::from(self.len)] {
            let dur = i64::from(s.dur_ms);
            if u <= start + dur {
                // Both fit an i32: a segment is at most 2^28 ms.
                let p = Fx::from_ratio((u - start) as i32, dur as i32);
                return s.from.lerp(s.to, s.ease.apply(p));
            }
            start += dur;
        }
        self.seg[usize::from(self.len) - 1].to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Fx, b: f32) -> bool {
        (a.to_f32() - b).abs() < 0.01
    }

    fn lin(ms: u32) -> Clip {
        Clip::tween(ms, Easing::Linear)
    }

    #[test]
    fn a_tween_runs_from_zero_to_one_and_stays_there() {
        let c = lin(100);
        assert!(near(c.value(0), 0.0) && near(c.value(50), 0.5) && near(c.value(100), 1.0));
        assert!(near(c.value(5_000), 1.0), "it stays at the end");
        assert!(!c.is_done(99) && c.is_done(100));
        assert_eq!(c.duration_ms(), Some(100));
    }

    #[test]
    fn then_plays_pieces_one_after_the_other() {
        let c = lin(100).then(Clip::hold(50, Fx::ONE)).then(Clip::segment(
            100,
            Easing::Linear,
            Fx::ONE,
            Fx::ZERO,
        ));
        assert_eq!(c.duration_ms(), Some(250));
        assert!(
            near(c.value(100), 1.0)
                && near(c.value(125), 1.0)
                && near(c.value(200), 0.5)
                && near(c.value(250), 0.0)
        );
    }

    #[test]
    fn a_clip_longer_than_the_limit_drops_the_extra_pieces_instead_of_failing() {
        let mut c = lin(10);
        for _ in 0..20 {
            c = c.then(lin(10));
        }
        assert_eq!(c.duration_ms(), Some(10 * MAX_SEGMENTS as u32));
    }

    #[test]
    fn delay_holds_the_first_value_and_stagger_delays_by_index() {
        let c = lin(100).delay(40);
        assert!(near(c.value(0), 0.0) && near(c.value(40), 0.0) && near(c.value(90), 0.5));
        assert_eq!(c.duration_ms(), Some(140));
        assert_eq!(
            lin(100).delay(10).delay(5).duration_ms(),
            Some(115),
            "delays add up"
        );
        let third = lin(100).stagger(3, 30);
        assert!(near(third.value(90), 0.0) && near(third.value(140), 0.5));
    }

    #[test]
    fn repeat_plays_again_and_forever_never_ends() {
        let c = lin(100).repeat(3);
        assert!(
            near(c.value(150), 0.5) && near(c.value(250), 0.5),
            "each play starts again"
        );
        assert_eq!(c.duration_ms(), Some(300));
        assert!(c.is_done(300) && near(c.value(9_999), 1.0));
        let f = lin(100).forever();
        assert_eq!(f.duration_ms(), None);
        assert!(!f.is_done(u32::MAX) && near(f.value(10_050), 0.5));
        assert_eq!(
            lin(100).repeat(0).duration_ms(),
            Some(100),
            "zero plays is one"
        );
    }

    #[test]
    fn ping_pong_goes_forward_then_back_and_ends_at_the_start() {
        let c = lin(100).ping_pong();
        assert_eq!(c.duration_ms(), Some(200));
        assert!(near(c.value(100), 1.0) && near(c.value(150), 0.5) && near(c.value(200), 0.0));
        assert!(near(c.value(5_000), 0.0), "over, and back where it began");
        let twice = lin(100).ping_pong().repeat(2);
        // Two plays of forward-and-back: 0 at the start of each, 1 at the turn, back to 0 at the very end.
        assert!(
            near(twice.value(200), 0.0)
                && near(twice.value(300), 1.0)
                && near(twice.value(350), 0.5)
        );
        assert!(twice.is_done(400) && near(twice.value(400), 0.0));
    }

    #[test]
    fn reverse_plays_backwards_and_twice_is_forwards_again() {
        let c = lin(100).reverse();
        assert!(near(c.value(0), 1.0) && near(c.value(25), 0.75) && near(c.value(100), 0.0));
        assert!(near(lin(100).reverse().reverse().value(25), 0.25));
    }

    #[test]
    fn speed_scales_time_and_is_held_to_a_sane_range() {
        let fast = lin(100).speed(200);
        assert!(near(fast.value(25), 0.5) && fast.is_done(50));
        assert_eq!(fast.duration_ms(), Some(50));
        assert_eq!(
            lin(100).speed(0).duration_ms(),
            Some(1000),
            "no slower than a tenth"
        );
        assert_eq!(
            lin(100).speed(u16::MAX).duration_ms(),
            Some(10),
            "no faster than ten times"
        );
    }

    #[test]
    fn options_combine_in_any_order() {
        let a = lin(100).delay(20).repeat(2).ping_pong().speed(200);
        let b = lin(100).speed(200).ping_pong().repeat(2).delay(20);
        assert_eq!(a, b);
        assert_eq!(a.duration_ms(), Some(20 + 200 * 2 * 100 / 200));
    }

    #[test]
    fn presets_have_their_character() {
        assert!(
            Clip::pop(200).value(120) > Fx::ONE,
            "pop overshoots before it settles"
        );
        assert!(near(Clip::pop(200).value(200), 1.0));
        let (mut lo, mut hi) = (Fx::ZERO, Fx::ZERO);
        let shake = Clip::shake();
        for t in (0..=shake.duration_ms().unwrap()).step_by(5) {
            let v = shake.value(t);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(
            near(lo, -0.8) && near(hi, 1.0),
            "swings both ways: {} {}",
            lo.to_f32(),
            hi.to_f32()
        );
        assert!(near(shake.value(5_000), 0.0), "and dies away");
        assert!(
            Clip::slide(100).value(25) > Fx::from_ratio(1, 4),
            "slides in fast"
        );
        assert!(
            Clip::fade(100).value(10) < Fx::from_ratio(1, 10),
            "fades in gently"
        );
    }

    #[test]
    fn odd_inputs_never_panic() {
        for c in [
            lin(0),
            lin(u32::MAX),
            Clip::hold(0, Fx::MAX),
            lin(5).delay(u32::MAX).speed(1000),
            lin(7).forever().ping_pong().reverse(),
        ] {
            for t in [0, 1, 7, 1_000, u32::MAX] {
                let _ = c.value(t);
                let _ = c.is_done(t);
            }
            let _ = c.duration_ms();
        }
    }
}
