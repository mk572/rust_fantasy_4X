//! Exact fixed-point values with a resolution of one half.
//!
//! HalfInt stores twice the represented value in an i32: raw value 7
//! represents 3.5. This avoids floating-point rounding for game values
//! that are always whole numbers or halves. Arithmetic is supported for
//! addition, subtraction, and negation; convert to f32 only when needed
//! for display or interoperability.
use std::ops::{Add, Neg, Sub};

/// A signed fixed-point value whose smallest step is 0.5.
///
/// The inner integer is twice the represented value. Use from_int for whole
/// numbers and from_raw for an already-scaled value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}
