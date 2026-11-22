//! The rows of the content that this crate names by number, with the alias each number stands for.
//!
//! The wizard rules, the temperaments and the setup say "the Myrran retort" as `Retort::new(9)`
//! (the memory rule is the number with its alias as a comment: the number is what a save and the
//! content use). If the content renumbers or drops a row the code goes on with a number that now
//! means another retort, or none, and the game starts all the same. [`problems`] is the list of
//! the numbers that no longer lead to the row of their alias; `setup::new_game` refuses the content
//! with it, and the tests hold the list to the code (`tests/it/c6_coupling.rs`).

use f4x_core::rules::Rules;

/// The retorts the crate names: number and alias.
pub const RETORTS: &[(u16, &str)] = &[
    (0, "alchemy"),
    (1, "warlord"),
    (2, "chaos_mastery"),
    (3, "nature_mastery"),
    (4, "sorcery_mastery"),
    (5, "infernal_power"),
    (6, "divine_power"),
    (8, "channeler"),
    (9, "myrran"),
    (10, "archmage"),
    (15, "conjurer"),
    (16, "charismatic"),
];

/// The realms the crate names: number and alias.
pub const REALMS: &[(u16, &str)] = &[
    (0, "nature"),
    (1, "sorcery"),
    (2, "chaos"),
    (3, "life"),
    (4, "death"),
];

/// Every number of [`RETORTS`] and [`REALMS`] that does not lead to the row of its alias, in words
/// that name the number, the alias and what is there.
pub fn problems(rules: &Rules) -> Vec<String> {
    let mut bad = Vec::new();
    for &(num, alias) in RETORTS {
        let found = rules.retorts.defs().iter().find(|d| d.num == num);
        if found.map(|d| d.id.as_str()) != Some(alias) {
            bad.push(format!(
                "the code uses retort {num} as {alias}; the content has {}",
                found.map_or("no such retort", |d| d.id.as_str())
            ));
        }
    }
    for &(num, alias) in REALMS {
        let found = rules.realms.defs().iter().find(|d| d.num == num);
        if found.map(|d| d.id.as_str()) != Some(alias) {
            bad.push(format!(
                "the code uses realm {num} as {alias}; the content has {}",
                found.map_or("no such realm", |d| d.id.as_str())
            ));
        }
    }
    bad
}
