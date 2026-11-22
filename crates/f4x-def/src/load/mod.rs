//! Loading and validation of game content files.

pub mod rules;

use std::collections::HashSet;
use std::path::Path;

use crate::content::Content;
use crate::feature::{FeatureDef, FeatureKind, Features};
use crate::issue::Issues;
use crate::map::Plane;
use crate::map::{TerrainClass, TerrainDef, Terrains, REQUIRED_CLASSES};
use crate::settings::SettingsDef;

pub use crate::issue::{ContentError, ContentIssue, Element, IssueKind, Outcome};

pub fn parse_terrains(src: &str) -> Result<Terrains, ContentError> {
    let defs: Vec<TerrainDef> =
        ron::from_str(src).map_err(|e| ContentError::parse("terrain.ron", &e))?;
    let problems = validate_terrains(&defs);
    if problems.is_empty() {
        Ok(Terrains::new(defs))
    } else {
        Err(ContentError::new(problems))
    }
}

pub fn load_terrains(path: &Path) -> Result<Terrains, ContentError> {
    let src = std::fs::read_to_string(path).map_err(|e| ContentError::io(path, &e))?;
    parse_terrains(&src)
}

/// Every problem of the terrain rows; empty means valid.
pub fn validate_terrains(defs: &[TerrainDef]) -> Vec<ContentIssue> {
    const F: &str = "terrain.ron";
    let mut problems = Issues::new();
    let mut seen = HashSet::new();
    let mut nums = HashSet::new();
    for d in defs {
        let at = format!("terrain {:?}", d.id);
        problems.extend_opt(id_problem(F, "terrain", &d.id));
        if !seen.insert(d.id.as_str()) {
            problems.duplicate(F, &at, "the id", format!("{:?}", d.id));
        }
        if d.num == 0 {
            problems.out_of_range(F, &at, "num", 0, "above 0");
        } else if !nums.insert(d.num) {
            problems.duplicate(F, &at, "the number", d.num);
        }
        if d.planes.is_empty() {
            problems.missing(F, &at, "planes (it is available on no plane)");
        }
        match (d.class == TerrainClass::Ocean, d.walkable) {
            (true, true) => problems.conflict(F, &at, "an ocean terrain must not be walkable"),
            (false, false) => problems.conflict(F, &at, "a land terrain must be walkable"),
            _ => {}
        }
        if d.walkable && d.move_cost == 0 {
            problems.out_of_range(F, &at, "move_cost", 0, "at least 1 for a walkable terrain");
        }
    }
    for plane in Plane::all() {
        for &class in REQUIRED_CLASSES {
            if !defs
                .iter()
                .any(|d| d.class == class && d.planes.contains(&plane))
            {
                problems.missing(F, "", &format!("a {class:?} terrain for {plane:?}"));
            }
        }
    }
    problems.into_vec()
}

/// The issue for an id that is not a plain lowercase word, if it is not one.
pub(crate) fn id_problem(file: &str, kind: &str, id: &str) -> Option<ContentIssue> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    (!ok).then(|| {
        ContentIssue::new(file, format!("{kind} {id:?}"), IssueKind::OutOfRange).with([
            "the id".to_string(),
            format!("{id:?}"),
            "lowercase [a-z0-9_] and not empty".to_string(),
        ])
    })
}

pub fn parse_features(src: &str) -> Result<Features, ContentError> {
    let defs: Vec<FeatureDef> =
        ron::from_str(src).map_err(|e| ContentError::parse("features.ron", &e))?;
    let problems = validate_features(&defs);
    if problems.is_empty() {
        Ok(Features::new(defs))
    } else {
        Err(ContentError::new(problems))
    }
}

/// Every problem of the feature rows; empty means valid.
pub fn validate_features(defs: &[FeatureDef]) -> Vec<ContentIssue> {
    const F: &str = "features.ron";
    let mut problems = Issues::new();
    let mut seen = HashSet::new();
    let mut nums = HashSet::new();
    for d in defs {
        let at = format!("feature {:?}", d.id);
        problems.extend_opt(id_problem(F, "feature", &d.id));
        if !seen.insert(d.id.as_str()) {
            problems.duplicate(F, &at, "the id", format!("{:?}", d.id));
        }
        if d.num == 0 {
            problems.out_of_range(F, &at, "num", 0, "above 0");
        } else if !nums.insert(d.num) {
            problems.duplicate(F, &at, "the number", d.num);
        }
        if d.kind != FeatureKind::Tower && d.spawn.is_empty() {
            problems.missing(
                F,
                &at,
                &format!("spawn rules (a {:?} feature needs some)", d.kind),
            );
        }
        for (i, r) in d.spawn.iter().enumerate() {
            let at = format!("feature {:?} spawn rule {i}", d.id);
            if r.weight == 0 {
                problems.out_of_range(F, &at, "weight", 0, "at least 1");
            }
            if r.planes.is_empty() || r.terrain.is_empty() {
                problems.missing(F, &at, "a terrain class and a plane (at least one of each)");
            }
            if r.terrain.contains(&TerrainClass::Ocean) {
                problems.conflict(F, &at, "features cannot spawn on Ocean");
            }
        }
    }
    if !defs.iter().any(|d| d.kind == FeatureKind::Tower) {
        problems.missing(F, "", "a Tower feature");
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
                problems.missing(
                    F,
                    "",
                    &format!("a {kind:?} feature that can spawn on {plane:?}"),
                );
            }
        }
    }
    problems.into_vec()
}

/// Load the game's help: the topics' text in `lang` (`text/<lang>/help/help.ron`, the fallback
/// language's when a language has no file) and the screens' areas (`help_screens.ron`, no words),
/// each from the first of `dirs` that has it. The help text is ours: nothing else is read, so the
/// original's text is never a fallback. Directories without the files give an empty [`Help`].
pub fn load_help_checked(
    dirs: &[&Path],
    lang: &str,
) -> Result<(crate::help::Help, Vec<ContentIssue>), ContentError> {
    use crate::text::FALLBACK_LANGUAGE;
    let first = |name: &str| {
        dirs.iter()
            .map(|d| d.join(name))
            .find(|p| rules::is_there(p))
    };
    let topics_file = first(&format!("text/{lang}/help/help.ron"))
        .or_else(|| first(&format!("text/{FALLBACK_LANGUAGE}/help/help.ron")));
    let help = crate::help::Help {
        topics: read_list(topics_file.as_deref())?,
        screens: read_list(first("help_screens.ron").as_deref())?,
    };
    // A bad topic, include or picture is left out, and said so: the help is not the game.
    Ok(help.repaired())
}

/// [`load_help_checked`] without the list of what was left out, for a caller that is not told yet.
/// The client reads the list (task 768); until then use the checked form.
pub fn load_help(dirs: &[&Path], lang: &str) -> Result<crate::help::Help, ContentError> {
    load_help_checked(dirs, lang).map(|(help, _)| help)
}

/// The rows of a `.ron` list; no file gives none.
fn read_list<T: serde::de::DeserializeOwned>(path: Option<&Path>) -> Result<Vec<T>, ContentError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(path).map_err(|e| ContentError::io(path, &e))?;
    ron::from_str(&text).map_err(|e| ContentError::parse(&path.display().to_string(), &e))
}

/// The value of a step that worked; the issues of one that did not go to `issues`.
fn step_of<T>(r: Result<T, ContentError>, issues: &mut crate::issue::Issues) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            issues.extend_error(e);
            None
        }
    }
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
            .find(|p| rules::is_there(p))
            .unwrap_or_else(|| dirs.first().map_or_else(|| name.into(), |d| d.join(name)));
        std::fs::read_to_string(&path).map_err(|e| ContentError::io(&path, &e))
    };
    // Every file gets its turn, so one run reports the problems of all of them.
    let mut issues = crate::issue::Issues::new();
    let terrains = step_of(
        read("terrain.ron").and_then(|t| parse_terrains(&t)),
        &mut issues,
    );
    let features = step_of(
        read("features.ron").and_then(|t| parse_features(&t)),
        &mut issues,
    );
    let settings = step_of(load_settings(dirs), &mut issues);
    let rules = step_of(rules::load_rules(dirs), &mut issues);
    let (Some(terrains), Some(features), Some(settings), Some(rules)) =
        (terrains, features, settings, rules)
    else {
        return Err(ContentError::new(issues.into_vec()));
    };
    check_feature_references(&features, &rules)?;
    check_spell_library(&settings.spell_library, &rules)?;
    Ok(Content {
        terrains,
        features,
        rules,
        settings,
    })
}

/// What the features and the rules say about each other must exist: the realm of a node and of
/// the book a site gives, the mineral Transmute makes of one, the minerals a world event can put
/// on the map.
fn check_feature_references(
    features: &Features,
    rules: &crate::rules::Rules,
) -> Result<(), ContentError> {
    let has_feature = |num: u16| features.defs().iter().any(|f| f.num == num);
    let mut problems = Issues::new();
    for f in features.defs() {
        let at = format!("feature {:?}", f.id);
        for realm in f.realm.iter().chain(&f.reward) {
            if rules.realms.try_get(*realm).is_none() {
                problems.dangling("features.ron", &at, "realm or reward", "realm", realm.num());
            }
        }
        if let Some(n) = f.transmute.filter(|n| !has_feature(*n)) {
            problems.dangling("features.ron", &at, "transmute", "feature", n);
        }
    }
    for &n in rules.config.event_ores.iter().filter(|n| !has_feature(**n)) {
        problems.dangling("rules.ron", "", "event_ores", "feature", n);
    }
    problems.into_result(())
}

fn check_spell_library(
    library: &crate::settings::SpellLibraryDef,
    rules: &crate::rules::Rules,
) -> Result<(), ContentError> {
    const F: &str = "rules_settings.ron";
    let mut problems = Issues::new();
    let spells = library
        .always_known
        .iter()
        .chain(library.retort_known.values().flatten())
        .chain(&library.arcane_closed)
        .chain(library.ai_main_realm_known.values())
        .chain(library.ai_main_realm_researchable.values());
    for spell in spells {
        if rules.spells.try_get(*spell).is_none() {
            problems.dangling(F, "spell_library", "spells", "spell", spell.num());
        }
    }
    for retort in library.retort_known.keys() {
        if rules.retorts.try_get(*retort).is_none() {
            problems.dangling(F, "spell_library", "retort_known", "retort", retort.num());
        }
    }
    for realm in library
        .ai_main_realm_known
        .keys()
        .chain(library.ai_main_realm_researchable.keys())
    {
        if rules.realms.try_get(*realm).is_none() {
            problems.dangling(F, "spell_library", "ai_main_realm", "realm", realm.num());
        }
    }
    problems.into_result(())
}

/// Read `rules_settings.ron` from the first of `dirs` that has it. The file is required: a content
/// without it is refused (`docs/content-policy.md`, R1); one that is there must parse and be in range.
pub fn load_settings(dirs: &[&Path]) -> Result<SettingsDef, ContentError> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("rules_settings.ron"))
        .find(|p| rules::is_there(p))
    else {
        return Err(ContentError::new(vec![ContentIssue::new(
            "rules_settings.ron",
            "",
            IssueKind::Missing,
        )
        .with(["the file"])]));
    };
    let text = std::fs::read_to_string(&path).map_err(|e| ContentError::io(&path, &e))?;
    // The file states every setting; one it leaves out is not filled from a built-in (only a save
    // from before the setting existed is: `settings::with_fallback`).
    let (read, gaps) = crate::settings::collecting_gaps(|| ron::from_str::<SettingsDef>(&text));
    let settings = read.map_err(|e| ContentError::parse("rules_settings.ron", &e))?;
    if !gaps.is_empty() {
        return Err(ContentError::new(
            gaps.into_iter()
                .map(|name| {
                    ContentIssue::new("rules_settings.ron", name, IssueKind::Missing).with([name])
                })
                .collect(),
        ));
    }
    // The settings types are shared with the save files, which must go on reading a field a
    // later version dropped; so the unknown keys of this file are looked for here.
    let unknown = unknown_keys(&text, &settings);
    if !unknown.is_empty() {
        return Err(ContentError::new(
            unknown
                .into_iter()
                .map(|k| {
                    ContentIssue::new("rules_settings.ron", k.clone(), IssueKind::UnknownKey)
                        .with([k])
                })
                .collect(),
        ));
    }
    let problems = settings.validate();
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(ContentError::new(problems))
    }
}

/// The keys of `text` that the type it was read into does not have, by their path: the file is
/// read as a plain tree and held against the tree of the value written out again. A key that was
/// misspelled is dropped by the reader without a word otherwise.
fn unknown_keys<T: serde::Serialize>(text: &str, read: &T) -> Vec<String> {
    use ron::Value;
    let (Ok(file), Ok(written)) = (
        ron::from_str::<Value>(text),
        ron::to_string(read)
            .map_err(|_| ())
            .and_then(|t| ron::from_str::<Value>(&t).map_err(|_| ())),
    ) else {
        return Vec::new();
    };
    fn walk(file: &Value, kept: &Value, path: &str, out: &mut Vec<String>) {
        match (file, kept) {
            (Value::Map(f), Value::Map(k)) => {
                for (key, value) in f.iter() {
                    let name = match key {
                        Value::String(s) => s.clone(),
                        other => format!("{other:?}"),
                    };
                    let at = if path.is_empty() {
                        name
                    } else {
                        format!("{path}.{name}")
                    };
                    match k.iter().find(|(name, _)| *name == key) {
                        Some((_, kept)) => walk(value, kept, &at, out),
                        None => out.push(at),
                    }
                }
            }
            (Value::Seq(f), Value::Seq(k)) => {
                for (i, (f, k)) in f.iter().zip(k).enumerate() {
                    walk(f, k, &format!("{path}[{i}]"), out);
                }
            }
            (Value::Option(Some(f)), Value::Option(Some(k))) => walk(f, k, path, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&file, &written, "", &mut out);
    out
}
