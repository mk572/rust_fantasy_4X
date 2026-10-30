//! Typed ids: the number of a definition in a [`super::registry::Registry`] of one definition type
//! (the original game's number, kept in the content file; there is no other numbering).
//!
//! Tests: none, because it only declares the typed id and its trait impls.

use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct Id<T> {
    num: u16,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub(crate) fn new(num: u16) -> Self {
        Self {
            num,
            _marker: PhantomData,
        }
    }

    /// The definition's number.
    pub fn num(self) -> u16 {
        self.num
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.num == other.num
    }
}
impl<T> Eq for Id<T> {}
impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.num.cmp(&other.num)
    }
}
impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.num.hash(state);
    }
}
impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.num)
    }
}
impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.num.serialize(s)
    }
}
impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        u16::deserialize(d).map(Id::new)
    }
}
