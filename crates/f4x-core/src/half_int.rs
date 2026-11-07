//! Exact fixed-point values with a resolution of one half.
//!
//! HalfInt stores twice the represented value in an i32: raw value 7
//! represents 3.5. This avoids floating-point rounding for game values
//! that are always whole numbers or halves. Arithmetic is supported for
//! addition, subtraction, and negation; convert to f32 only when needed
//! for display or interoperability.
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// A signed fixed-point value whose smallest step is 0.5.
///
/// The inner integer is twice the represented value. Use from_int for whole
/// numbers and from_raw for an already-scaled value.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct HalfInt(i32);

impl HalfInt {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// Creates a value from an integer that has already been doubled.
    ///
    /// For example, raw value 7 represents 3.5.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Creates a value from a whole number.
    pub const fn from_int(value: i32) -> Self {
        Self(value * 2)
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
        (self.0 + 1).div_euclid(2)
    }

    /// Converts this value to a floating-point number, e.g. for display.
    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 2.0
    }
}

impl Add for HalfInt {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for HalfInt {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Neg for HalfInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl AddAssign for HalfInt {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for HalfInt {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
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
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        if abs.is_multiple_of(2) {
            write!(f, "{sign}{}", abs / 2)
        } else {
            write!(f, "{sign}{}.5", abs / 2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HalfInt;

    #[test]
    fn represents_whole_and_half_values_exactly() {
        let value = HalfInt::from_int(3) + HalfInt::from_raw(1);
        assert_eq!(value.raw(), 7);
        assert_eq!(value.as_f32(), 3.5);
        assert!(!value.is_whole());
        assert!(HalfInt::from_int(4).is_whole());
    }

    #[test]
    fn supports_basic_arithmetic_and_negative_values() {
        let a = HalfInt::from_raw(7); // 3.5
        let b = HalfInt::from_int(2); // 2.0

        assert_eq!((a - b).raw(), 3); // 1.5
        assert_eq!((-a).raw(), -7); // -3.5
        assert_eq!((a + b).raw(), 11); // 5.5
    }

    #[test]
    fn rounds_down_and_up_to_whole_numbers() {
        assert_eq!(HalfInt::from_raw(3).floor(), 1);
        assert_eq!(HalfInt::from_raw(3).ceil(), 2);
        assert_eq!(HalfInt::from_raw(4).ceil(), 2);
        assert_eq!(HalfInt::from_raw(-3).floor(), -2);
        assert_eq!(HalfInt::from_raw(-3).ceil(), -1);
    }

    #[test]
    fn sums_assigns_and_shows_halves() {
        let total: HalfInt = [1, 1, 1].into_iter().map(HalfInt::from_raw).sum();
        assert_eq!(total.to_string(), "1.5");
        let mut v = HalfInt::from_int(2);
        v += HalfInt::from_raw(1);
        v -= HalfInt::from_int(1);
        assert_eq!(v.to_string(), "1.5");
        assert_eq!(HalfInt::from_raw(-3).to_string(), "-1.5");
        assert_eq!(HalfInt::from_int(4).to_string(), "4");
    }

    #[test]
    fn is_saved_as_the_raw_number() {
        assert_eq!(ron::to_string(&HalfInt::from_raw(7)).unwrap(), "7");
        assert_eq!(ron::from_str::<HalfInt>("7").unwrap(), HalfInt::from_raw(7));
    }
}
