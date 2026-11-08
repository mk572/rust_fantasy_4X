//! Loading and validation of game content files.

pub mod rules;
#[cfg(test)]
mod testutil;

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use crate::content::Content;
use crate::feature::{FeatureDef, FeatureKind, Features};
use crate::map::Plane;
use crate::map::{TerrainClass, TerrainDef, Terrains, REQUIRED_CLASSES};
use crate::settings::SettingsDef;

#[derive(Debug)]
pub enum ContentError {
    Io(String),
    Parse(String),
    Invalid(Vec<String>),
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::Io(m) => write!(f, "cannot read content: {m}"),
            ContentError::Parse(m) => write!(f, "cannot parse content: {m}"),
            ContentError::Invalid(problems) => {
                write!(f, "invalid content:")?;
                problems.iter().try_for_each(|p| write!(f, "\n  - {p}"))
            }
        }
    }
}

impl std::error::Error for ContentError {}

pub fn parse_terrains(src: &str) -> Result<Terrains, ContentError> {
    let defs: Vec<TerrainDef> =
        ron::from_str(src).map_err(|e| ContentError::Parse(e.to_string()))?;
    let problems = validate_terrains(&defs);
    if problems.is_empty() {
        Ok(Terrains::new(defs))
    } else {
        Err(ContentError::Invalid(problems))
    }
}

pub fn load_terrains(path: &Path) -> Result<Terrains, ContentError> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    parse_terrains(&src)
}

/// Returns a human-readable list of problems; empty means valid.
pub fn validate_terrains(defs: &[TerrainDef]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    let mut nums = HashSet::new();
    for d in defs {
        let id_ok = !d.id.is_empty()
            && d.id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !id_ok {
            problems.push(format!(
                "terrain id {:?} must be lowercase [a-z0-9_] and non-empty",
                d.id
            ));
        }
        if !seen.insert(d.id.as_str()) {
            problems.push(format!("duplicate terrain id {:?}", d.id));
        }
        if d.num == 0 || !nums.insert(d.num) {
            problems.push(format!(
                "terrain {:?} needs a number that no other has",
                d.id
            ));
        }
        if d.planes.is_empty() {
            problems.push(format!("terrain {:?} is available on no plane", d.id));
        }
        match (d.class == TerrainClass::Ocean, d.walkable) {
            (true, true) => problems.push(format!("ocean terrain {:?} must not be walkable", d.id)),
            (false, false) => problems.push(format!("land terrain {:?} must be walkable", d.id)),
            _ => {}
        }
        if d.walkable && d.move_cost == 0 {
            problems.push(format!("walkable terrain {:?} needs move_cost >= 1", d.id));
        }
    }
    for plane in Plane::all() {
        for &class in REQUIRED_CLASSES {
            if !defs
                .iter()
                .any(|d| d.class == class && d.planes.contains(&plane))
            {
                problems.push(format!("no {class:?} terrain defined for {plane:?}"));
            }
        }
    }
    problems
}

pub(crate) fn id_problem(kind: &str, id: &str) -> Option<String> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    (!ok).then(|| format!("{kind} id {id:?} must be lowercase [a-z0-9_] and non-empty"))
}

pub fn parse_features(src: &str) -> Result<Features, ContentError> {
    let defs: Vec<FeatureDef> =
        ron::from_str(src).map_err(|e| ContentError::Parse(e.to_string()))?;
    let problems = validate_features(&defs);
    if problems.is_empty() {
        Ok(Features::new(defs))
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// Returns a human-readable list of problems; empty means valid.
pub fn validate_features(defs: &[FeatureDef]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    let mut nums = HashSet::new();
    for d in defs {
        problems.extend(id_problem("feature", &d.id));
        if !seen.insert(d.id.as_str()) {
            problems.push(format!("duplicate feature id {:?}", d.id));
        }
        if d.num == 0 || !nums.insert(d.num) {
            problems.push(format!(
                "feature {:?} needs a number that no other has",
                d.id
            ));
        }
        if d.kind != FeatureKind::Tower && d.spawn.is_empty() {
            problems.push(format!(
                "feature {:?} ({:?}) has no spawn rules",
                d.id, d.kind
            ));
        }
        for (i, r) in d.spawn.iter().enumerate() {
            let at = format!("feature {:?} spawn rule {i}", d.id);
            if r.weight == 0 {
                problems.push(format!("{at}: weight must be at least 1"));
            }
            if r.planes.is_empty() || r.terrain.is_empty() {
                problems.push(format!(
                    "{at}: needs at least one terrain class and one plane"
                ));
            }
            if r.terrain.contains(&TerrainClass::Ocean) {
                problems.push(format!("{at}: features cannot spawn on Ocean"));
            }
        }
    }
    if !defs.iter().any(|d| d.kind == FeatureKind::Tower) {
        problems.push("no Tower feature defined".into());
    }
    for plane in Plane::all() {
        for kind in [
            FeatureKind::Mineral,
            FeatureKind::Encounter,
            FeatureKind::Node,
        ] {
            let any = defs
                .iter()
                .any(|d| d.kind == kind && d.spawn.iter().any(|r| r.planes.contains(&plane)));
            if !any {
                problems.push(format!("no {kind:?} feature can spawn on {plane:?}"));
            }
        }
    }
    problems
}

/// Load the game's help: the topics' text in `lang` (`text/<lang>/help/help.ron`, the fallback
/// language's when a language has no file) and the screens' areas (`help_screens.ron`, no words),
/// each from the first of `dirs` that has it. The help text is ours: nothing else is read, so the
/// original's text is never a fallback. Directories without the files give an empty [`Help`].
pub fn load_help(dirs: &[&Path], lang: &str) -> Result<crate::help::Help, ContentError> {
    use crate::text::FALLBACK_LANGUAGE;
    let first = |name: &str| dirs.iter().map(|d| d.join(name)).find(|p| p.exists());
    let topics_file = first(&format!("text/{lang}/help/help.ron"))
        .or_else(|| first(&format!("text/{FALLBACK_LANGUAGE}/help/help.ron")));
    let help = crate::help::Help {
        topics: read_list(topics_file.as_deref())?,
        screens: read_list(first("help_screens.ron").as_deref())?,
    };
    let problems = help.problems();
    if problems.is_empty() {
        Ok(help)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// The rows of a `.ron` list; no file gives none.
fn read_list<T: serde::de::DeserializeOwned>(path: Option<&Path>) -> Result<Vec<T>, ContentError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    ron::from_str(&text).map_err(|e| ContentError::Parse(format!("{}: {e}", path.display())))
}

/// Load and validate all content files from a content directory.
pub fn load_content(dir: &Path) -> Result<Content, ContentError> {
    load_content_layers(&[dir])
}

/// Like [`load_content`], with several directories: each file is read from the first one that
/// has it (the game puts its local directory before the committed `content/`).
pub fn load_content_layers(dirs: &[&Path]) -> Result<Content, ContentError> {
    let read = |name: &str| {
        let path = dirs
            .iter()
            .map(|d| d.join(name))
            .find(|p| p.exists())
            .unwrap_or_else(|| dirs.first().map_or_else(|| name.into(), |d| d.join(name)));
        std::fs::read_to_string(&path)
            .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))
    };
    let terrains = parse_terrains(&read("terrain.ron")?)?;
    let features = parse_features(&read("features.ron")?)?;
    let settings = load_settings(dirs)?;
    let rules = rules::load_rules(dirs)?;
    if dirs.iter().any(|d| d.join("rules_settings.ron").exists()) {
        check_spell_library(&settings.spell_library, &rules)?;
    }
    Ok(Content {
        terrains,
        features,
        rules,
        settings,
    })
}

/// The spells and realms the `spell_library` of `rules_settings.ron` names must exist.
fn check_spell_library(
    library: &crate::settings::SpellLibraryDef,
    rules: &crate::rules::Rules,
) -> Result<(), ContentError> {
    let mut problems = Vec::new();
    let spells = library
        .always_known
        .iter()
        .chain(library.retort_known.values().flatten())
        .chain(&library.arcane_closed)
        .chain(library.ai_main_realm_known.values())
        .chain(library.ai_main_realm_researchable.values());
    for spell in spells {
        if rules.spells.try_get(*spell).is_none() {
            problems.push(format!("spell_library: unknown spell {}", spell.num()));
        }
    }
    for retort in library.retort_known.keys() {
        if rules.retorts.try_get(*retort).is_none() {
            problems.push(format!("spell_library: unknown retort {}", retort.num()));
        }
    }
    for realm in library
        .ai_main_realm_known
        .keys()
        .chain(library.ai_main_realm_researchable.keys())
    {
        if rules.realms.try_get(*realm).is_none() {
            problems.push(format!("spell_library: unknown realm {}", realm.num()));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// Read `rules_settings.ron` from the first of `dirs` that has it. The file is optional (content
/// without it plays the symmetric game); one that is there must parse and be in range.
pub fn load_settings(dirs: &[&Path]) -> Result<SettingsDef, ContentError> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("rules_settings.ron"))
        .find(|p| p.exists())
    else {
        return Ok(SettingsDef::default());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    let settings: SettingsDef = ron::from_str(&text)
        .map_err(|e| ContentError::Parse(format!("rules_settings.ron: {e}")))?;
    let problems = settings.validate();
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testdir::TestResult;

    fn shipped() -> String {
        let path = super::testutil::content_dir().join("terrain.ron");
        std::fs::read_to_string(path).expect("crates/f4x-def/content/terrain.ron exists")
    }

    /// The game's own `content/` folder (the files tests of the rules data read).
    fn real_content() -> Content {
        load_content(&super::testutil::content_dir()).expect("the shipped content loads")
    }

    /// The bonus a level path gives in all up to `level`, for a stat.
    fn level_total(content: &Content, path: u16, level: usize, stat: crate::rules::Stat) -> i32 {
        use crate::rules::effects::Effect;
        use crate::rules::ModOp;
        let path = content
            .rules
            .level_paths
            .by_num(path)
            .expect("the level path exists");
        (0..=level)
            .filter_map(|i| path.level(i))
            .flat_map(|effects| effects.iter())
            .map(|r| match r.effect {
                Effect::Modify {
                    stat: s,
                    op: ModOp::Add,
                    value,
                } if s == stat => value.floor(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn the_hero_types_and_their_starting_records_are_the_originals() -> TestResult {
        use crate::rules::{HeroKind, PickType};
        let c = load_content_layers(&[&super::testutil::content_dir()])?;
        let units = &c.rules.units;
        let heroes: Vec<_> = units
            .defs()
            .iter()
            .filter(|u| u.kind == crate::rules::UnitKind::Hero)
            .collect();
        assert_eq!(heroes.len(), 35);
        assert_eq!(c.rules.hero_templates.len(), 35);
        // The records follow the units' order.
        for (h, t) in heroes.iter().zip(&c.rules.hero_templates) {
            assert_eq!(h.num, t.unit.num());
        }
        let count = |k| {
            heroes
                .iter()
                .filter(|h| h.hero.as_ref().is_some_and(|x| x.kind == k))
                .count()
        };
        assert_eq!(
            [
                count(HeroKind::Warrior),
                count(HeroKind::Bow),
                count(HeroKind::SwordOrStaff),
                count(HeroKind::Mage)
            ],
            [19, 4, 3, 9]
        );
        let hero = |num: u16| {
            units
                .by_num(num)
                .and_then(|u| u.hero.clone())
                .expect("the hero unit exists")
        };
        assert_eq!(hero(16 /* ranger */).kind, HeroKind::Bow);
        // The bow hero is the Elven Archer and the melee hero the Knight (docs/divergences.md).
        assert_eq!(hero(30 /* elven_archer */).kind, HeroKind::Bow);
        assert_eq!(hero(31 /* knight */).kind, HeroKind::Warrior);

        let t = |num: u16| {
            c.rules
                .hero_templates
                .iter()
                .find(|t| t.unit.num() == num)
                .expect("the hero has a template")
        };
        assert_eq!(
            t(2 /* sage */).spells,
            [
                crate::rules::SpellId::new(42), // dispel_magic_true
                crate::rules::SpellId::new(48), // counter_magic
            ]
        );
        assert_eq!(t(2 /* sage */).casting_skill, 2);
        assert_eq!(
            t(5 /* bard */).abilities,
            [
                crate::rules::Ability::new(75), // leadership
                crate::rules::Ability::new(97), // female
            ]
        );
        assert_eq!(
            (t(30 /* elven_archer */).picks, t(30).pick_type),
            (3, PickType::Any)
        );
        assert_eq!(t(34 /* chosen */).spells.len(), 4);
        Ok(())
    }

    #[test]
    fn the_level_paths_are_the_originals_level_bonuses() -> TestResult {
        use crate::rules::Stat::*;
        let c = load_content_layers(&[&super::testutil::content_dir()])?;
        // A normal unit at Champion (level 5): resistance +5, attacks +3, defence +2, to hit +3.
        let at = |stat| level_total(&c, 2 /* standard_xp */, 5, stat);
        assert_eq!(
            [
                at(Resistance),
                at(Melee),
                at(Ranged),
                at(Defense),
                at(ToHit)
            ],
            [5, 3, 3, 2, 3]
        );
        // Veteran gains defence, not attacks.
        assert_eq!(level_total(&c, 2 /* standard_xp */, 2, Defense), 1);
        assert_eq!(level_total(&c, 2 /* standard_xp */, 2, Melee), 1);
        // A hero at level 8: resistance +8, attacks +8, defence +4, to hit +3, hit points +8.
        let hero = |stat| level_total(&c, 1 /* hero_xp */, 8, stat);
        assert_eq!(
            [
                hero(Resistance),
                hero(Melee),
                hero(Defense),
                hero(ToHit),
                hero(MaxHp)
            ],
            [8, 8, 4, 3, 8]
        );
        Ok(())
    }

    #[test]
    fn shipped_terrain_file_is_valid() -> TestResult {
        let t = parse_terrains(&shipped())?;
        assert!(t.id_numbered(4 /* grassland */).is_some());
        Ok(())
    }

    #[test]
    fn reports_every_problem() -> TestResult {
        let src = shipped()
            .replace("id: \"forest\"", "id: \"Forest\"")
            .replace("class: Forest", "class: Hills");
        let Err(ContentError::Invalid(p)) = parse_terrains(&src) else {
            return Err("the broken terrain file was accepted".into());
        };
        assert!(p.iter().any(|m| m.contains("must be lowercase")));
        assert!(p.iter().any(|m| m.contains("no Forest terrain")));
        Ok(())
    }

    #[test]
    fn reports_parse_errors() {
        assert!(matches!(
            parse_terrains("[(id: 1)]"),
            Err(ContentError::Parse(_))
        ));
    }

    #[test]
    fn features_validation_reports_problems() -> TestResult {
        let src = std::fs::read_to_string(super::testutil::content_dir().join("features.ron"))?;
        let broken = src
            .replace("id: \"coal\"", "id: \"iron_ore\"")
            .replace("kind: Tower", "kind: Mineral");
        let Err(ContentError::Invalid(p)) = parse_features(&broken) else {
            return Err("the broken features file was accepted".into());
        };
        assert!(p.iter().any(|m| m.contains("duplicate feature id")));
        assert!(p.iter().any(|m| m.contains("no Tower feature")));
        assert!(matches!(
            parse_features("[(id: 1)]"),
            Err(ContentError::Parse(_))
        ));
        Ok(())
    }

    #[test]
    fn the_real_rules_data_has_every_item_and_power_of_the_original() -> TestResult {
        let c = load_content(&super::testutil::content_dir())?;
        assert_eq!(c.rules.items.len(), 250);
        assert_eq!(c.rules.item_powers.len(), 66);
        assert!(c.rules.items.id_numbered(56).is_some());
        assert!(c.rules.items.id_numbered(13).is_some());
        Ok(())
    }

    #[test]
    fn item_movement_is_in_whole_moves() -> TestResult {
        use crate::rules::effects::Effect;
        use crate::rules::{ModOp, Stat};
        let c = load_content(&super::testutil::content_dir())?;
        let mut seen = 0;
        for item in c.rules.items.defs() {
            for rule in &item.effects {
                if let Effect::Modify {
                    stat: Stat::Moves,
                    op: ModOp::Add,
                    value,
                } = rule.effect
                {
                    seen += 1;
                    assert!(
                        (1..=4).contains(&value.floor()) && value.is_whole(),
                        "{} has {value} moves",
                        item.id
                    );
                }
            }
        }
        assert_eq!(seen, 58);
        Ok(())
    }

    #[test]
    fn shipped_settings_have_five_levels_with_the_originals_figures() {
        let c = real_content();
        let s = &c.settings;
        assert_eq!(s.levels.len(), 5);
        let gold: Vec<_> = s
            .levels
            .iter()
            .map(|l| l.wizards.starting_gold.human)
            .collect();
        assert_eq!(gold, [125, 100, 75, 50, 25]);
        assert_eq!(s.levels[4].ai.production_percent, 400);
        assert!(s.levels[1].rules.safe_flee.human && !s.levels[2].rules.safe_flee.human);
        assert!(!s.levels[0].events.random_events && s.levels[1].events.random_events);
        assert_eq!(s.new_game.difficulty, 2);
        assert_eq!(s.max_opponents, 9);
        let sizes: Vec<_> = s
            .land_sizes
            .iter()
            .map(|l| (l.id.as_str(), l.land_percent))
            .collect();
        assert_eq!(sizes, [("small", 15), ("medium", 20), ("large", 30)]);
        let pressure: Vec<_> = s.levels.iter().map(|l| l.events.pressure_percent).collect();
        assert_eq!(pressure, [50, 66, 75, 80, 100]);
        assert!(s
            .levels
            .iter()
            .all(|l| l.world.start_reveal_radius.0 == 2.5));
        assert!(s.levels.iter().all(|l| l.wizards.starting_mana.human == 0));
    }

    #[test]
    fn content_without_settings_leaves_them_empty_and_a_local_file_wins() -> TestResult {
        let none = crate::testdir::TestDir::new("settings-none");
        assert_eq!(load_settings(&[&none])?, SettingsDef::default());
        let over = crate::testdir::TestDir::new("settings-layer");
        std::fs::write(
            over.join("rules_settings.ron"),
            "(new_game: (opponents: 3))",
        )?;
        let committed = super::testutil::content_dir();
        let c = load_content_layers(&[&over, &committed])?;
        assert_eq!(c.settings.new_game.opponents, 3);
        assert!(c.settings.levels.is_empty());
        Ok(())
    }

    #[test]
    fn a_settings_file_out_of_range_is_refused() -> TestResult {
        let over = crate::testdir::TestDir::new("settings-bad");
        std::fs::write(
            over.join("rules_settings.ron"),
            "(new_game: (opponents: 10))",
        )?;
        let err = load_settings(&[&over]).unwrap_err();
        assert!(err.to_string().contains("10 opponents"), "{err}");
        std::fs::write(over.join("rules_settings.ron"), "(levels: [])")?;
        assert!(load_settings(&[&over]).is_ok());
        std::fs::write(
            over.join("rules_settings.ron"),
            "(new_game: (difficulty: 2)",
        )?;
        assert!(matches!(
            load_settings(&[&over]),
            Err(ContentError::Parse(_))
        ));
        Ok(())
    }

    #[test]
    fn the_shipped_spell_library_is_the_originals() {
        // The built-in default (used when there is no rules_settings.ron) is the original's numbers,
        // and so is the shipped file.
        let c = real_content();
        let default = crate::settings::SpellLibraryDef::default();
        assert_eq!(c.settings.spell_library, default);
    }

    #[test]
    fn a_spell_library_naming_an_unknown_spell_or_realm_is_refused() -> TestResult {
        let over = crate::testdir::TestDir::new("settings-spells");
        let committed = super::testutil::content_dir();
        std::fs::write(
            over.join("rules_settings.ron"),
            r#"(spell_library: (always_known: [9999]))"#,
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("9999"), "{err}");
        std::fs::write(
            over.join("rules_settings.ron"),
            r#"(spell_library: (ai_main_realm_known: {9998: 9}))"#,
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("9998"), "{err}");
        std::fs::write(
            over.join("rules_settings.ron"),
            "(spell_library: (known: []))",
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("0 rows"), "{err}");
        Ok(())
    }

    #[test]
    fn each_retort_constant_is_the_number_of_its_row_in_the_content() {
        let rules = &real_content().rules;
        let named = [
            (
                crate::settings::Retort::new(0), /* alchemy */
                "alchemy",
            ),
            (
                crate::settings::Retort::new(1), /* warlord */
                "warlord",
            ),
            (
                crate::settings::Retort::new(2), /* chaos_mastery */
                "chaos_mastery",
            ),
            (
                crate::settings::Retort::new(3), /* nature_mastery */
                "nature_mastery",
            ),
            (
                crate::settings::Retort::new(4), /* sorcery_mastery */
                "sorcery_mastery",
            ),
            (
                crate::settings::Retort::new(5), /* infernal_power */
                "infernal_power",
            ),
            (
                crate::settings::Retort::new(6), /* divine_power */
                "divine_power",
            ),
            (
                crate::settings::Retort::new(7), /* sage_master */
                "sage_master",
            ),
            (
                crate::settings::Retort::new(8), /* channeler */
                "channeler",
            ),
            (crate::settings::Retort::new(9) /* myrran */, "myrran"),
            (
                crate::settings::Retort::new(10), /* archmage */
                "archmage",
            ),
            (
                crate::settings::Retort::new(11), /* mana_focusing */
                "mana_focusing",
            ),
            (
                crate::settings::Retort::new(12), /* node_mastery */
                "node_mastery",
            ),
            (crate::settings::Retort::new(13) /* famous */, "famous"),
            (
                crate::settings::Retort::new(14), /* runemaster */
                "runemaster",
            ),
            (
                crate::settings::Retort::new(15), /* conjurer */
                "conjurer",
            ),
            (
                crate::settings::Retort::new(16), /* charismatic */
                "charismatic",
            ),
            (
                crate::settings::Retort::new(17), /* artificer */
                "artificer",
            ),
        ];
        assert_eq!(named.len(), rules.retorts.len(), "a constant for each row");
        for (id, alias) in named {
            assert_eq!(rules.retorts.get(id).id, alias, "number {}", id.num());
            let by_alias = rules.retorts.iter().find(|(_, d)| d.id == alias);
            assert_eq!(by_alias.map(|(found, _)| found), Some(id), "{alias}");
        }
    }
}
