//! A table of definitions kept sorted by number, with the number as its id: the one definition
//! behind the terrain and the feature tables (`Terrains`, `Features`). The registries of the rules
//! (`rules::registry::Registry`) have their own typed ids and are separate.
//!
//! Tests: `tests/it/` (`docs/f4x-def-test-design.md`).

use std::marker::PhantomData;

/// A definition that has a number.
pub trait Numbered {
    fn number(&self) -> u16;
}

/// The id of a definition of a [`Table`]: its number.
pub trait TableId: Copy {
    fn from_number(num: u16) -> Self;
    fn number(self) -> u16;
}

/// The loaded definitions, sorted by number; the id of one is its number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table<D, I> {
    defs: Vec<D>,
    ids: PhantomData<fn() -> I>,
}

impl<D, I> Default for Table<D, I> {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            ids: PhantomData,
        }
    }
}

impl<D: Numbered, I: TableId> Table<D, I> {
    pub fn new(mut defs: Vec<D>) -> Self {
        defs.sort_by_key(|d| d.number());
        Self {
            defs,
            ids: PhantomData,
        }
    }

    pub fn defs(&self) -> &[D] {
        &self.defs
    }

    /// The definitions, to put the names of a language on them. Their numbers stay as they are.
    pub fn defs_mut(&mut self) -> &mut [D] {
        &mut self.defs
    }

    /// The definition with the number of `id`.
    pub fn get(&self, id: I) -> Option<&D> {
        // The numbers of a table that has no gaps run from 0 (the shipped terrains): the slot of
        // the number is the definition; the search is for a table that has gaps.
        let num = id.number();
        if let Some(d) = self
            .defs
            .get(usize::from(num))
            .filter(|d| d.number() == num)
        {
            return Some(d);
        }
        self.defs
            .binary_search_by_key(&num, |d| d.number())
            .ok()
            .map(|i| &self.defs[i])
    }

    /// The id of the definition numbered `num` (what a save and the name tables call it), if the
    /// table holds one.
    pub fn id_numbered(&self, num: u16) -> Option<I> {
        let id = I::from_number(num);
        self.get(id).map(|_| id)
    }

    /// The id of the first definition for which `wanted` holds.
    pub fn first_where(&self, wanted: impl Fn(&D) -> bool) -> Option<I> {
        self.defs
            .iter()
            .find(|d| wanted(d))
            .map(|d| I::from_number(d.number()))
    }
}
