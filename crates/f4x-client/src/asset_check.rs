//! The client's asset tables (`crates/f4x-client/content/spell_art.ron`, `unit_art.ron`, `sound.ron`) and their check
//! against the rules: every id they name must be a spell, unit or realm the rules have.
//!
//! The tables themselves, and their checks that need no rules, live in `f4x-extract`, which the
//! extractor shares; only this check needs the game's rules, so it is the client's.

use std::collections::BTreeSet;
use std::path::Path;

use f4x_def::rules::Rules;
use f4x_extract::sound::{self, SoundTable};
use f4x_extract::spell_art::{self, Role, SpellArt};
use f4x_extract::unit_art::{self, UnitArt};
use f4x_extract::wizard_art::{self, WizardArt};

use crate::icon_art::{self, IconArt};
use crate::names::Names;

/// The asset tables of the client.
pub struct Tables {
    pub spells: SpellArt,
    pub units: Vec<UnitArt>,
    pub wizards: Vec<WizardArt>,
    pub sounds: SoundTable,
    pub icons: IconArt,
    pub names: Names,
}

/// The tables from the first of `dirs` that has each, checked against `rules`.
pub fn load_tables(dirs: &[&Path], rules: &Rules, language: &str) -> Result<Tables, String> {
    let spells = spell_art::load_spell_art(dirs)?;
    let units = unit_art::load_unit_art(dirs)?;
    let wizards = wizard_art::load_wizard_art(dirs)?;
    let sounds = sound::load_sound(dirs)?;
    let icons = icon_art::load_icon_art(dirs)?;
    let names = Names::load(dirs, language)?;
    let mut problems = check_spell_art(&spells, rules);
    problems.extend(check_unit_art(&units, rules));
    problems.extend(check_wizard_art(&wizards, rules));
    problems.extend(check_sound(&sounds, rules));
    problems.extend(check_icon_art(&icons, rules));
    problems.extend(names.problems(rules));
    if problems.is_empty() {
        Ok(Tables {
            spells,
            units,
            wizards,
            sounds,
            icons,
            names,
        })
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
        .filter(|id| crate::alias::id_of(&rules.spells, id).is_none())
    {
        problems.push(format!("spell_art: unknown spell \"{id}\""));
    }
    for p in &art.shared {
        if let Role::Realm(realm) = &p.role {
            if crate::alias::id_of(&rules.realms, realm).is_none() {
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
    for u in units
        .iter()
        .filter(|u| crate::alias::id_of(&rules.units, &u.id).is_none())
    {
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

/// The table lists the ready-made wizards of the rules, in their order and with a slot for each
/// portrait the game keeps.
pub fn check_wizard_art(wizards: &[WizardArt], rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    let ids: Vec<&str> = rules.wizards.defs().iter().map(|w| w.id.as_str()).collect();
    let table: Vec<&str> = wizards.iter().map(|w| w.id.as_str()).collect();
    if ids != table {
        problems.push(format!(
            "wizard_art: the wizards are {table:?}, the rules have {ids:?}"
        ));
    }
    if wizards.len() != usize::from(f4x_core::player::PORTRAITS) {
        problems.push(format!(
            "wizard_art: {} wizards, the game keeps {} portraits",
            wizards.len(),
            f4x_core::player::PORTRAITS
        ));
    }
    problems
}

/// The items of the table are the rules' items, in the rules' order, each with a picture of its
/// type; the battle information lines are lines the window has.
pub fn check_icon_art(art: &IconArt, rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    let ids: Vec<&str> = rules.items.defs().iter().map(|i| i.id.as_str()).collect();
    let table: Vec<&str> = art.items.iter().map(|i| i.id.as_str()).collect();
    if ids != table {
        problems.push(format!(
            "icon_art: {} items, the rules have {} (or not in the same order)",
            table.len(),
            ids.len()
        ));
        return problems;
    }
    for (def, item) in rules.items.defs().iter().zip(&art.items) {
        let (first, last) = def.kind.icon_range();
        // The original draws its "White Lightning I" sword with a mace's picture.
        if def.id != "white_lightning_i" && !(first..=last).contains(&item.icon) {
            problems.push(format!(
                "icon_art: {} ({:?}) has picture {}, its type has {first} to {last}",
                def.id, def.kind, item.icon
            ));
        }
    }
    let keys: Vec<&str> = f4x_core::battle::info::ALL.iter().map(|i| i.key).collect();
    for key in art
        .battle_info
        .keys()
        .filter(|k| !keys.contains(&k.as_str()))
    {
        problems.push(format!(
            "icon_art: unknown battle information line \"{key}\""
        ));
    }
    problems
}

/// A spell or unit of the table that the rules do not have.
pub fn check_sound(table: &SoundTable, rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    for id in table.spells.keys().chain(table.global_music.keys()) {
        if crate::alias::id_of(&rules.spells, id).is_none() {
            problems.push(format!("sound: unknown spell \"{id}\""));
        }
    }
    for id in table.units.keys() {
        if crate::alias::id_of(&rules.units, id).is_none() {
            problems.push(format!("sound: unknown unit \"{id}\""));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app::shipped;
    use f4x_def::testdir::TestResult;

    #[test]
    fn the_shipped_tables_agree_with_the_shipped_rules() -> TestResult {
        let rules = &shipped::content().rules;
        let tables = load_tables(&[Path::new(crate::test_app::CLIENT_CONTENT)], rules, "en")?;
        assert_eq!(tables.icons.items.len(), rules.items.len());
        assert_eq!(tables.icons.battle_info.len(), 14);
        assert_eq!(tables.sounds.units.len(), rules.units.len());
        assert_eq!(tables.units.len(), rules.units.len());
        assert_eq!(tables.wizards.len(), rules.wizards.len());
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
        let wizards = wizard_art::parse_wizard_art(r#"[(id: "nobody", pictures: ())]"#)?;
        assert_eq!(check_wizard_art(&wizards, rules).len(), 2);
        let icons =
            icon_art::parse_icon_art(r#"(items: [], battle_info: {"battle_info.nothing": 42})"#)?;
        assert_eq!(check_icon_art(&icons, rules).len(), 1);
        Ok(())
    }
}
