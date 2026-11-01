//! A reference from one definition to another: the author's name in the content file, and the
//! number it stands for, made once when the rules load.
//!
//! The content files name things by alias (`race: Some("high_men")`). [`Ref`] keeps that name
//! (it is what is written back, and what an error message shows) and holds the [`Id`] after
//! [`Names::resolve`] has looked it up, so the game asks `id()` and never compares strings.
//! A name that is no definition stays unresolved (`id()` is `None`); content validation
//! reports it with the file and the entry.
//!
//! Tests: in refs/tests.rs.

use std::any::TypeId;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ids::Id;
use super::registry::Def;

/// A named reference to a definition of kind `T`. Equal when the names are.
pub struct Ref<T> {
    name: String,
    id: Option<Id<T>>,
}

impl<T> Ref<T> {
    /// A reference that is not resolved yet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: None,
        }
    }

    /// The author's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The definition's number; `None` until the rules are loaded, or for an unknown name.
    pub fn id(&self) -> Option<Id<T>> {
        self.id
    }

    /// Set the number (the loader does; a test building rules by hand may too).
    pub fn set(&mut self, id: Option<Id<T>>) {
        self.id = id;
    }
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            id: self.id,
        }
    }
}

impl<T> PartialEq for Ref<T> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl<T> Eq for Ref<T> {}

impl<T> std::fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.name)
    }
}

impl<T> From<&str> for Ref<T> {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl<T> Serialize for Ref<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.name.serialize(s)
    }
}

impl<'de, T> Deserialize<'de> for Ref<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(Ref::new)
    }
}

/// The numbers of a list of references, leaving out a name that is no definition.
pub fn ids<T>(list: &[Ref<T>]) -> Vec<Id<T>> {
    list.iter().filter_map(Ref::id).collect()
}

/// The names of every kind of definition and their numbers, for [`Ref`]s to be resolved
/// against. Made from the tables before they become registries.
#[derive(Default)]
pub struct Names {
    kinds: HashMap<TypeId, HashMap<String, u16>>,
}

impl Names {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a kind of definition.
    pub fn add<T: Def + 'static>(&mut self, defs: &[T]) {
        let table = self.kinds.entry(TypeId::of::<T>()).or_default();
        for d in defs {
            // The first of a repeated alias wins, as in `Registry`.
            table.entry(d.id().to_string()).or_insert_with(|| d.num());
        }
    }

    /// Set the number of one reference.
    pub fn resolve<T: Def + 'static>(&self, r: &mut Ref<T>) {
        let num = self
            .kinds
            .get(&TypeId::of::<T>())
            .and_then(|table| table.get(r.name()));
        r.set(num.map(|&n| Id::new(n)));
    }

    /// Set the numbers of a list of references.
    pub fn resolve_all<T: Def + 'static>(&self, list: &mut [Ref<T>]) {
        for r in list {
            self.resolve(r);
        }
    }

    /// Set the number of a reference that may be missing.
    pub fn resolve_opt<T: Def + 'static>(&self, r: &mut Option<Ref<T>>) {
        if let Some(r) = r {
            self.resolve(r);
        }
    }
}

#[cfg(test)]
mod tests;
