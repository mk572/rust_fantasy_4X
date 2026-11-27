//! A lever of an AI: a number or a switch of `ai_battle.ron` or `ai_strategy.ron`, as a check or
//! an override reaches it, and the macro that declares a group of them.
//!
//! Tests: at the end of this file; those of each file's levers with their structs.

/// One lever, as a check or an override reaches it.
pub enum Lever<'a> {
    /// A weight, a share, a ratio or odds.
    Number(&'a mut f32),
    /// A distance, a count or a number of turns.
    Count(&'a mut u32),
    /// A number that may be below zero.
    Int(&'a mut i32),
    /// A number of things.
    Size(&'a mut usize),
    /// A small number.
    Byte(&'a mut u8),
    /// A big number.
    Long(&'a mut u64),
    /// On or off.
    Switch(&'a mut bool),
}

macro_rules! lever_from {
    ($($kind:ty => $variant:ident,)*) => {
        $(impl<'a> From<&'a mut $kind> for Lever<'a> {
            fn from(v: &'a mut $kind) -> Self {
                Lever::$variant(v)
            }
        })*
    };
}

lever_from!(
    f32 => Number,
    u32 => Count,
    i32 => Int,
    usize => Size,
    u8 => Byte,
    u64 => Long,
    bool => Switch,
);

impl Lever<'_> {
    /// The value as a number (a switch is 1 when on).
    pub fn value(&self) -> f64 {
        match self {
            Lever::Number(v) => f64::from(**v),
            Lever::Count(v) => f64::from(**v),
            Lever::Int(v) => f64::from(**v),
            Lever::Size(v) => **v as f64,
            Lever::Byte(v) => f64::from(**v),
            Lever::Long(v) => **v as f64,
            Lever::Switch(v) => f64::from(u8::from(**v)),
        }
    }

    /// Set the lever to what `text` says, if it is a value of its kind that `allowed` takes. A
    /// switch takes `1`, `0`, `true` and `false`. Nothing changes otherwise.
    pub(super) fn take(
        self,
        text: &str,
        allowed: impl Fn(f64) -> bool,
    ) -> Result<(), OverrideError> {
        fn parsed<T: std::str::FromStr + Copy>(
            text: &str,
            allowed: impl Fn(T) -> bool,
        ) -> Option<T> {
            text.parse().ok().filter(|&v| allowed(v))
        }
        match self {
            Lever::Number(v) => parsed(text, |n: f32| allowed(f64::from(n))).map(|n| *v = n),
            Lever::Count(v) => parsed(text, |n: u32| allowed(f64::from(n))).map(|n| *v = n),
            Lever::Int(v) => parsed(text, |n: i32| allowed(f64::from(n))).map(|n| *v = n),
            Lever::Size(v) => parsed(text, |n: usize| allowed(n as f64)).map(|n| *v = n),
            Lever::Byte(v) => parsed(text, |n: u8| allowed(f64::from(n))).map(|n| *v = n),
            Lever::Long(v) => parsed(text, |n: u64| allowed(n as f64)).map(|n| *v = n),
            Lever::Switch(v) => match text {
                "1" | "true" => Some(true),
                "0" | "false" => Some(false),
                _ => None,
            }
            .map(|n| *v = n),
        }
        .ok_or(OverrideError::BadValue)
    }
}

/// Why an override was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideError {
    /// No lever has the name.
    UnknownLever,
    /// The text is not a value the lever can take.
    BadValue,
}

/// Whether the lever `name` may be `v`: a finite number, at most 1 if it is named in
/// `at_most_one` (a share), above 0 if it is named in `above_zero`. Only the levers that are
/// whole numbers with a sign (`signed`) or are named nowhere else may be below zero.
pub(super) fn allows(
    name: &str,
    v: f64,
    signed: bool,
    at_most_one: &[&str],
    above_zero: &[&str],
) -> bool {
    v.is_finite()
        && (signed || v >= 0.0)
        && (v <= 1.0 || !at_most_one.contains(&name))
        && (v > 0.0 || !above_zero.contains(&name))
}

/// What [`allows`] asks of the lever, for the report.
pub(super) fn wanted(name: &str, at_most_one: &[&str], above_zero: &[&str]) -> &'static str {
    if at_most_one.contains(&name) {
        "a share, 0 to 1"
    } else if above_zero.contains(&name) {
        "a number above 0"
    } else {
        "a number of 0 or more"
    }
}

/// A group of levers: the struct, and `each`, which goes over its levers by name, so that no list
/// of them can leave one out. `Default` (zeros) is for the empty `Rules` only; the file names
/// every lever. A group may also hold lists, which no override reaches.
macro_rules! levers {
    ($(#[$meta:meta])* $group:ident { $($name:ident: $kind:ty,)* }) => {
        $crate::rules::lever::levers!(
            @group [derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize,
                Default)]
            $(#[$meta])* $group { $($name: $kind,)* } {}
        );
    };
    ($(#[$meta:meta])* $group:ident {
        $($name:ident: $kind:ty,)*
    }
    lists { $($(#[$list_meta:meta])* $list:ident: $list_kind:ty,)* }) => {
        $crate::rules::lever::levers!(
            @group [derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize,
                Default)]
            $(#[$meta])* $group { $($name: $kind,)* }
            { $($(#[$list_meta])* $list: $list_kind,)* }
        );
    };
    (@group [$derive:meta] $(#[$meta:meta])* $group:ident { $($name:ident: $kind:ty,)* }
        { $($(#[$list_meta:meta])* $list:ident: $list_kind:ty,)* }) => {
        $(#[$meta])*
        #[$derive]
        #[serde(deny_unknown_fields)]
        pub struct $group {
            $(pub $name: $kind,)*
            $($(#[$list_meta])* pub $list: $list_kind,)*
        }

        impl $group {
            fn each(
                &mut self,
                f: &mut dyn FnMut(&'static str, $crate::rules::lever::Lever<'_>),
            ) {
                $(f(stringify!($name), (&mut self.$name).into());)*
            }
        }
    };
}

pub(crate) use levers;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lever_takes_a_value_of_its_kind_and_keeps_its_own_otherwise() {
        let any = |_| true;
        let mut count = 4u32;
        assert_eq!(Lever::from(&mut count).take("7", any), Ok(()));
        assert_eq!(count, 7);
        for wrong in ["-1", "1.5", "many", ""] {
            assert_eq!(
                Lever::from(&mut count).take(wrong, any),
                Err(OverrideError::BadValue),
                "{wrong}"
            );
        }
        assert_eq!(count, 7);
        let mut switch = true;
        assert_eq!(Lever::from(&mut switch).take("0", any), Ok(()));
        assert!(!switch);
        assert_eq!(
            Lever::from(&mut switch).take("2", any),
            Err(OverrideError::BadValue)
        );
    }

    #[test]
    fn a_value_that_is_not_allowed_is_not_taken() {
        let mut share = 0.5f32;
        let at_most_one = |v| allows("share", v, false, &["share"], &[]);
        assert_eq!(
            Lever::from(&mut share).take("1.5", at_most_one),
            Err(OverrideError::BadValue)
        );
        assert_eq!(share, 0.5);
        assert_eq!(Lever::from(&mut share).take("1", at_most_one), Ok(()));
        assert_eq!(share, 1.0);
    }
}
