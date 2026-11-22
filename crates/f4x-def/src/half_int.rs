//! Exact fixed-point values with a resolution of one half.
//!
//! HalfInt stores twice the represented value in an i32: raw value 7
//! represents 3.5. In a file it is the amount itself: `1`, `0.5`, `1.5`. This avoids floating-point rounding for game values
//! that are always whole numbers or halves. Arithmetic is supported for
//! addition, subtraction, and negation; convert to f32 only when needed
//! for display or interoperability.
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A signed fixed-point value whose smallest step is 0.5.
///
/// The inner integer is twice the represented value. Use from_int for whole
/// numbers and from_raw for an already-scaled value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HalfInt(i32);

impl HalfInt {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// One half.
    pub const HALF: Self = Self(1);

    /// The largest value (the "not reached yet" of a search).
    pub const MAX: Self = Self(i32::MAX);

    /// Creates a value from an integer that has already been doubled.
    ///
    /// For example, raw value 7 represents 3.5.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Creates a value from a whole number.
    pub const fn from_int(value: i32) -> Self {
        Self(value.saturating_mul(2))
    }

    /// Returns the doubled integer representation.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Returns whether this value represents a whole number.
    pub const fn is_whole(self) -> bool {
        self.0 % 2 == 0
    }

    /// The value rounded down to a whole number (towards negative infinity).
    pub const fn floor(self) -> i32 {
        self.0.div_euclid(2)
    }

    /// The value rounded up to a whole number (towards positive infinity): 1.5 is 2.
    pub const fn ceil(self) -> i32 {
        // In 64 bits, so that `HalfInt::MAX` has a ceiling.
        (self.0 as i64 + 1).div_euclid(2) as i32
    }

    /// Half of the value, rounded up to the next half: 3 gives 1.5, 1.5 gives 1, 1 gives 0.5.
    pub const fn half(self) -> Self {
        Self((self.0 as i64 + 1).div_euclid(2) as i32)
    }

    /// One half more: 3 becomes 3.5 (a `const` form of `+ HALF`).
    pub const fn plus_half(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// The value times `num` over `den`, rounded down to a half: 3 times 3 over 2 is 4.5, 0.5 times
    /// 1 over 2 is 0. The product is counted in 128 bits, so a large `num` does not overflow, and
    /// a result the type cannot hold is the nearest value it can (the largest or the smallest).
    /// `den` must not be 0.
    pub const fn scaled(self, num: i64, den: i64) -> Self {
        Self::clamped((self.0 as i128 * num as i128).div_euclid(den as i128))
    }

    /// The value of `raw` halves, or the nearest value the type holds.
    const fn clamped(raw: i128) -> Self {
        if raw > i32::MAX as i128 {
            Self(i32::MAX)
        } else if raw < i32::MIN as i128 {
            Self(i32::MIN)
        } else {
            Self(raw as i32)
        }
    }

    /// The value times `percent` over 100, rounded down to a half: 3 at 150 is 4.5.
    pub const fn percent(self, percent: i32) -> Self {
        self.scaled(percent as i64, 100)
    }

    /// The value times `percent` over 100, rounded to the nearest half (a half more rounds up): 1
    /// at 25 is 0.5, 1 at 20 is 0.
    pub const fn percent_nearest(self, percent: i32) -> Self {
        Self::clamped((self.0 as i128 * percent as i128 + 50).div_euclid(100))
    }

    /// The value without its sign.
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// Converts this value to a floating-point number, e.g. for display.
    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 2.0
    }

    /// The difference, or zero when `other` is the larger (what is left of a stock that cannot
    /// go below nothing, such as the moves of a unit).
    pub fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0).max(0))
    }
}

/// Serde adapter (`#[serde(with = "f4x_def::half_int::raw")]`) that writes a [`HalfInt`] as its
/// doubled count, the form of an amount a save kept in halves before `HalfInt` existed: the
/// moves of a unit are saved in half-moves, so saves of older versions still load.
pub mod raw {
    use super::HalfInt;

    pub fn serialize<S: serde::Serializer>(value: &HalfInt, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32(value.raw())
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HalfInt, D::Error> {
        <i32 as serde::Deserialize>::deserialize(d).map(HalfInt::from_raw)
    }
}

/// Sums and products stop at the ends of the range instead of wrapping or panicking: an amount of
/// gold or mana at the limit stays there.
impl Add for HalfInt {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for HalfInt {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Neg for HalfInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(self.0.saturating_neg())
    }
}

/// A whole number beside a `HalfInt` is that many whole units: `gold >= 5`, `gold - 5`.
impl PartialEq<i32> for HalfInt {
    fn eq(&self, other: &i32) -> bool {
        i64::from(self.0) == i64::from(*other) * 2
    }
}

impl PartialOrd<i32> for HalfInt {
    fn partial_cmp(&self, other: &i32) -> Option<std::cmp::Ordering> {
        Some(i64::from(self.0).cmp(&(i64::from(*other) * 2)))
    }
}

impl Add<i32> for HalfInt {
    type Output = Self;

    fn add(self, rhs: i32) -> Self::Output {
        self + Self::from_int(rhs)
    }
}

impl Sub<i32> for HalfInt {
    type Output = Self;

    fn sub(self, rhs: i32) -> Self::Output {
        self - Self::from_int(rhs)
    }
}

impl AddAssign<i32> for HalfInt {
    fn add_assign(&mut self, rhs: i32) {
        *self += Self::from_int(rhs);
    }
}

impl SubAssign<i32> for HalfInt {
    fn sub_assign(&mut self, rhs: i32) {
        *self -= Self::from_int(rhs);
    }
}

impl Mul<i32> for HalfInt {
    type Output = Self;

    fn mul(self, rhs: i32) -> Self::Output {
        Self(self.0.saturating_mul(rhs))
    }
}

/// Divided by a whole number, rounded to a half towards zero: 3 over 2 is 1.5, 1.5 over 2 is 0.5,
/// 0.5 over 2 is 0.
impl Div<i32> for HalfInt {
    type Output = Self;

    fn div(self, rhs: i32) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl AddAssign for HalfInt {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}

impl SubAssign for HalfInt {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_sub(rhs.0);
    }
}

impl Sum for HalfInt {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

/// Shows whole values as `3` and halves as `3.5`.
impl fmt::Display for HalfInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = match (self.0 < 0, f.sign_plus()) {
            (true, _) => "-",
            (false, true) => "+",
            (false, false) => "",
        };
        let abs = self.0.unsigned_abs();
        if abs.is_multiple_of(2) {
            write!(f, "{sign}{}", abs / 2)
        } else {
            write!(f, "{sign}{}.5", abs / 2)
        }
    }
}

/// Written and read as the amount itself, not the doubled count: `1` is one, `1.5` is one and a
/// half, `0.5` a half. A number that is not a multiple of 0.5 is an error.
impl serde::Serialize for HalfInt {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.is_whole() {
            serializer.serialize_i32(self.0 / 2)
        } else {
            serializer.serialize_f64(f64::from(self.0) / 2.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for HalfInt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Amount;
        impl serde::de::Visitor<'_> for Amount {
            type Value = HalfInt;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a whole number or a number ending in .5")
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<HalfInt, E> {
                self.visit_f64(v as f64)
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<HalfInt, E> {
                self.visit_f64(v as f64)
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<HalfInt, E> {
                let doubled = v * 2.0;
                if doubled.fract() != 0.0
                    || doubled < f64::from(i32::MIN)
                    || doubled > f64::from(i32::MAX)
                {
                    return Err(E::custom(format!("{v} is not a whole or a half number")));
                }
                Ok(HalfInt(doubled as i32))
            }
        }
        deserializer.deserialize_any(Amount)
    }
}
