//! Loaded definition tables. Definitions refer to each other by string id in the
//! content files; `f4x-data` validates that every reference resolves, and the
//! registry turns an id string into a typed [`Id`] once, at the edges.

use std::collections::HashMap;

use super::ids::Id;

/// A definition with a unique string id.
pub trait Def {
    fn id(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry<T> {
    defs: Vec<T>,
    index: HashMap<String, u16>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T: Def> Registry<T> {
    /// Build a registry. If an id repeats, the first definition wins the lookup
    /// (content validation reports duplicates).
    pub fn new(defs: Vec<T>) -> Self {
        assert!(defs.len() <= u16::MAX as usize, "too many definitions");
        let mut index = HashMap::new();
        for (i, d) in defs.iter().enumerate() {
            // Content validation reports a repeated id; a test must not build one by accident.
            #[cfg(test)]
            assert!(
                !index.contains_key(d.id()),
                "id repeated in a registry: {}",
                d.id()
            );
            index.entry(d.id().to_string()).or_insert(i as u16);
        }
        Self { defs, index }
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn id_of(&self, name: &str) -> Option<Id<T>> {
        self.index.get(name).map(|&i| Id::new(i))
    }

    pub fn get(&self, id: Id<T>) -> &T {
        &self.defs[id.index()]
    }

    /// Look up a definition by its string id.
    pub fn by_name(&self, name: &str) -> Option<&T> {
        self.id_of(name).map(|id| self.get(id))
    }

    /// Definitions in file order (stable, so iteration is deterministic).
    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, d)| (Id::new(i as u16), d))
    }

    pub fn defs(&self) -> &[T] {
        &self.defs
    }
}
