//! The original's own tables, for the tests that hold the shipped content against them.
//! `tests/original/*.tsv` are the original's tables as `scripts/export_original_*.py` read them
//! from the ReMoM clone, in the original's row order; the content rows are in the same order. A
//! number of the content may differ from the original only where a list of departures says so,
//! with the reason.
//!
//! Tests: in original/tests.rs (the content against the tables). The spells, the retorts and the
//! ready-made wizards are checked in `f4x-core`, which they need.

use std::collections::BTreeMap;

pub const UNITS: &str = include_str!("../tests/original/units.tsv");
pub const BUILDINGS: &str = include_str!("../tests/original/buildings.tsv");
pub const ITEM_POWERS: &str = include_str!("../tests/original/item_powers.tsv");
pub const ITEMS: &str = include_str!("../tests/original/items.tsv");
pub const RACES: &str = include_str!("../tests/original/races.tsv");
pub const SPELLS: &str = include_str!("../tests/original/spells.tsv");

/// The rows of a TSV table as column name to value, one map per row.
pub fn table(text: &str) -> Vec<BTreeMap<String, i32>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    lines
        .map(|line| {
            head.iter()
                .zip(line.split('\t'))
                .map(|(name, v)| (name.to_string(), v.parse().expect("a number")))
                .collect()
        })
        .collect()
}

/// The rows of a TSV table with a name column, as the cells of each row by column name.
pub fn named_table(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    lines
        .map(|line| {
            head.iter()
                .zip(line.split('\t'))
                .map(|(name, v)| (name.to_string(), v.to_string()))
                .collect()
        })
        .collect()
}

pub fn number(row: &BTreeMap<String, String>, column: &str) -> i64 {
    row[column].parse().expect("a number")
}

/// The realms in the original's numbering.
pub const REALMS: [&str; 6] = ["nature", "sorcery", "chaos", "life", "death", "arcane"];

/// Letters and digits in lower case, to compare names that differ in an apostrophe.
pub fn plain(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests;
