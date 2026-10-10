//! Typed ids: an index into a [`super::registry::Registry`] of one definition type.

use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct Id<T> {
    index: u16,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub(crate) fn new(index: u16) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    pub fn index(self) -> usize {
        self.index as usize
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
        self.index == other.index
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
        self.index.cmp(&other.index)
    }
}
impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.index)
    }
}
impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.index.serialize(s)
    }
}
impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        u16::deserialize(d).map(Id::new)
    }
}
