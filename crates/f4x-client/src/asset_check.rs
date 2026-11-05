//! The client's asset tables (`content/spell_art.ron`, `unit_art.ron`, `sound.ron`) and their check
//! against the rules: every id they name must be a spell, unit or realm the rules have.
//!
//! The tables themselves, and their checks that need no rules, live in `f4x-extract`, which the
//! extractor shares; only this check needs the game's rules, so it is the client's.

use std::collections::BTreeSet;
use std::path::Path;

use f4x_core::rules::Rules;
use f4x_extract::sound::{self, SoundTable};
use f4x_extract::spell_art::{self, Role, SpellArt};
use f4x_extract::unit_art::{self, UnitArt};

/// The three tables from the first of `dirs` that has each, checked against `rules`.
pub fn load_tables(
    dirs: &[&Path],
    rules: &Rules,
) -> Result<(SpellArt, Vec<UnitArt>, SoundTable), String> {
    let spells = spell_art::load_spell_art(dirs)?;
    let units = unit_art::load_unit_art(dirs)?;
    let sounds = sound::load_sound(dirs)?;
    let mut problems = check_spell_art(&spells, rules);
    problems.extend(check_unit_art(&units, rules));
    problems.extend(check_sound(&sounds, rules));
    if problems.is_empty() {
        Ok((spells, units, sounds))
    } else {
        Err(format!("\n  - {}", problems.join("\n  - ")))
    }
}

/// A spell or a realm of the table that the rules do not have.
pub fn check_spell_art(art: &SpellArt, rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    for id in art
        .spells
        .keys()
        .filter(|id| rules.spells.id_of(id).is_none())
    {
        problems.push(format!("spell_art: unknown spell \"{id}\""));
    }
    for p in &art.shared {
        if let Role::Realm(realm) = &p.role {
            if rules.realms.id_of(realm).is_none() {
                problems.push(format!("spell_art: unknown realm \"{realm}\""));
            }
        }
    }
    problems
}

/// A unit the rules do not have, or a rules unit with no entry; a hero needs a portrait.
pub fn check_unit_art(units: &[UnitArt], rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    let named: BTreeSet<&str> = units.iter().map(|u| u.id.as_str()).collect();
    for u in units.iter().filter(|u| rules.units.id_of(&u.id).is_none()) {
        problems.push(format!("unit_art: unknown unit \"{}\"", u.id));
    }
    for def in rules.units.defs() {
        if !named.contains(def.id.as_str()) {
            problems.push(format!("unit_art: no entry for unit \"{}\"", def.id));
        }
        let has_portrait = units.iter().any(|u| u.id == def.id && u.portrait.is_some());
        if def.hero.is_some() && !has_portrait {
            problems.push(format!("unit_art: hero \"{}\" has no portrait", def.id));
        }
    }
    problems
}

/// A spell or unit of the table that the rules do not have.
pub fn check_sound(table: &SoundTable, rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    for id in table.spells.keys().chain(table.global_music.keys()) {
        if rules.spells.id_of(id).is_none() {
            problems.push(format!("sound: unknown spell \"{id}\""));
        }
    }
    for id in table.units.keys() {
        if rules.units.id_of(id).is_none() {
            problems.push(format!("sound: unknown unit \"{id}\""));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app::shipped;
    use f4x_core::testdir::TestResult;

    #[test]
    fn the_shipped_tables_agree_with_the_shipped_rules() -> TestResult {
        let rules = &shipped::content().rules;
        let (_, units, sounds) = load_tables(&[Path::new(crate::test_app::CONTENT)], rules)?;
        assert_eq!(sounds.units.len(), rules.units.len());
        assert_eq!(units.len(), rules.units.len());
        Ok(())
    }

    #[test]
    fn an_unknown_spell_unit_or_realm_is_reported() -> TestResult {
        let rules = &shipped::content().rules;
        let spells = spell_art::parse_spell_art(
            r#"(shared: [(lbx: "specfx", entry: 1, role: Realm("no_realm"))],
                spells: {"no_such_spell": [(lbx: "specfx", entry: 0, role: Cast)]})"#,
        )?;
        assert_eq!(check_spell_art(&spells, rules).len(), 2);
        let sounds = sound::parse_sound(
            r#"(spells: {"no_such_spell": 1}, units: {"nobody": (move: 1, melee: 1)})"#,
        )?;
        assert_eq!(check_sound(&sounds, rules).len(), 2);
        let units = unit_art::parse_unit_art(r#"[(id: "nobody", art: None, portrait: None)]"#)?;
        assert!(check_unit_art(&units, rules)
            .iter()
            .any(|p| p.contains("unknown unit")));
        Ok(())
    }
}
