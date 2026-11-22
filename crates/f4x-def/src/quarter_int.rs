//! Exact fixed-point values with a resolution of one quarter.
//!
//! QuarterInt stores four times the represented value in an i32: raw value 7 represents 1.75. In a
//! file it is the amount itself: `1`, `0.5`, `0.25`, `1.75`. It is the type of an amount that is
//! half of a [`HalfInt`] (a square shared by two cities gives half of its food, and half of 0.5
//! food is 0.25), so that such an amount stays exact until it is added up. Convert back with
//! [`QuarterInt::floor_half`] or [`QuarterInt::floor`].
use crate::half_int::HalfInt;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// A signed fixed-point value whose smallest step is 0.25.
///
/// The inner integer is four times the represented value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuarterInt(i32);

impl QuarterInt {
    /// The value zero.
    pub const ZERO: Self = Self(0);

    /// One quarter.
    pub const QUARTER: Self = Self(1);

    /// Creates a value from an integer that has already been multiplied by four.
    ///
    /// For example, raw value 7 represents 1.75.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Creates a value from a whole number.
    pub const fn from_int(value: i32) -> Self {
        Self(value.saturating_mul(4))
    }

    /// The same amount as a `HalfInt`, exact.
    pub const fn from_half(value: HalfInt) -> Self {
        Self(value.raw().saturating_mul(2))
    }

    /// Half of a `HalfInt`, exact: half of 0.5 is 0.25, half of 3 is 1.5.
    pub const fn half_of(value: HalfInt) -> Self {
        Self(value.raw())
    }

    /// Returns the quadrupled integer representation.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Returns whether this value represents a whole number.
    pub const fn is_whole(self) -> bool {
        self.0 % 4 == 0
    }

    /// The value rounded down to a half (towards negative infinity): 1.75 is 1.5, 0.25 is 0.
    pub const fn floor_half(self) -> HalfInt {
        HalfInt::from_raw(self.0.div_euclid(2))
    }

    /// The value rounded down to a whole number (towards negative infinity).
    pub const fn floor(self) -> i32 {
        self.0.div_euclid(4)
    }

    /// The value rounded up to a whole number (towards positive infinity): 1.25 is 2.
    pub const fn ceil(self) -> i32 {
        (self.0 as i64 + 3).div_euclid(4) as i32
    }

    /// The value times `percent` over 100, rounded down to a quarter: 1.5 at 150 is 2.25. A result
    /// the type cannot hold is the nearest value it can.
    pub const fn percent(self, percent: i32) -> Self {
        let raw = (self.0 as i64 * percent as i64).div_euclid(100);
        Self(if raw > i32::MAX as i64 {
            i32::MAX
        } else if raw < i32::MIN as i64 {
            i32::MIN
        } else {
            raw as i32
        })
    }

    /// Converts this value to a floating-point number, e.g. for display.
    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 4.0
    }
}

impl From<HalfInt> for QuarterInt {
    fn from(value: HalfInt) -> Self {
        Self::from_half(value)
    }
}

impl Add for QuarterInt {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for QuarterInt {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Neg for QuarterInt {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Add<HalfInt> for QuarterInt {
    type Output = Self;

    fn add(self, rhs: HalfInt) -> Self::Output {
        self + Self::from_half(rhs)
    }
}

impl AddAssign for QuarterInt {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for QuarterInt {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Sum for QuarterInt {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

/// Shows the amount as `3`, `3.25`, `3.5` or `3.75`.
impl fmt::Display for QuarterInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = match (self.0 < 0, f.sign_plus()) {
            (true, _) => "-",
            (false, true) => "+",
            (false, false) => "",
        };
        let abs = self.0.unsigned_abs();
        let fraction = match abs % 4 {
            0 => "",
            1 => ".25",
            2 => ".5",
            _ => ".75",
        };
        write!(f, "{sign}{}{fraction}", abs / 4)
    }
}

/// Written and read as the amount itself, not the quadrupled count. A number that is not a
/// multiple of 0.25 is an error.
impl serde::Serialize for QuarterInt {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.is_whole() {
            serializer.serialize_i32(self.0 / 4)
        } else {
            serializer.serialize_f64(f64::from(self.0) / 4.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for QuarterInt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Amount;
        impl serde::de::Visitor<'_> for Amount {
            type Value = QuarterInt;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number that is a multiple of 0.25")
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<QuarterInt, E> {
                self.visit_f64(v as f64)
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<QuarterInt, E> {
                self.visit_f64(v as f64)
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<QuarterInt, E> {
                let scaled = v * 4.0;
                if scaled.fract() != 0.0
                    || scaled < f64::from(i32::MIN)
                    || scaled > f64::from(i32::MAX)
                {
                    return Err(E::custom(format!("{v} is not a multiple of 0.25")));
                }
                Ok(QuarterInt(scaled as i32))
            }
        }
        deserializer.deserialize_any(Amount)
    }
}
