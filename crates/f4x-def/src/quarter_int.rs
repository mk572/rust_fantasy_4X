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
        (self.0 + 3).div_euclid(4)
    }

    /// The value times `percent` over 100, rounded down to a quarter: 1.5 at 150 is 2.25.
    pub const fn percent(self, percent: i32) -> Self {
        Self((self.0 * percent).div_euclid(100))
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
                if scaled.fract() != 0.0 || scaled.abs() > f64::from(i32::MAX) {
                    return Err(E::custom(format!("{v} is not a multiple of 0.25")));
                }
                Ok(QuarterInt(scaled as i32))
            }
        }
        deserializer.deserialize_any(Amount)
    }
}

#[cfg(test)]
mod tests {
    use super::QuarterInt;
    use crate::half_int::HalfInt;

    #[test]
    fn represents_wholes_halves_and_quarters_exactly() {
        let value = QuarterInt::from_int(1) + QuarterInt::from_raw(3);
        assert_eq!(value.raw(), 7);
        assert_eq!(value.as_f32(), 1.75);
        assert!(!value.is_whole());
        assert!(QuarterInt::from_int(2).is_whole());
        assert_eq!(QuarterInt::from_half(HalfInt::from_raw(3)).raw(), 6);
    }

    #[test]
    fn half_of_a_half_int_is_exact() {
        assert_eq!(QuarterInt::half_of(HalfInt::HALF), QuarterInt::QUARTER);
        assert_eq!(QuarterInt::half_of(HalfInt::from_int(3)).as_f32(), 1.5);
        assert_eq!(QuarterInt::half_of(HalfInt::ZERO), QuarterInt::ZERO);
    }

    #[test]
    fn rounds_down_to_halves_and_to_wholes_and_up_to_wholes() {
        let q = QuarterInt::from_raw;
        assert_eq!(q(7).floor_half(), HalfInt::from_raw(3));
        assert_eq!(q(1).floor_half(), HalfInt::ZERO);
        assert_eq!(q(2).floor_half(), HalfInt::HALF);
        assert_eq!(q(-1).floor_half(), HalfInt::from_raw(-1));
        assert_eq!((q(7).floor(), q(8).floor(), q(-1).floor()), (1, 2, -1));
        assert_eq!((q(5).ceil(), q(4).ceil(), q(-3).ceil()), (2, 1, 0));
    }

    #[test]
    fn two_shared_halves_of_food_add_up_to_a_half() {
        let shared: QuarterInt = [HalfInt::HALF; 2]
            .into_iter()
            .map(QuarterInt::half_of)
            .sum();
        assert_eq!(shared.floor_half(), HalfInt::HALF);
        let one: QuarterInt = [HalfInt::HALF; 1]
            .into_iter()
            .map(QuarterInt::half_of)
            .sum();
        assert_eq!(one.floor_half(), HalfInt::ZERO);
    }

    #[test]
    fn percent_rounds_down_to_a_quarter() {
        assert_eq!(
            QuarterInt::from_raw(6).percent(150),
            QuarterInt::from_raw(9)
        );
        assert_eq!(
            QuarterInt::from_raw(5).percent(150),
            QuarterInt::from_raw(7)
        );
    }

    #[test]
    fn sums_assigns_and_shows_quarters() {
        let total: QuarterInt = [1, 1, 1].into_iter().map(QuarterInt::from_raw).sum();
        assert_eq!(total.to_string(), "0.75");
        let mut v = QuarterInt::from_int(2);
        v += QuarterInt::from_raw(2);
        v -= QuarterInt::QUARTER;
        assert_eq!(v.to_string(), "2.25");
        assert_eq!(QuarterInt::from_raw(6).to_string(), "1.5");
        assert_eq!(QuarterInt::from_int(4).to_string(), "4");
        assert_eq!(QuarterInt::from_raw(-3).to_string(), "-0.75");
        assert_eq!(format!("{:+}", QuarterInt::from_raw(1)), "+0.25");
        assert_eq!(
            QuarterInt::from_raw(4) + HalfInt::HALF,
            QuarterInt::from_raw(6)
        );
    }

    #[test]
    fn is_written_and_read_as_the_amount() {
        assert_eq!(ron::to_string(&QuarterInt::from_int(1)).unwrap(), "1");
        assert_eq!(ron::to_string(&QuarterInt::from_raw(3)).unwrap(), "0.75");
        for (text, raw) in [
            ("1", 4),
            ("0.25", 1),
            ("0.5", 2),
            ("1.75", 7),
            ("-1.25", -5),
        ] {
            assert_eq!(
                ron::from_str::<QuarterInt>(text).unwrap().raw(),
                raw,
                "{text}"
            );
        }
        for text in ["0.3", "1.1", "\"1\""] {
            assert!(ron::from_str::<QuarterInt>(text).is_err(), "{text}");
        }
    }
}
