//! Loading and validation of game content files.

pub mod rules;

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

impl ContentError {
    /// The error, saying which file it is about when a parse error does not already.
    fn in_file(self, file: &str) -> Self {
        match self {
            ContentError::Parse(m) if !m.contains(file) => {
                ContentError::Parse(format!("{file}: {m}"))
            }
            other => other,
        }
    }
}

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
            .find(|p| rules::is_there(p))
            .unwrap_or_else(|| dirs.first().map_or_else(|| name.into(), |d| d.join(name)));
        std::fs::read_to_string(&path)
            .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))
    };
    let terrains = parse_terrains(&read("terrain.ron")?).map_err(|e| e.in_file("terrain.ron"))?;
    let features = parse_features(&read("features.ron")?).map_err(|e| e.in_file("features.ron"))?;
    let settings = load_settings(dirs)?;
    let rules = rules::load_rules(dirs)?;
    check_feature_references(&features, &rules)?;
    if dirs
        .iter()
        .any(|d| rules::is_there(&d.join("rules_settings.ron")))
    {
        check_spell_library(&settings.spell_library, &rules)?;
    }
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
    let mut problems = Vec::new();
    for f in features.defs() {
        for realm in f.realm.iter().chain(&f.reward) {
            if rules.realms.try_get(*realm).is_none() {
                problems.push(format!(
                    "feature {:?} names unknown realm {}",
                    f.id,
                    realm.num()
                ));
            }
        }
        if let Some(n) = f.transmute.filter(|n| !has_feature(*n)) {
            problems.push(format!(
                "feature {:?} transmutes into unknown feature {n}",
                f.id
            ));
        }
    }
    for &n in rules.config.event_ores.iter().filter(|n| !has_feature(**n)) {
        problems.push(format!("rules.ron: event_ores names unknown feature {n}"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ContentError::Invalid(problems))
    }
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
        .find(|p| rules::is_there(p))
    else {
        return Ok(SettingsDef::default());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    let settings: SettingsDef = ron::from_str(&text)
        .map_err(|e| ContentError::Parse(format!("rules_settings.ron: {e}")))?;
    // The settings types are shared with the save files, which must go on reading a field a
    // later version dropped; so the unknown keys of this file are looked for here.
    let unknown = unknown_keys(&text, &settings);
    if !unknown.is_empty() {
        return Err(ContentError::Invalid(
            unknown
                .into_iter()
                .map(|k| format!("rules_settings.ron: {k} is not a setting"))
                .collect(),
        ));
    }
    let problems = settings.validate();
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(ContentError::Invalid(problems))
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
