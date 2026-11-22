//! Loaded definition tables. Definitions refer to each other by number in the content files;
//! `load` validates that every reference resolves, and the registry finds a definition by its
//! number. The alias (`id`) of an entry is a human's aid in the files and is not looked up here.

use super::ids::Id;

/// A definition with a unique string id (its alias for human editing) and its number.
pub trait Def {
    fn id(&self) -> &str;

    /// The definition's number, which is the [`Id`] the game passes around.
    fn num(&self) -> u16;
}

/// The definitions of one kind, in the order of their numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry<T> {
    defs: Vec<T>,
    /// The row of each number (`NO_ROW` where there is none).
    rows: Vec<u16>,
}

const NO_ROW: u16 = u16::MAX;

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            rows: Vec::new(),
        }
    }
}

impl<T: Def> Registry<T> {
    /// Build a registry. The definitions must be in ascending order of their numbers
    /// (content validation reports a repeated alias). Panics when they are not: use
    /// [`Registry::try_new`] for rows that come from a file.
    pub fn new(defs: Vec<T>) -> Self {
        Self::try_new(defs).unwrap_or_else(|problem| panic!("{problem}"))
    }

    /// Build a registry from rows that came from a file: the problem, in words, when they are not
    /// in the order of their numbers (a number twice, or one smaller than the one before) or when
    /// there are more than a registry can hold.
    pub fn try_new(defs: Vec<T>) -> Result<Self, String> {
        if defs.len() >= NO_ROW as usize {
            return Err(format!("too many definitions ({})", defs.len()));
        }
        #[cfg(test)]
        let mut aliases = std::collections::HashSet::new();
        let mut rows: Vec<u16> = Vec::new();
        let mut last: Option<u16> = None;
        for (i, d) in defs.iter().enumerate() {
            let num = d.num();
            if last.is_some_and(|l| num <= l) {
                return Err(format!(
                    "numbers out of order or repeated at {} (number {num})",
                    d.id()
                ));
            }
            last = Some(num);
            // Content validation reports a repeated alias; a test must not build one by accident.
            #[cfg(test)]
            assert!(
                aliases.insert(d.id().to_string()),
                "id repeated in a registry: {}",
                d.id()
            );
            if rows.len() <= num as usize {
                rows.resize(num as usize + 1, NO_ROW);
            }
            rows[num as usize] = i as u16;
        }
        Ok(Self { defs, rows })
    }

    /// A registry of definitions in any order: they are put in the order of their numbers
    /// first. For a test that adds a fixture to the shipped definitions; a content file is
    /// written in order and goes through [`Registry::new`].
    pub fn new_sorted(mut defs: Vec<T>) -> Self {
        defs.sort_by_key(|d| d.num());
        Self::new(defs)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// The definition numbered `id`. Panics for a number the table does not hold; an id made
    /// by another registry is the only way to get one.
    pub fn get(&self, id: Id<T>) -> &T {
        &self.defs[self.row(id).expect("a number this table holds")]
    }

    /// The definition numbered `id`, if the table holds one: for a number that came from a
    /// command or a save rather than from this table.
    pub fn try_get(&self, id: Id<T>) -> Option<&T> {
        self.row(id).map(|row| &self.defs[row])
    }

    /// The id of the definition numbered `num`, if there is one.
    pub fn id_numbered(&self, num: u16) -> Option<Id<T>> {
        let id = Id::new(num);
        self.row(id).map(|_| id)
    }

    /// The definition numbered `num`, if the table holds one.
    pub fn by_num(&self, num: u16) -> Option<&T> {
        self.try_get(Id::new(num))
    }

    /// The definition numbered `num`, to change. For a game that applies its settings to its own
    /// copy of the content (a ship's capacity); the numbers and ids must stay as they are.
    pub fn by_num_mut(&mut self, num: u16) -> Option<&mut T> {
        let row = self.row(Id::new(num))?;
        self.defs.get_mut(row)
    }

    fn row(&self, id: Id<T>) -> Option<usize> {
        match self.rows.get(id.num() as usize) {
            Some(&r) if r != NO_ROW => Some(r as usize),
            _ => None,
        }
    }

    /// Definitions in the order of their numbers (stable, so iteration is deterministic).
    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        let mut nums = self.rows.iter().enumerate().filter(|(_, &r)| r != NO_ROW);
        std::iter::from_fn(move || {
            let (n, &r) = nums.next()?;
            Some((Id::new(n as u16), &self.defs[r as usize]))
        })
    }

    pub fn defs(&self) -> &[T] {
        &self.defs
    }
}
