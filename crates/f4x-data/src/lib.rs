//! Loading and validation of game content files.

pub mod rules;
pub mod save;
#[cfg(test)]
mod testutil;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use f4x_core::content::Content;
use f4x_core::feature::def::{FeatureDef, FeatureKind, Features};
use f4x_core::game::setting::SettingsDef;
use f4x_core::map::terrain::{TerrainClass, TerrainDef, Terrains, REQUIRED_CLASSES};
use f4x_core::map::Plane;

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
    for plane in Plane::ALL {
        for class in REQUIRED_CLASSES {
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
    for plane in Plane::ALL {
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

/// Load the game's help (`help.ron`, `help_screens.ron`) from the content directory `dir`. The
/// help text is ours: nothing else is read, so the original's text is never a fallback. A
/// directory without the files gives an empty [`Help`].
pub fn load_help(dir: &Path) -> Result<f4x_core::help::Help, ContentError> {
    let help = f4x_core::help::Help {
        topics: optional::<f4x_core::help::HelpTopic>(dir, "help.ron")?,
        screens: optional::<f4x_core::help::HelpScreen>(dir, "help_screens.ron")?,
    };
    let problems = help.problems();
    if problems.is_empty() {
        Ok(help)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

fn optional<T: serde::de::DeserializeOwned>(
    dir: &Path,
    name: &str,
) -> Result<Vec<T>, ContentError> {
    let path = dir.join(name);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    ron::from_str(&text).map_err(|e| ContentError::Parse(format!("{name}: {e}")))
}

/// Load the optional event messages (`event_texts.ron`, from `EVENTMSG.LBX`) from a content
/// directory. A directory without them yields none, and the events are told without words.
pub fn load_event_texts(dir: &Path) -> Result<Vec<f4x_core::world_event::EventText>, ContentError> {
    let path = dir.join("event_texts.ron");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    ron::from_str(&text).map_err(|e| ContentError::Parse(format!("event_texts.ron: {e}")))
}

/// Load the optional original lines of the wizards' talk (`diplomsg.ron`, from `DIPLOMSG.LBX`)
/// from a content directory: the variants of each situation, with `{placeholders}`. A directory
/// without them yields none, and the wizards speak the game's own lines.
pub fn load_diplomacy_lines(dir: &Path) -> Result<HashMap<u16, Vec<String>>, ContentError> {
    #[derive(serde::Deserialize)]
    struct Row {
        situation: u16,
        lines: Vec<String>,
    }
    let path = dir.join("diplomsg.ron");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    let rows: Vec<Row> =
        ron::from_str(&text).map_err(|e| ContentError::Parse(format!("diplomsg.ron: {e}")))?;
    Ok(rows.into_iter().map(|r| (r.situation, r.lines)).collect())
}

/// One language's messages: every `*.ron` file of `text/<lang>/` in each directory. A key may be
/// in one file only; a directory earlier in `dirs` wins over a later one for a key both have.
fn load_language(
    dirs: &[&Path],
    lang: &str,
) -> Result<HashMap<String, f4x_core::text::Entry>, ContentError> {
    let mut all = HashMap::new();
    for dir in dirs.iter().rev() {
        let folder = dir.join("text").join(lang);
        let Ok(files) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut paths: Vec<_> = files
            .filter_map(|f| f.ok().map(|f| f.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        paths.sort();
        let mut here: HashMap<String, f4x_core::text::Entry> = HashMap::new();
        let mut problems = Vec::new();
        for path in paths {
            let src = std::fs::read_to_string(&path)
                .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
            let entries: HashMap<String, f4x_core::text::Entry> = ron::from_str(&src)
                .map_err(|e| ContentError::Parse(format!("{}: {e}", path.display())))?;
            for (key, entry) in entries {
                if here.insert(key.clone(), entry).is_some() {
                    problems.push(format!(
                        "{}: the key {key} is in two files",
                        folder.display()
                    ));
                }
            }
        }
        if !problems.is_empty() {
            return Err(ContentError::Invalid(problems));
        }
        all.extend(here);
    }
    Ok(all)
}

/// The messages in `lang`, with those of the fallback language behind them.
pub fn load_texts(dirs: &[&Path], lang: &str) -> Result<f4x_core::text::Texts, ContentError> {
    use f4x_core::text::{Texts, FALLBACK_LANGUAGE};
    let fallback = load_language(dirs, FALLBACK_LANGUAGE)?;
    let entries = if lang == FALLBACK_LANGUAGE {
        HashMap::new()
    } else {
        load_language(dirs, lang)?
    };
    Ok(Texts::new(lang, entries, FALLBACK_LANGUAGE, fallback))
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
    let mut terrains = parse_terrains(&read("terrain.ron")?)?;
    let mut features = parse_features(&read("features.ron")?)?;
    let mut settings = load_settings(dirs)?;
    let client_settings = load_client_settings(dirs)?;
    let mut problems = Vec::new();
    let at = (dirs, client_settings.language.as_str());
    rules::fill_names(
        at,
        ("terrain", "terrain"),
        terrains.defs_mut(),
        (|d| d.num, |d| &d.id),
        |d, n: String| d.name = n,
        &mut problems,
    )?;
    rules::fill_names(
        at,
        ("feature", "feature"),
        features.defs_mut(),
        (|d| d.num, |d| &d.id),
        |d, n: String| d.name = n,
        &mut problems,
    )?;
    if !problems.is_empty() {
        return Err(ContentError::Invalid(problems));
    }
    let rules = rules::load_rules(dirs, &client_settings.language, &features)?;
    settings
        .spell_library
        .resolve(rules.spells.defs(), &rules.retorts, &rules.realms);
    if dirs.iter().any(|d| d.join("rules_settings.ron").exists()) {
        check_spell_library(&settings.spell_library, &rules)?;
    }
    let texts = load_texts(dirs, &client_settings.language)?;
    Ok(Content {
        terrains,
        features,
        rules,
        settings,
        client_settings,
        texts,
    })
}

/// The spells and realms the `spell_library` of `rules_settings.ron` names must exist.
fn check_spell_library(
    library: &f4x_core::game::setting::SpellLibraryDef,
    rules: &f4x_core::rules::Rules,
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
        if spell.id().is_none() {
            problems.push(format!("spell_library: unknown spell \"{}\"", spell.name()));
        }
    }
    for retort in library.retort_known.keys() {
        if rules.retorts.id_of(retort).is_none() {
            problems.push(format!("spell_library: unknown retort \"{retort}\""));
        }
    }
    for realm in library
        .ai_main_realm_known
        .keys()
        .chain(library.ai_main_realm_researchable.keys())
    {
        if rules.realms.id_of(realm).is_none() {
            problems.push(format!("spell_library: unknown realm \"{realm}\""));
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

/// Read `client_settings.ron` from the first of `dirs` that has it: the language and the
/// switches of the client. The file is optional; one that is there must parse.
pub fn load_client_settings(
    dirs: &[&Path],
) -> Result<f4x_core::game::setting::ClientSettingsDef, ContentError> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("client_settings.ron"))
        .find(|p| p.exists())
    else {
        return Ok(Default::default());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    ron::from_str(&text).map_err(|e| ContentError::Parse(format!("client_settings.ron: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::rules::Ref;
    use f4x_core::testdir::TestResult;

    fn shipped() -> String {
        let path = crate::testutil::content_dir().join("terrain.ron");
        std::fs::read_to_string(path).expect("content/terrain.ron exists")
    }

    /// `content/font_colour_sets.ron`, written by `f4x-extract art`: sixteen colour sets of
    /// fifteen colours each (a glyph colour index 1 to 15 picks one), the first one white.
    #[test]
    fn the_font_colour_sets_are_sixteen_sets_of_fifteen_colours() {
        #[derive(serde::Deserialize)]
        struct Sets {
            sets: Vec<Vec<(u8, u8, u8)>>,
        }
        let path = crate::testutil::content_dir().join("font_colour_sets.ron");
        let text = std::fs::read_to_string(path).expect("content/font_colour_sets.ron exists");
        let parsed: Sets = ron::from_str(&text).expect("the colour sets parse");
        assert_eq!(parsed.sets.len(), 16);
        assert!(parsed.sets.iter().all(|set| set.len() == 15));
        assert_eq!(parsed.sets[0][0], (255, 255, 255));
    }

    /// The game's own `content/` folder (the files tests of the rules data read).
    fn real_content() -> Content {
        load_content(&crate::testutil::content_dir()).expect("the shipped content loads")
    }

    /// The bonus a level path gives in all up to `level`, for a stat.
    fn level_total(
        content: &Content,
        path: &str,
        level: usize,
        stat: f4x_core::rules::Stat,
    ) -> i32 {
        use f4x_core::rules::effects::Effect;
        use f4x_core::rules::ModOp;
        let path = content
            .rules
            .level_paths
            .by_name(path)
            .expect("the level path exists");
        (0..=level)
            .filter_map(|i| path.level(i))
            .flat_map(|(_, effects)| effects.iter())
            .map(|r| match r.effect {
                Effect::Modify {
                    stat: s,
                    op: ModOp::Add,
                    value,
                } if s == stat => value,
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn the_hero_types_and_their_starting_records_are_the_originals() -> TestResult {
        use f4x_core::rules::{HeroKind, PickType};
        let c = load_content_layers(&[&crate::testutil::content_dir()])?;
        let units = &c.rules.units;
        let heroes: Vec<_> = units
            .defs()
            .iter()
            .filter(|u| u.kind == f4x_core::rules::UnitKind::Hero)
            .collect();
        assert_eq!(heroes.len(), 35);
        assert_eq!(c.rules.hero_templates.len(), 35);
        // The records follow the units' order.
        for (h, t) in heroes.iter().zip(&c.rules.hero_templates) {
            assert_eq!(h.id, t.unit);
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
        let hero = |id: &str| {
            units
                .by_name(id)
                .and_then(|u| u.hero.clone())
                .expect("the hero unit exists")
        };
        assert_eq!(hero("ranger").kind, HeroKind::Bow);
        // The bow hero is the Elven Archer and the melee hero the Knight (docs/divergences.md).
        assert_eq!(hero("elven_archer").kind, HeroKind::Bow);
        assert_eq!(hero("knight").kind, HeroKind::Warrior);

        let t = |id: &str| {
            c.rules
                .hero_templates
                .iter()
                .find(|t| t.unit == id)
                .expect("the hero has a template")
        };
        assert_eq!(
            t("sage").spells,
            [Ref::new("dispel_magic_true"), Ref::new("counter_magic")]
        );
        assert_eq!(t("sage").casting_skill, 2);
        assert_eq!(
            t("bard").abilities,
            [Ref::new("leadership"), Ref::new("female")]
        );
        assert_eq!(
            (t("elven_archer").picks, t("elven_archer").pick_type),
            (3, PickType::Any)
        );
        assert_eq!(t("chosen").spells.len(), 4);
        Ok(())
    }

    #[test]
    fn rolled_heroes_follow_init_heroes() -> TestResult {
        use f4x_core::game::rng::GameRng;
        use f4x_core::hero::kind::roll;
        use f4x_core::hero::{HeroAbilities, Tier};
        use f4x_core::rules::PickType;
        let c = load_content_layers(&[&crate::testutil::content_dir()])?;
        let rules = &c.rules;
        // Steps of ability a record has beyond what its template gives: each tier is a step.
        let steps = |a: &HeroAbilities| -> i32 {
            [
                a.leadership,
                a.legendary,
                a.blademaster,
                a.armsmaster,
                a.constitution,
                a.might,
                a.arcane_power,
                a.prayermaster,
                a.agility,
                a.sage,
            ]
            .iter()
            .map(|t| match t {
                Tier::None => 0,
                Tier::Normal => 1,
                Tier::Super => 2,
            })
            .sum::<i32>()
                + i32::from(a.lucky)
                + i32::from(a.charmed)
                + i32::from(a.noble)
        };
        let mut distinct = std::collections::BTreeSet::new();
        for seed in 0..200 {
            // The game's heroes are rolled once from its stream, not once for each wizard.
            let roster = roll(&mut GameRng::new(seed), rules);
            assert_eq!(roster, roll(&mut GameRng::new(seed), rules));
            assert_eq!(roster.len(), 35);
            for (r, t) in roster.iter().zip(&rules.hero_templates) {
                let fixed = HeroAbilities::from_ids(
                    &t.abilities
                        .iter()
                        .filter_map(|a| a.id())
                        .collect::<Vec<_>>(),
                );
                let gained = steps(&r.abilities) - steps(&fixed)
                    + (i32::from(r.casting_skill) - t.casting_skill as i32);
                // A pick that finds nothing to raise (a second Arcane Power on a hero
                // without a magic attack) still counts, so never more than the picks.
                assert!(
                    (0..=t.picks as i32).contains(&gained),
                    "{} gained {gained} of {} picks",
                    t.unit,
                    t.picks
                );
                assert_eq!(r.spells, f4x_core::rules::refs::ids(&t.spells));
                // Fixed abilities stay (a pick may double one).
                let a = &r.abilities;
                for name in [
                    "leadership",
                    "legendary",
                    "blademaster",
                    "armsmaster",
                    "constitution",
                    "might",
                    "arcane_power",
                    "prayermaster",
                    "agility",
                    "sage",
                ] {
                    assert!(a.tier(name) >= fixed.tier(name), "{} lost {name}", t.unit);
                }
                assert!(a.lucky >= fixed.lucky && a.charmed >= fixed.charmed);
                assert!(a.noble >= fixed.noble && a.female == fixed.female);
                let martial = [
                    a.leadership,
                    a.legendary,
                    a.blademaster,
                    a.armsmaster,
                    a.constitution,
                    a.might,
                    a.agility,
                ];
                let magic = [a.arcane_power, a.prayermaster, a.sage];
                let (f_martial, f_magic) = (
                    [
                        fixed.leadership,
                        fixed.legendary,
                        fixed.blademaster,
                        fixed.armsmaster,
                        fixed.constitution,
                        fixed.might,
                        fixed.agility,
                    ],
                    [fixed.arcane_power, fixed.prayermaster, fixed.sage],
                );
                match t.pick_type {
                    PickType::Mage => {
                        assert_eq!(martial, f_martial, "{} drew a warrior ability", t.unit)
                    }
                    PickType::Warrior => {
                        assert_eq!(magic, f_magic, "{} drew a mage ability", t.unit);
                        assert_eq!(i32::from(r.casting_skill), t.casting_skill as i32);
                    }
                    PickType::Any => {}
                }
                // The original's exclusions.
                // Arcane Power only goes to a hero with a magic ranged attack.
                let unit = rules.units.by_name(&t.unit).ok_or("the value is missing")?;
                if !unit.abilities.iter().any(|x| {
                    rules
                        .abilities
                        .id_of(x)
                        .is_some_and(|id| f4x_core::hero::kind::MAGIC_RANGED.contains(&id))
                }) {
                    assert_eq!(a.arcane_power, fixed.arcane_power, "{}", t.unit);
                }
                if unit
                    .ability_ids
                    .contains(&f4x_core::rules::Ability::new(40))
                {
                    assert_eq!(a.charmed, fixed.charmed, "{} is immune already", t.unit);
                }
            }
            distinct.insert(format!(
                "{:?}",
                roster.iter().map(|r| r.abilities).collect::<Vec<_>>()
            ));
        }
        assert!(
            distinct.len() > 100,
            "the draws are not varied: {}",
            distinct.len()
        );
        Ok(())
    }

    #[test]
    fn every_hero_has_names_in_the_text_folder() -> TestResult {
        let committed = crate::testutil::content_dir();
        let c = load_content_layers(&[&committed])?;
        for t in &c.rules.hero_templates {
            assert_eq!(t.names.len(), 5, "hero {}", t.unit);
        }
        let dwarf = c.rules.hero_templates.iter().find(|t| t.unit == "dwarf");
        assert_eq!(dwarf.map(|t| t.names[0].as_str()), Some("Brax"));
        // A layer can replace the file; a number that is no hero is a load error.
        let over = f4x_core::testdir::TestDir::new("hero-names");
        std::fs::create_dir_all(over.join("text/en/name"))?;
        std::fs::write(
            over.join("text/en/name/hero.ron"),
            r#"{ 9999: ["Nobody"] }"#,
        )?;
        assert!(load_content_layers(&[&over, &committed]).is_err());
        Ok(())
    }

    #[test]
    fn the_level_paths_are_the_originals_level_bonuses() -> TestResult {
        use f4x_core::rules::Stat::*;
        let c = load_content_layers(&[&crate::testutil::content_dir()])?;
        // A normal unit at Champion (level 5): resistance +5, attacks +3, defence +2, to hit +3.
        let at = |stat| level_total(&c, "standard_xp", 5, stat);
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
        assert_eq!(level_total(&c, "standard_xp", 2, Defense), 1);
        assert_eq!(level_total(&c, "standard_xp", 2, Melee), 1);
        // A hero at level 8: resistance +8, attacks +8, defence +4, to hit +3, hit points +8.
        let hero = |stat| level_total(&c, "hero_xp", 8, stat);
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
        assert!(t.id_of("grassland").is_some());
        Ok(())
    }

    #[test]
    fn shipped_content_generates_a_map() -> TestResult {
        let g = f4x_core::game::Game::with_generated_map(1, &Default::default(), &real_content())?;
        assert!(g.map().is_some());
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

    /// The minerals and the placed features on one plane of a generated map (a tower is on both).
    fn feature_ids(
        generated: &f4x_core::map::mapgen::Generated,
        plane: Plane,
        c: &Content,
    ) -> Vec<(u32, u32, String)> {
        let grid = generated.map.plane(plane);
        let mut out = Vec::new();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if let Some(f) = grid.mineral(x as i32, y as i32) {
                    out.push((
                        x,
                        y,
                        c.features
                            .get(f)
                            .expect("a placed mineral exists")
                            .id
                            .clone(),
                    ));
                }
            }
        }
        for p in &generated.placements {
            let def = c.features.get(p.feature).expect("a placed feature exists");
            if p.at.plane == plane || def.kind == FeatureKind::Tower {
                out.push((p.at.x, p.at.y, def.id.clone()));
            }
        }
        out
    }

    #[test]
    fn features_validation_reports_problems() -> TestResult {
        let src = std::fs::read_to_string(crate::testutil::content_dir().join("features.ron"))?;
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
    fn the_buildings_that_allow_nightshade_exist() -> TestResult {
        // The real rules data, not the small test set.
        let c = load_content(&crate::testutil::content_dir())?;
        for id in f4x_core::city::nightshade::NIGHTSHADE_BUILDINGS {
            assert!(
                c.rules.buildings.id_numbered(id.num()).is_some(),
                "no building {}",
                id.num()
            );
        }
        Ok(())
    }

    #[test]
    fn the_real_rules_data_has_every_item_and_power_of_the_original() -> TestResult {
        let c = load_content(&crate::testutil::content_dir())?;
        assert_eq!(c.rules.items.len(), 250);
        assert_eq!(c.rules.item_powers.len(), 66);
        assert!(c.rules.items.id_of("white_lightning_ii").is_some());
        assert!(c.rules.items.id_of("deathbringer").is_some());
        Ok(())
    }

    #[test]
    fn item_movement_is_in_half_moves_whole_moves_doubled() -> TestResult {
        use f4x_core::rules::effects::Effect;
        use f4x_core::rules::{ModOp, Stat};
        let c = load_content(&crate::testutil::content_dir())?;
        let mut seen = 0;
        for item in c.rules.items.defs() {
            for rule in &item.effects {
                if let Effect::Modify {
                    stat: Stat::HalfMoves,
                    op: ModOp::Add,
                    value,
                } = rule.effect
                {
                    seen += 1;
                    assert!(
                        (2..=8).contains(&value) && value % 2 == 0,
                        "{} has {value} half-moves",
                        item.id
                    );
                }
            }
        }
        assert_eq!(seen, 58);
        Ok(())
    }

    #[test]
    fn every_special_item_power_does_something_and_its_spell_exists() -> TestResult {
        let c = load_content(&crate::testutil::content_dir())?;
        for power in c.rules.item_powers.defs() {
            if power.bonus.is_none() {
                assert!(
                    f4x_core::item::power_is_handled(power),
                    "item power {} has no effect in the rules",
                    power.id
                );
            }
            if let Some(spell) = &power.spell {
                assert!(
                    spell.id().is_some(),
                    "item power {} names the missing spell {spell:?}",
                    power.id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn any_wizard_can_be_given_any_prefab_item_once() {
        use f4x_core::item::make::{make_item, Grade};
        let c = real_content();
        let rules = &c.rules;
        // The wizard's books play no part in generating an item, only in crafting.
        let mut rng = f4x_core::game::rng::GameRng::new(3);
        let mut taken = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..2000 {
            let item = make_item(&mut rng, rules, Grade::Any, 0, &mut taken);
            assert!(item.cost > 0 && (!item.name.is_empty() || item.made.is_some()));
            if let Some(id) = item.prefab {
                assert!(!ids.contains(&id), "{id:?} given twice");
                ids.push(id);
            }
        }
        assert!(ids.len() > 150, "{}", ids.len());
        // A prefab item of a value lies between half of it and all of it.
        for seed in 0..100 {
            let mut taken = Vec::new();
            let item = make_item(
                &mut f4x_core::game::rng::GameRng::new(seed),
                rules,
                Grade::Artifact,
                1200,
                &mut taken,
            );
            if item.prefab.is_some() {
                assert!((600..=1200).contains(&item.cost), "{item:?}");
            }
        }
    }

    #[test]
    fn random_items_stay_within_their_grade() {
        use f4x_core::item::make::{random_item, Grade};
        let c = real_content();
        let rules = &c.rules;
        for seed in 0..300 {
            let mut rng = f4x_core::game::rng::GameRng::new(seed);
            // An Enchant Item grade item has at most two plain powers, each of 200 or less.
            let item = random_item(&mut rng, rules, Grade::Enchant, 0);
            assert!(
                item.powers.is_empty() && item.cost < 300 + 2 * 400 + 100,
                "{item:?}"
            );
            let item = random_item(&mut rng, rules, Grade::Artifact, 0);
            assert!(
                item.powers.is_empty() && (!item.name.is_empty() || item.made.is_some()),
                "{item:?}"
            );
        }
    }

    #[test]
    fn generated_maps_obey_placement_rules() -> TestResult {
        let c = real_content();
        let p = f4x_core::map::mapgen::GenParams::default();
        for seed in 0..5 {
            let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
            let towers = |plane| -> Vec<(u32, u32)> {
                feature_ids(&map, plane, &c)
                    .into_iter()
                    .filter(|(_, _, id)| id == "tower")
                    .map(|(x, y, _)| (x, y))
                    .collect()
            };
            // Towers: right number, identical coordinates on both planes.
            assert_eq!(
                towers(Plane::Arcanus).len(),
                p.tower_count as usize,
                "seed {seed}"
            );
            assert_eq!(towers(Plane::Arcanus), towers(Plane::Myrror), "seed {seed}");
            for plane in Plane::ALL {
                let grid = map.map.plane(plane);
                let nodes = feature_ids(&map, plane, &c)
                    .into_iter()
                    .filter(|(_, _, id)| id.ends_with("_node"))
                    .count();
                assert_eq!(
                    nodes,
                    p.planes[plane.index()].node_count as usize,
                    "seed {seed}"
                );
                // No feature sits on ocean; every feature is on terrain its rules allow.
                for (x, y, id) in feature_ids(&map, plane, &c) {
                    let class = c
                        .terrains
                        .get(grid.get(x as i32, y as i32).ok_or("the value is missing")?)
                        .ok_or("the value is missing")?
                        .class;
                    assert_ne!(class, TerrainClass::Ocean, "{id} on ocean, seed {seed}");
                    let def = c
                        .features
                        .get(c.features.id_of(&id).ok_or("the value is missing")?)
                        .ok_or("the value is missing")?;
                    if def.kind == FeatureKind::Node {
                        // A node's square has its own base terrain, and no river runs there.
                        let realm = id.strip_suffix("_node").ok_or("the value is missing")?;
                        assert_eq!(
                            Some(class),
                            TerrainClass::of_node_realm(realm),
                            "{id}'s square, seed {seed}"
                        );
                        assert!(!grid.has_river(x as i32, y as i32), "{id}, seed {seed}");
                    } else if def.kind != FeatureKind::Tower {
                        assert!(
                            c.features
                                .weighted(def.kind, class, plane)
                                .iter()
                                .any(|(f, _)| c.features.get(*f).is_some_and(|d| d.id == id)),
                            "{id} not allowed on {class:?}/{plane:?}, seed {seed}"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn myrror_has_its_own_minerals_and_more_of_them() -> TestResult {
        let c = real_content();
        let p = f4x_core::map::mapgen::GenParams::default();
        let (mut arc_minerals, mut myr_minerals) = (0, 0);
        for seed in 0..10 {
            let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
            let arcanus = feature_ids(&map, Plane::Arcanus, &c);
            let myrror = feature_ids(&map, Plane::Myrror, &c);
            let is_myrror_only = |id: &str| id == "adamantium_ore" || id == "crysx_crystal";
            assert!(
                arcanus.iter().all(|(_, _, id)| !is_myrror_only(id)),
                "seed {seed}"
            );
            let is_mineral = |id: &String| {
                c.features
                    .id_of(id)
                    .and_then(|f| c.features.get(f))
                    .is_some_and(|f| f.kind == FeatureKind::Mineral)
            };
            arc_minerals += arcanus.iter().filter(|(_, _, id)| is_mineral(id)).count();
            myr_minerals += myrror.iter().filter(|(_, _, id)| is_mineral(id)).count();
        }
        assert!(
            myr_minerals > arc_minerals,
            "myrror {myr_minerals} vs arcanus {arc_minerals}"
        );
        Ok(())
    }

    #[test]
    fn myrror_only_minerals_appear_across_many_seeds() -> TestResult {
        let c = real_content();
        let p = f4x_core::map::mapgen::GenParams::default();
        let mut found = false;
        for seed in 0..10 {
            let map = f4x_core::map::mapgen::generate(seed, &p, &c)?;
            found |= feature_ids(&map, Plane::Myrror, &c)
                .iter()
                .any(|(_, _, id)| id == "adamantium_ore");
            if found {
                break;
            }
        }
        assert!(found, "adamantium never generated on Myrror");
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
    fn the_shipped_client_settings_name_the_language_and_every_switch() -> TestResult {
        let dir = crate::testutil::content_dir();
        let c = load_client_settings(&[&dir])?;
        assert_eq!(c.language, "en");
        assert_eq!(c.preferences.len(), 19);
        assert!(!c.preferences["strategic_combat_only"] && c.preferences["sound_effects"]);
        Ok(())
    }

    #[test]
    fn a_client_settings_file_can_change_the_language_and_must_parse() -> TestResult {
        let over = f4x_core::testdir::TestDir::new("client-settings");
        assert_eq!(load_client_settings(&[&over])?, Default::default());
        std::fs::write(over.join("client_settings.ron"), r#"(language: "de")"#)?;
        assert_eq!(load_client_settings(&[&over])?.language, "de");
        std::fs::write(over.join("client_settings.ron"), "(language: ")?;
        assert!(matches!(
            load_client_settings(&[&over]),
            Err(ContentError::Parse(_))
        ));
        Ok(())
    }

    #[test]
    fn content_without_settings_leaves_them_empty_and_a_local_file_wins() -> TestResult {
        let none = f4x_core::testdir::TestDir::new("settings-none");
        assert_eq!(load_settings(&[&none])?, SettingsDef::default());
        let over = f4x_core::testdir::TestDir::new("settings-layer");
        std::fs::write(
            over.join("rules_settings.ron"),
            "(new_game: (opponents: 3))",
        )?;
        let committed = crate::testutil::content_dir();
        let c = load_content_layers(&[&over, &committed])?;
        assert_eq!(c.settings.new_game.opponents, 3);
        assert!(c.settings.levels.is_empty());
        Ok(())
    }

    #[test]
    fn a_settings_file_out_of_range_is_refused() -> TestResult {
        let over = f4x_core::testdir::TestDir::new("settings-bad");
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
        let mut default = f4x_core::game::setting::SpellLibraryDef::default();
        default.resolve(c.rules.spells.defs(), &c.rules.retorts, &c.rules.realms);
        assert_eq!(c.settings.spell_library, default);
    }

    #[test]
    fn a_spell_library_naming_an_unknown_spell_or_realm_is_refused() -> TestResult {
        let over = f4x_core::testdir::TestDir::new("settings-spells");
        let committed = crate::testutil::content_dir();
        std::fs::write(
            over.join("rules_settings.ron"),
            r#"(spell_library: (always_known: ["no_such_spell"]))"#,
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("no_such_spell"), "{err}");
        std::fs::write(
            over.join("rules_settings.ron"),
            r#"(spell_library: (ai_main_realm_known: {"no_realm": "sprites"}))"#,
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("no_realm"), "{err}");
        std::fs::write(
            over.join("rules_settings.ron"),
            "(spell_library: (known: []))",
        )?;
        let err = load_content_layers(&[&over, &committed]).unwrap_err();
        assert!(err.to_string().contains("0 rows"), "{err}");
        Ok(())
    }

    /// A building's help topic is its description in the city builder: it names the topic,
    /// is titled with the building's name, and gives each plain effect with its number.
    #[test]
    fn every_building_has_a_help_topic_that_gives_its_effects_with_their_numbers() -> TestResult {
        use f4x_core::rules::effects::{Effect, Trigger};
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        let content = load_content(&dir)?;
        for (_, def) in content.rules.buildings.iter() {
            let topic = help
                .topic_of(def.help)
                .ok_or_else(|| format!("{} has no help topic", def.id))?;
            assert_eq!(topic.title, def.name, "{}", def.id);
            for rule in &def.effects {
                if let (Trigger::Passive, Effect::Modify { value, .. }) = (&rule.when, &rule.effect)
                {
                    assert!(
                        topic.text.contains(&value.abs().to_string()),
                        "the help of {} leaves out {value}: {}",
                        def.id,
                        topic.text
                    );
                }
            }
            if !def.requires.is_empty() {
                assert!(topic.text.contains("Needs"), "{}: {}", def.id, topic.text);
            }
        }
        Ok(())
    }

    #[test]
    fn each_townsfolk_topic_names_the_buildings_its_race_cannot_build() -> TestResult {
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        let content = load_content(&dir)?;
        assert_eq!(content.rules.races.iter().count(), 14);
        for (_, r) in content.rules.races.iter() {
            let index = r
                .help
                .ok_or_else(|| format!("{} has no help topic", r.id))?;
            let topic = help.topic(index).ok_or("the value is missing")?;
            for (_, b) in content.rules.buildings.iter() {
                if !b.buildable_by(f4x_core::rules::RaceId::new(r.num)) {
                    assert!(
                        topic.text.contains(&b.name),
                        "topic {index} ({}) does not name the {}",
                        r.id,
                        b.name
                    );
                }
            }
        }
        Ok(())
    }

    /// A retort, a race and a spell realm each name a help topic that exists and has text; a
    /// retort's topic is titled with its name.
    #[test]
    fn every_retort_race_and_realm_has_a_help_topic() -> TestResult {
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        let content = load_content(&dir)?;
        for r in content.rules.retorts.defs() {
            let topic = help
                .topic_of(r.help)
                .ok_or_else(|| format!("retort {} has no help topic", r.id))?;
            assert_eq!(topic.title, r.name, "retort {}", r.id);
            assert!(!topic.text.trim().is_empty(), "retort {}", r.id);
        }
        for r in content.rules.races.defs() {
            assert!(help.topic_of(r.help).is_some(), "race {}", r.id);
        }
        for r in content.rules.realms.defs() {
            // The arcane realm has no spells of its own to explain.
            if r.id != "arcane" {
                assert!(help.topic_of(r.help).is_some(), "realm {}", r.id);
            }
        }
        Ok(())
    }

    #[test]
    fn the_shipped_help_is_consistent_and_every_topic_is_opened_by_an_area() -> TestResult {
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        assert_eq!(help.screens.len(), 41);
        assert!(!help.topics.is_empty());
        // The game's own text is written for areas that open it: no orphan topic.
        let mut opened: std::collections::HashSet<i32> = help
            .screens
            .iter()
            .flat_map(|s| s.fields.iter().map(|f| f.help))
            .collect();
        // A topic that another topic includes is shown with it.
        opened.extend(help.topics.iter().filter_map(|t| t.include.map(i32::from)));
        // An ability, a spell, a building or a ready-made wizard, a retort, a race or a realm names its own topic.
        let content = load_content(&dir)?;
        opened.extend(
            content
                .rules
                .abilities
                .defs()
                .iter()
                .filter_map(|a| a.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .spells
                .defs()
                .iter()
                .filter_map(|s| s.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .wizards
                .defs()
                .iter()
                .map(|w| i32::from(w.help)),
        );
        opened.extend(
            content
                .rules
                .buildings
                .defs()
                .iter()
                .filter_map(|b| b.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .retorts
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .races
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        opened.extend(
            content
                .rules
                .realms
                .defs()
                .iter()
                .filter_map(|r| r.help.map(i32::from)),
        );
        // The three switches of the Settings screen the original has no topic for (the screen's
        // `help_topic` in the client names them).
        opened.extend([807, 808, 809]);
        for t in &help.topics {
            assert!(
                opened.contains(&i32::from(t.index)),
                "topic {} has no area",
                t.index
            );
            assert!(!t.text.trim().is_empty(), "topic {} is empty", t.index);
        }
        // The main screen's Done button is first at a point that Next Turn also covers.
        let (topic, _) = help.at(1, 250, 178).ok_or("the value is missing")?;
        assert_eq!(topic.index, 223);
        let (topic, _) = help
            .at_where(1, 250, 178, |t| t != 223)
            .ok_or("the value is missing")?;
        assert_eq!(topic.index, 229);
        Ok(())
    }

    /// A ready-made wizard names its help topic (`help` in `wizards.ron`); the topic is titled
    /// with the wizard's name, and the text is ours.
    #[test]
    fn every_ready_made_wizard_has_a_help_topic_by_number() -> TestResult {
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        let content = load_content(&dir)?;
        for w in content.rules.wizards.defs() {
            let topic = help
                .topics
                .iter()
                .find(|t| t.index == w.help)
                .ok_or_else(|| format!("no help topic {} for the wizard {}", w.help, w.num))?;
            assert_eq!(topic.title, w.name, "wizard {}", w.num);
            assert!(!topic.text.trim().is_empty(), "wizard {}", w.num);
        }
        Ok(())
    }

    #[test]
    fn every_ability_and_every_spell_has_a_help_topic() -> TestResult {
        let dir = crate::testutil::content_dir();
        let help = load_help(&dir)?;
        let content = load_content(&dir)?;
        let mut used = std::collections::HashSet::new();
        for ability in content.rules.abilities.defs() {
            let topic = help.topic_of(ability.help);
            assert!(
                topic.is_some_and(|t| !t.text.trim().is_empty()),
                "no help for the ability {}",
                ability.id
            );
            assert!(
                used.insert(ability.help),
                "ability {} shares its topic",
                ability.id
            );
        }
        for spell in content.rules.spells.defs() {
            let topic = help.topic_of(spell.help);
            assert!(
                topic.is_some_and(|t| !t.text.trim().is_empty()),
                "no help for the spell {}",
                spell.id
            );
            assert!(
                used.insert(spell.help),
                "spell {} shares its topic",
                spell.id
            );
        }
        Ok(())
    }
}
