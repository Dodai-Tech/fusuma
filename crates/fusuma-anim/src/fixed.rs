//! Q16.16 fixed-point numbers.

use core::ops::{Add, Div, Mul, Neg, Sub};

/// A signed fixed-point number with 16 fractional bits: range about ±32768, step 1/65536.
///
/// Arithmetic saturates instead of wrapping or panicking, so a runaway value stays at the end of
/// the range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fx(i32);

fn sat(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Fx {
    /// Number of fractional bits.
    pub const FRAC: u32 = 16;
    /// 0.
    pub const ZERO: Self = Self(0);
    /// 1.
    pub const ONE: Self = Self(1 << 16);
    /// 0.5.
    pub const HALF: Self = Self(1 << 15);
    /// The largest value.
    pub const MAX: Self = Self(i32::MAX);
    /// The smallest value.
    pub const MIN: Self = Self(i32::MIN);

    /// From the raw representation (the value times 65536).
    #[must_use]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// The raw representation (the value times 65536).
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// A whole number, saturating.
    #[must_use]
    pub const fn from_int(i: i32) -> Self {
        let v = (i as i64) << 16;
        Self(if v > i32::MAX as i64 {
            i32::MAX
        } else if v < i32::MIN as i64 {
            i32::MIN
        } else {
            v as i32
        })
    }

    /// `n / d`, rounded towards zero; saturates, and is 0 for `0 / 0`.
    #[must_use]
    pub fn from_ratio(n: i32, d: i32) -> Self {
        if d == 0 {
            return match n.signum() {
                1 => Self::MAX,
                -1 => Self::MIN,
                _ => Self::ZERO,
            };
        }
        Self(sat((i64::from(n) << 16) / i64::from(d)))
    }

    /// From a float, saturating; NaN is 0.
    #[must_use]
    pub fn from_f32(f: f32) -> Self {
        if f.is_nan() {
            return Self::ZERO;
        }
        // `as` saturates for floats.
        Self((f * 65536.0) as i32)
    }

    /// As a float.
    #[must_use]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 65536.0
    }

    /// The nearest whole number, halves rounded up.
    #[must_use]
    pub fn round(self) -> i32 {
        ((i64::from(self.0) + 32768) >> 16) as i32
    }

    /// The absolute value, saturating (`MIN.abs()` is `MAX`).
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// The smaller of two values.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// The larger of two values.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// Limited to `lo..=hi`; `lo` wins if the bounds are swapped.
    #[must_use]
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        self.min(hi).max(lo)
    }

    /// Limited to 0..=1.
    #[must_use]
    pub fn clamp01(self) -> Self {
        self.clamp(Self::ZERO, Self::ONE)
    }

    /// `self + (other - self) * t`, computed without overflow in between.
    #[must_use]
    pub fn lerp(self, other: Self, t: Self) -> Self {
        let d = i64::from(other.0) - i64::from(self.0);
        Self(sat(i64::from(self.0) + ((d * i64::from(t.0)) >> 16)))
    }
}

impl Add for Fx {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self(self.0.saturating_add(o.0))
    }
}

impl Sub for Fx {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self(self.0.saturating_sub(o.0))
    }
}

impl Neg for Fx {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl Mul for Fx {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self(sat((i64::from(self.0) * i64::from(o.0)) >> 16))
    }
}

impl Div for Fx {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        if o.0 == 0 {
            return match self.0.signum() {
                1 => Self::MAX,
                -1 => Self::MIN,
                _ => Self::ZERO,
            };
        }
        Self(sat((i64::from(self.0) << 16) / i64::from(o.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_and_ratios() {
        assert_eq!(Fx::from_int(3).raw(), 3 << 16);
        assert_eq!(Fx::from_ratio(1, 2), Fx::HALF);
        assert_eq!(Fx::from_ratio(-3, 2).to_f32(), -1.5);
        assert_eq!(Fx::from_ratio(1, 0), Fx::MAX);
        assert_eq!(Fx::from_ratio(-1, 0), Fx::MIN);
        assert_eq!(Fx::from_ratio(0, 0), Fx::ZERO);
        assert_eq!(Fx::from_int(i32::MAX), Fx::MAX);
        assert_eq!(Fx::from_int(i32::MIN), Fx::MIN);
    }

    #[test]
    fn arithmetic() {
        let (a, b) = (Fx::from_ratio(3, 2), Fx::from_int(2));
        assert_eq!((a * b).to_f32(), 3.0);
        assert_eq!((a / b).to_f32(), 0.75);
        assert_eq!((a + b).to_f32(), 3.5);
        assert_eq!((a - b).to_f32(), -0.5);
        assert_eq!((-a).to_f32(), -1.5);
        assert_eq!(Fx::from_ratio(-5, 2).abs().to_f32(), 2.5);
    }

    #[test]
    fn saturation_never_wraps_or_panics() {
        assert_eq!(Fx::MAX + Fx::ONE, Fx::MAX);
        assert_eq!(Fx::MIN - Fx::ONE, Fx::MIN);
        assert_eq!(Fx::MAX * Fx::from_int(2), Fx::MAX);
        assert_eq!(Fx::MIN * Fx::from_int(2), Fx::MIN);
        assert_eq!(-Fx::MIN, Fx::MAX);
        assert_eq!(Fx::MIN.abs(), Fx::MAX);
        assert_eq!(Fx::MAX / Fx::from_raw(1), Fx::MAX);
        assert_eq!(Fx::ONE / Fx::ZERO, Fx::MAX);
        assert_eq!(Fx::ZERO / Fx::ZERO, Fx::ZERO);
    }

    #[test]
    fn rounding_and_floats() {
        assert_eq!(Fx::from_ratio(5, 2).round(), 3);
        assert_eq!(Fx::from_ratio(-5, 2).round(), -2, "halves round up");
        assert_eq!(Fx::from_ratio(-7, 4).round(), -2);
        assert_eq!(Fx::MAX.round(), 32768);
        assert_eq!(Fx::from_f32(0.25).raw(), 16384);
        assert_eq!(Fx::from_f32(f32::NAN), Fx::ZERO);
        assert_eq!(Fx::from_f32(1e30), Fx::MAX);
        assert_eq!(Fx::from_f32(-1e30), Fx::MIN);
        assert_eq!(Fx::from_f32(f32::INFINITY), Fx::MAX);
    }

    #[test]
    fn clamp_and_lerp() {
        assert_eq!(Fx::from_int(5).clamp01(), Fx::ONE);
        assert_eq!(Fx::from_int(-5).clamp01(), Fx::ZERO);
        assert_eq!(Fx::HALF.clamp01(), Fx::HALF);
        assert_eq!(
            Fx::from_int(0).lerp(Fx::from_int(10), Fx::HALF),
            Fx::from_int(5)
        );
        assert_eq!(Fx::from_int(10).lerp(Fx::from_int(0), Fx::ONE), Fx::ZERO);
        // From one end of the range to the other does not overflow in between.
        assert_eq!(Fx::MIN.lerp(Fx::MAX, Fx::ONE), Fx::MAX);
        assert_eq!(Fx::MIN.lerp(Fx::MAX, Fx::ZERO), Fx::MIN);
        assert_eq!(
            Fx::ZERO.lerp(Fx::from_int(8), Fx::from_int(2)),
            Fx::from_int(16),
            "t beyond 1 extrapolates"
        );
    }
}
