//! Loaded definition tables. Definitions refer to each other by string id in the
//! content files; `f4x-data` validates that every reference resolves, and the
//! registry turns an id string into a typed [`Id`] once, at the edges.

use std::collections::HashMap;

use super::ids::Id;

/// A definition with a unique string id (its alias for human editing) and the original game's
/// number.
pub trait Def {
    fn id(&self) -> &str;

    /// The definition's number, which is the [`Id`] the game passes around.
    fn num(&self) -> u16;
}

/// The definitions of one kind, in the order of their numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry<T> {
    defs: Vec<T>,
    /// The alias of each definition to its number.
    index: HashMap<String, u16>,
    /// The row of each number (`NO_ROW` where there is none).
    rows: Vec<u16>,
}

const NO_ROW: u16 = u16::MAX;

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            index: HashMap::new(),
            rows: Vec::new(),
        }
    }
}

impl<T: Def> Registry<T> {
    /// Build a registry. The definitions must be in ascending order of their numbers. If an
    /// alias repeats, the first definition wins the lookup (content validation reports
    /// duplicates).
    pub fn new(defs: Vec<T>) -> Self {
        assert!(defs.len() < NO_ROW as usize, "too many definitions");
        let mut index = HashMap::new();
        let mut rows: Vec<u16> = Vec::new();
        let mut last: Option<u16> = None;
        for (i, d) in defs.iter().enumerate() {
            let num = d.num();
            assert!(
                last.is_none_or(|l| num > l),
                "numbers out of order or repeated at {}",
                d.id()
            );
            last = Some(num);
            // Content validation reports a repeated alias; a test must not build one by accident.
            #[cfg(test)]
            assert!(
                !index.contains_key(d.id()),
                "id repeated in a registry: {}",
                d.id()
            );
            index.entry(d.id().to_string()).or_insert(num);
            if rows.len() <= num as usize {
                rows.resize(num as usize + 1, NO_ROW);
            }
            rows[num as usize] = i as u16;
        }
        Self { defs, index, rows }
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

    pub fn id_of(&self, name: &str) -> Option<Id<T>> {
        self.index.get(name).map(|&n| Id::new(n))
    }

    /// The definition numbered `id`. Panics for a number the table does not hold; an id made
    /// by another registry is the only way to get one.
    pub fn get(&self, id: Id<T>) -> &T {
        &self.defs[self.row(id).expect("a number this table holds")]
    }

    /// The id of the definition numbered `num`, if there is one.
    pub fn id_numbered(&self, num: u16) -> Option<Id<T>> {
        let id = Id::new(num);
        self.row(id).map(|_| id)
    }

    fn row(&self, id: Id<T>) -> Option<usize> {
        match self.rows.get(id.num() as usize) {
            Some(&r) if r != NO_ROW => Some(r as usize),
            _ => None,
        }
    }

    /// Look up a definition by its string id.
    pub fn by_name(&self, name: &str) -> Option<&T> {
        self.id_of(name).map(|id| self.get(id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestResult;

    /// The registry is generic over its definition; a bare row is enough to test the table.
    struct Row(&'static str, u16);

    impl Def for Row {
        fn id(&self) -> &str {
            self.0
        }
        fn num(&self) -> u16 {
            self.1
        }
    }

    fn table() -> Registry<Row> {
        Registry::new(vec![Row("a", 1), Row("b", 4), Row("c", 9)])
    }

    #[test]
    fn a_definition_is_found_by_alias_and_by_number() -> TestResult {
        let t = table();
        let b = t.id_of("b").ok_or("b is in the table")?;
        assert_eq!(b.num(), 4);
        assert_eq!(t.get(b).id(), "b");
        assert_eq!(t.id_numbered(9).map(|id| t.get(id).id()), Some("c"));
        assert_eq!(t.by_name("a").map(Def::num), Some(1));
        Ok(())
    }

    #[test]
    fn a_name_or_number_the_table_lacks_is_none() {
        let t = table();
        assert!(t.id_of("z").is_none());
        assert!(t.id_numbered(2).is_none(), "a gap between numbers");
        assert!(t.id_numbered(500).is_none(), "past the last number");
    }

    #[test]
    fn iteration_follows_the_numbers_and_new_sorted_orders_them() {
        let t = Registry::new_sorted(vec![Row("c", 9), Row("a", 1), Row("b", 4)]);
        let order: Vec<_> = t.iter().map(|(id, d)| (id.num(), d.id())).collect();
        assert_eq!(order, [(1, "a"), (4, "b"), (9, "c")]);
        assert_eq!(t.len(), 3);
        assert!(!t.is_empty());
        assert!(Registry::<Row>::default().is_empty());
    }

    #[test]
    #[should_panic(expected = "numbers out of order")]
    fn numbers_out_of_order_are_refused() {
        Registry::new(vec![Row("b", 4), Row("a", 1)]);
    }

    #[test]
    #[should_panic(expected = "id repeated")]
    fn a_repeated_alias_is_refused_in_tests() {
        Registry::new(vec![Row("a", 1), Row("a", 2)]);
    }
}
