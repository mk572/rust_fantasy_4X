//! What the interface calls each thing of the rules, by the thing's number (`num`): the tables
//! `text/<language>/name/*.ron` of the client's content. The rules carry no display name and
//! nothing below the client reads one; the client looks the name up here when it draws.
//! A language that lacks a table or a number falls back to English.

use std::collections::BTreeMap;
use std::path::Path;

use f4x_def::names::NameBundle;
use f4x_def::rules::Rules;
use f4x_def::text::{Gender, FALLBACK_LANGUAGE};
use serde::de::DeserializeOwned;

type Table<V> = BTreeMap<u16, V>;

/// The names of one language over those of the fallback language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    unit: Table<String>,
    ability: Table<String>,
    building: Table<String>,
    race: Table<(String, String)>,
    realm: Table<String>,
    retort: Table<String>,
    wizard: Table<String>,
    spell: Table<String>,
    item: Table<String>,
    item_power: Table<String>,
    terrain: Table<String>,
    feature: Table<String>,
    /// The levels of a level path in order, then its bonus levels.
    level_path: Table<Vec<String>>,
    /// The city names of a race, in the order they are handed out.
    city: Table<Vec<String>>,
    /// The names of a hero unit; the first is the name the hero takes.
    hero: Table<Vec<String>>,
    /// The rival wizards' names, one list for each slot (a list of the table is a slot).
    rival: Vec<Vec<(String, Gender)>>,
}

/// The table `text/<lang>/name/<file>.ron` of the first of `dirs` that has it, over the same
/// table of the fallback language.
fn table<V: DeserializeOwned>(dirs: &[&Path], lang: &str, file: &str) -> Result<Table<V>, String> {
    let mut all = Table::new();
    for l in [FALLBACK_LANGUAGE, lang] {
        let rel = format!("text/{l}/name/{file}.ron");
        let Some(path) = dirs.iter().map(|d| d.join(&rel)).find(|p| p.exists()) else {
            continue;
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let rows: Table<V> =
            ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        all.extend(rows);
    }
    Ok(all)
}

/// The rival wizards' lists: `text/<lang>/name/rival.ron` of the first directory that has it,
/// else the fallback language's.
fn rival_lists(dirs: &[&Path], lang: &str) -> Result<Vec<Vec<(String, Gender)>>, String> {
    for l in [lang, FALLBACK_LANGUAGE] {
        let rel = format!("text/{l}/name/rival.ron");
        let Some(path) = dirs.iter().map(|d| d.join(&rel)).find(|p| p.exists()) else {
            continue;
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        return ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()));
    }
    Ok(Vec::new())
}

impl Names {
    /// The names in `lang`, from the first of `dirs` that has each table.
    pub fn load(dirs: &[&Path], lang: &str) -> Result<Self, String> {
        Ok(Self {
            unit: table(dirs, lang, "unit")?,
            ability: table(dirs, lang, "ability")?,
            building: table(dirs, lang, "building")?,
            race: table(dirs, lang, "race")?,
            realm: table(dirs, lang, "realm")?,
            retort: table(dirs, lang, "retort")?,
            wizard: table(dirs, lang, "wizard")?,
            spell: table(dirs, lang, "spell")?,
            item: table(dirs, lang, "item")?,
            item_power: table(dirs, lang, "item_power")?,
            terrain: table(dirs, lang, "terrain")?,
            feature: table(dirs, lang, "feature")?,
            level_path: table(dirs, lang, "level_path")?,
            city: table(dirs, lang, "city")?,
            hero: table(dirs, lang, "hero")?,
            rival: rival_lists(dirs, lang)?,
        })
    }

    /// The name lists a new game draws from (`f4x_core::game::setup::NewGame::names`).
    pub fn bundle(&self) -> NameBundle {
        NameBundle {
            cities: self.city.clone(),
            heroes: self.hero.clone(),
            rivals: self.rival.clone(),
        }
    }
}

/// A name by number; a number the table lacks gives "" (the start-up check, `asset_check`,
/// reports every number of the rules that has no name).
fn get(table: &Table<String>, num: u16) -> &str {
    table.get(&num).map_or("", String::as_str)
}

impl Names {
    pub fn unit(&self, num: u16) -> &str {
        get(&self.unit, num)
    }
    pub fn ability(&self, num: u16) -> &str {
        get(&self.ability, num)
    }
    pub fn building(&self, num: u16) -> &str {
        get(&self.building, num)
    }
    pub fn realm(&self, num: u16) -> &str {
        get(&self.realm, num)
    }
    pub fn retort(&self, num: u16) -> &str {
        get(&self.retort, num)
    }
    pub fn wizard(&self, num: u16) -> &str {
        get(&self.wizard, num)
    }
    pub fn spell(&self, num: u16) -> &str {
        get(&self.spell, num)
    }
    pub fn item(&self, num: u16) -> &str {
        get(&self.item, num)
    }
    pub fn item_power(&self, num: u16) -> &str {
        get(&self.item_power, num)
    }
    pub fn terrain(&self, num: u16) -> &str {
        get(&self.terrain, num)
    }
    pub fn feature(&self, num: u16) -> &str {
        get(&self.feature, num)
    }

    /// A race's name.
    pub fn race(&self, num: u16) -> &str {
        self.race.get(&num).map_or("", |r| r.0.as_str())
    }

    /// A race's name for more than one ("Barbarians"); its name when the table has no plural.
    pub fn race_plural(&self, num: u16) -> &str {
        match self.race.get(&num) {
            Some((name, plural)) if plural.is_empty() => name,
            Some((_, plural)) => plural,
            None => "",
        }
    }

    /// The name of level `index` (0-based, bonus levels last) of a level path.
    pub fn level(&self, path_num: u16, index: usize) -> &str {
        self.level_path
            .get(&path_num)
            .and_then(|l| l.get(index))
            .map_or("", String::as_str)
    }

    /// What is wrong with the tables next to the rules: a thing of the rules with no name, or a
    /// name for a number the rules do not have.
    pub fn problems(&self, rules: &Rules) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |kind: &str, file: &str, have: Vec<u16>, named: Vec<u16>| {
            for n in have.iter().filter(|n| !named.contains(n)) {
                out.push(format!(
                    "name: {kind} {n} has no name in text/<language>/name/{file}.ron"
                ));
            }
            for n in named.iter().filter(|n| !have.contains(n)) {
                out.push(format!(
                    "name: text/<language>/name/{file}.ron names the unknown {kind} {n}"
                ));
            }
        };
        let nums = |t: &Table<String>| t.keys().copied().collect::<Vec<_>>();
        check(
            "unit",
            "unit",
            rules.units.defs().iter().map(|d| d.num).collect(),
            nums(&self.unit),
        );
        check(
            "ability",
            "ability",
            rules.abilities.defs().iter().map(|d| d.num).collect(),
            nums(&self.ability),
        );
        check(
            "building",
            "building",
            rules.buildings.defs().iter().map(|d| d.num).collect(),
            nums(&self.building),
        );
        check(
            "race",
            "race",
            rules.races.defs().iter().map(|d| d.num).collect(),
            self.race.keys().copied().collect(),
        );
        check(
            "realm",
            "realm",
            rules.realms.defs().iter().map(|d| d.num).collect(),
            nums(&self.realm),
        );
        check(
            "retort",
            "retort",
            rules.retorts.defs().iter().map(|d| d.num).collect(),
            nums(&self.retort),
        );
        check(
            "wizard",
            "wizard",
            rules.wizards.defs().iter().map(|d| d.num).collect(),
            nums(&self.wizard),
        );
        check(
            "spell",
            "spell",
            rules.spells.defs().iter().map(|d| d.num).collect(),
            nums(&self.spell),
        );
        check(
            "item",
            "item",
            rules.items.defs().iter().map(|d| d.num).collect(),
            nums(&self.item),
        );
        check(
            "item power",
            "item_power",
            rules.item_powers.defs().iter().map(|d| d.num).collect(),
            nums(&self.item_power),
        );
        check(
            "level path",
            "level_path",
            rules.level_paths.defs().iter().map(|d| d.num).collect(),
            self.level_path.keys().copied().collect(),
        );
        check(
            "race (city names)",
            "city",
            rules.races.defs().iter().map(|d| d.num).collect(),
            self.city.keys().copied().collect(),
        );
        check(
            "hero",
            "hero",
            rules.hero_templates.iter().map(|t| t.unit.num()).collect(),
            self.hero.keys().copied().collect(),
        );
        for path in rules.level_paths.defs() {
            let named = self.level_path.get(&path.num).map_or(0, Vec::len);
            if named != path.level_count() {
                out.push(format!(
                    "name: level path {} has {} levels, text/<language>/name/level_path.ron names {named}",
                    path.num,
                    path.level_count()
                ));
            }
        }
        out
    }
}

use crate::text::{Args, Texts};
use f4x_core::item::crafting::{MadeName, Suffix};
use f4x_core::item::Item;

impl Names {
    /// What an item is called: the name it was given, the name of the prefab item it is a copy
    /// of, or the default name made from its parts.
    pub fn item_name(&self, item: &Item, texts: &Texts) -> String {
        if !item.name.is_empty() {
            return item.name.clone();
        }
        if let Some(made) = &item.made {
            return self.made_name(made, texts);
        }
        item.prefab
            .map_or_else(String::new, |id| self.item(id.num()).to_string())
    }

    /// The default name of an item made without one, in the language of the texts, at most
    /// thirty letters (the original's buffer).
    pub fn made_name(&self, made: &MadeName, texts: &Texts) -> String {
        let kind = texts.get(made.kind.key(), &Args::new());
        let mut name = match made.bonus {
            Some(n) => texts.get(
                "item.name_bonus",
                &Args::new().count("n", n).text("type", &kind),
            ),
            None => kind,
        };
        let suffix = match &made.suffix {
            None => None,
            Some(Suffix::Special(id)) => Some(self.item_power(id.num()).to_string()),
            Some(Suffix::Spell(id, n)) => Some(
                texts.get(
                    "item.name_charges",
                    &Args::new()
                        .text("spell", self.spell(id.num()))
                        .count("n", *n as i32),
                ),
            ),
            Some(other) => other.key().map(|k| texts.get(k, &Args::new())),
        };
        if let Some(suffix) = suffix {
            name = texts.get(
                "item.name_of",
                &Args::new().text("item", &name).text("suffix", &suffix),
            );
        }
        name.chars().take(30).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::{TestDir, TestResult};

    #[test]
    fn a_name_is_found_by_number_and_a_language_falls_back_to_english() -> TestResult {
        let dir = TestDir::new("names");
        std::fs::create_dir_all(dir.join("text/en/name"))?;
        std::fs::create_dir_all(dir.join("text/xx/name"))?;
        std::fs::write(
            dir.join("text/en/name/unit.ron"),
            r#"{ 1: "Spearmen", 2: "Archers" }"#,
        )?;
        std::fs::write(dir.join("text/xx/name/unit.ron"), r#"{ 1: "Lanzeros" }"#)?;
        std::fs::write(
            dir.join("text/en/name/race.ron"),
            r#"{ 0: ("Elf", "Elves"), 1: ("Orc", "") }"#,
        )?;
        let names = Names::load(&[&dir], "xx")?;
        assert_eq!(names.unit(1), "Lanzeros");
        assert_eq!(names.unit(2), "Archers");
        assert_eq!(names.unit(3), "");
        assert_eq!((names.race(0), names.race_plural(0)), ("Elf", "Elves"));
        assert_eq!(names.race_plural(1), "Orc");
        Ok(())
    }

    #[test]
    fn the_shipped_names_load() -> TestResult {
        let names = Names::load(&[Path::new(crate::test_app::CLIENT_CONTENT)], "en")?;
        assert_eq!(names.unit(0), "Dwarf");
        assert_eq!(names.race_plural(0), "Barbarians");
        Ok(())
    }

    #[test]
    fn the_shipped_name_lists_make_the_bundle_a_new_game_takes() -> TestResult {
        let names = Names::load(&[Path::new(crate::test_app::CLIENT_CONTENT)], "en")?;
        let content = crate::test_app::shipped::content();
        let bundle = names.bundle();
        for race in content.rules.races.defs() {
            let list = bundle.cities.get(&race.num);
            assert!(list.is_some_and(|l| l.len() >= 10), "race {}", race.id);
        }
        for t in &content.rules.hero_templates {
            assert_eq!(
                bundle.heroes[&t.unit.num()].len(),
                5,
                "hero {}",
                t.unit.num()
            );
        }
        assert_eq!(bundle.hero(0, "dwarf"), "Brax");
        assert_eq!(bundle.rivals.len(), 4);
        assert!(bundle.rivals.iter().all(|l| l.len() == 10));
        assert!(names.problems(&content.rules).is_empty());
        Ok(())
    }

    #[test]
    fn an_item_is_called_by_its_given_name_its_prefab_name_or_its_parts() {
        use crate::test_app::shipped;
        let content = shipped::content();
        let names = shipped::names();
        let texts = shipped::texts();
        let def = &content.rules.items.defs()[0];
        let mut item = Item::from_def(def);
        assert_eq!(names.item_name(&item, texts), names.item(def.num));
        assert!(!names.item_name(&item, texts).is_empty());
        item.name = "Rex's Blade".into();
        assert_eq!(names.item_name(&item, texts), "Rex's Blade");
        // A power and a spell name their suffix in the words of the text.
        item.name.clear();
        item.prefab = None;
        item.made = Some(MadeName {
            kind: f4x_core::item::crafting::TypeName::of(f4x_def::rules::ItemKind::Sword, 0),
            bonus: Some(3),
            suffix: Some(Suffix::Wizardry),
        });
        assert_eq!(names.item_name(&item, texts), "+3 Sword of Wizardry");
    }
}
