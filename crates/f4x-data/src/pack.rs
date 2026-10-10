//! Content packs: a directory with a `pack.ron` manifest and any subset of the data
//! files. Packs load in order; a later pack overrides earlier definitions that share
//! an id, adds new ones, and can remove earlier ones. The game ships one `base`
//! pack; mods are further packs.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use f4x_core::art::{key, Art, ArtEntry, PackInfo, ART_KINDS};
use f4x_core::content::{Content, FeatureDef, Features, TerrainDef, Terrains};
use f4x_core::rules::{
    AbilityDef, BuildingDef, Def, ItemDef, LevelPathDef, RaceDef, RealmDef, Registry, Rules,
    RulesConfig, SpellDef, UnitDef,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::rules::validate_rules;
use crate::{validate_features, validate_terrains, ContentError};

/// `pack.ron`: describes a pack and how it relates to others.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// Pack ids that must be loaded before this one.
    #[serde(default)]
    pub depends: Vec<String>,
    /// Definitions to remove from earlier packs, as `"<kind>:<id>"` (for example
    /// `"unit:cave_troll"`); art entries are `"art:<kind>:<id>"`.
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Default)]
struct Raw {
    config: RulesConfig,
    terrains: Vec<TerrainDef>,
    features: Vec<FeatureDef>,
    races: Vec<RaceDef>,
    units: Vec<UnitDef>,
    abilities: Vec<AbilityDef>,
    buildings: Vec<BuildingDef>,
    realms: Vec<RealmDef>,
    spells: Vec<SpellDef>,
    items: Vec<ItemDef>,
    level_paths: Vec<LevelPathDef>,
    art: Vec<ArtEntry>,
}

/// Read and parse `dir/name`; `Ok(None)` if the file does not exist.
fn read_file<T: DeserializeOwned>(
    dir: &Path,
    name: &str,
    pack: &str,
) -> Result<Option<T>, ContentError> {
    let path = dir.join(name);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ContentError::Io(format!("{}: {e}", path.display()))),
    };
    ron::from_str(&src)
        .map(Some)
        .map_err(|e| ContentError::Parse(format!("pack {pack:?}, {name}: {e}")))
}

/// Merge `incoming` into `acc`: same id replaces in place (keeping its position),
/// new ids append. Ids repeated inside `incoming` itself are reported.
fn merge<T: Def>(
    acc: &mut Vec<T>,
    incoming: Vec<T>,
    file: &str,
    pack: &str,
    problems: &mut Vec<String>,
) {
    let mut seen = HashSet::new();
    for d in &incoming {
        if !seen.insert(d.id().to_string()) {
            problems.push(format!(
                "pack {pack:?}: duplicate id {:?} in {file}",
                d.id()
            ));
        }
    }
    for d in incoming {
        match acc.iter().position(|x| x.id() == d.id()) {
            Some(i) => acc[i] = d,
            None => acc.push(d),
        }
    }
}

fn remove<T: Def>(
    acc: &mut Vec<T>,
    kind: &str,
    ids: &[(&str, &str)],
    pack: &str,
    problems: &mut Vec<String>,
) {
    for (_, id) in ids.iter().filter(|(k, _)| *k == kind) {
        match acc.iter().position(|x| x.id() == *id) {
            Some(i) => {
                acc.remove(i);
            }
            None => problems.push(format!("pack {pack:?} removes unknown {kind} {id:?}")),
        }
    }
}

/// Load packs in the given order (base first) and validate the combined result.
pub fn load_packs(dirs: &[PathBuf]) -> Result<Content, ContentError> {
    let mut raw = Raw::default();
    let mut packs: Vec<PackInfo> = Vec::new();
    let mut problems = Vec::new();

    for dir in dirs {
        let manifest: Manifest = read_file(dir, "pack.ron", &dir.display().to_string())?
            .ok_or_else(|| ContentError::Io(format!("{}: missing pack.ron", dir.display())))?;
        let pack = manifest.id.clone();
        if packs.iter().any(|p| p.id == pack) {
            problems.push(format!("pack {pack:?} is loaded twice"));
        }
        for dep in &manifest.depends {
            if !packs.iter().any(|p| &p.id == dep) {
                problems.push(format!(
                    "pack {pack:?} depends on {dep:?}, which is not loaded before it"
                ));
            }
        }
        packs.push(PackInfo {
            id: pack.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            dir: dir.display().to_string(),
        });

        // Removals first, so a pack can remove an entry and define its own version.
        let removals: Vec<(&str, &str)> = manifest
            .remove
            .iter()
            .filter_map(|r| {
                let parsed = r.split_once(':');
                if parsed.is_none() {
                    problems.push(format!("pack {pack:?}: malformed remove entry {r:?}"));
                }
                parsed
            })
            .collect();
        remove(
            &mut raw.terrains,
            "terrain",
            &removals,
            &pack,
            &mut problems,
        );
        remove(
            &mut raw.features,
            "feature",
            &removals,
            &pack,
            &mut problems,
        );
        remove(&mut raw.races, "race", &removals, &pack, &mut problems);
        remove(&mut raw.units, "unit", &removals, &pack, &mut problems);
        remove(
            &mut raw.abilities,
            "ability",
            &removals,
            &pack,
            &mut problems,
        );
        remove(
            &mut raw.buildings,
            "building",
            &removals,
            &pack,
            &mut problems,
        );
        remove(&mut raw.realms, "realm", &removals, &pack, &mut problems);
        remove(&mut raw.spells, "spell", &removals, &pack, &mut problems);
        remove(&mut raw.items, "item", &removals, &pack, &mut problems);
        remove(
            &mut raw.level_paths,
            "level_path",
            &removals,
            &pack,
            &mut problems,
        );
        let art_removals: Vec<(&str, &str)> = removals
            .iter()
            .filter(|(k, _)| *k == "art")
            .copied()
            .collect();
        for (_, art_key) in &art_removals {
            match raw.art.iter().position(|a| a.key == *art_key) {
                Some(i) => {
                    raw.art.remove(i);
                }
                None => problems.push(format!("pack {pack:?} removes unknown art {art_key:?}")),
            }
        }
        let known_kinds = [
            "terrain",
            "feature",
            "race",
            "unit",
            "ability",
            "building",
            "realm",
            "spell",
            "item",
            "level_path",
            "art",
        ];
        for (k, id) in &removals {
            if !known_kinds.contains(k) {
                problems.push(format!(
                    "pack {pack:?}: cannot remove {k}:{id} (unknown kind)"
                ));
            }
        }

        macro_rules! load {
            ($field:ident, $file:literal, $ty:ty) => {
                if let Some(list) = read_file::<Vec<$ty>>(dir, $file, &pack)? {
                    merge(&mut raw.$field, list, $file, &pack, &mut problems);
                }
            };
        }
        load!(terrains, "terrain.ron", TerrainDef);
        load!(features, "features.ron", FeatureDef);
        load!(races, "races.ron", RaceDef);
        load!(units, "units.ron", UnitDef);
        load!(abilities, "abilities.ron", AbilityDef);
        load!(buildings, "buildings.ron", BuildingDef);
        load!(realms, "realms.ron", RealmDef);
        load!(spells, "spells.ron", SpellDef);
        load!(items, "items.ron", ItemDef);
        load!(level_paths, "level_paths.ron", LevelPathDef);
        if let Some(mut list) = read_file::<Vec<ArtEntry>>(dir, "art.ron", &pack)? {
            list.iter_mut().for_each(|a| a.pack = pack.clone());
            merge(&mut raw.art, list, "art.ron", &pack, &mut problems);
        }
        if let Some(config) = read_file::<RulesConfig>(dir, "rules.ron", &pack)? {
            raw.config = config;
        }
    }

    problems.extend(validate_terrains(&raw.terrains));
    problems.extend(validate_features(&raw.features));
    let rules = Rules {
        config: raw.config,
        races: Registry::new(raw.races),
        units: Registry::new(raw.units),
        abilities: Registry::new(raw.abilities),
        buildings: Registry::new(raw.buildings),
        realms: Registry::new(raw.realms),
        spells: Registry::new(raw.spells),
        items: Registry::new(raw.items),
        level_paths: Registry::new(raw.level_paths),
    };
    let features = Features::new(raw.features);
    problems.extend(validate_rules(&rules, &features));
    let content = Content {
        terrains: Terrains::new(raw.terrains),
        features,
        rules,
        art: Art::new(raw.art),
        packs,
    };
    problems.extend(validate_art(&content));
    if problems.is_empty() {
        Ok(content)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// Load the single pack at `dir`.
pub fn load_content(dir: &Path) -> Result<Content, ContentError> {
    load_packs(&[dir.to_path_buf()])
}

/// Every art key a definition in `c` could have.
fn known_art_keys(c: &Content) -> HashSet<String> {
    let mut keys = HashSet::new();
    let mut add = |kind: &str, ids: Vec<&str>| {
        keys.extend(ids.into_iter().map(|id| key(kind, id)));
    };
    add(
        "terrain",
        c.terrains.defs().iter().map(|d| d.id.as_str()).collect(),
    );
    add(
        "feature",
        c.features.defs().iter().map(|d| d.id.as_str()).collect(),
    );
    add(
        "race",
        c.rules.races.defs().iter().map(|d| d.id.as_str()).collect(),
    );
    add(
        "unit",
        c.rules.units.defs().iter().map(|d| d.id.as_str()).collect(),
    );
    add(
        "ability",
        c.rules
            .abilities
            .defs()
            .iter()
            .map(|d| d.id.as_str())
            .collect(),
    );
    add(
        "building",
        c.rules
            .buildings
            .defs()
            .iter()
            .map(|d| d.id.as_str())
            .collect(),
    );
    add(
        "realm",
        c.rules
            .realms
            .defs()
            .iter()
            .map(|d| d.id.as_str())
            .collect(),
    );
    add(
        "spell",
        c.rules
            .spells
            .defs()
            .iter()
            .map(|d| d.id.as_str())
            .collect(),
    );
    add(
        "item",
        c.rules.items.defs().iter().map(|d| d.id.as_str()).collect(),
    );
    keys
}

/// A sprite path must stay inside the pack's `art/` directory.
fn safe_relative(file: &str) -> bool {
    let p = Path::new(file);
    !file.is_empty() && p.is_relative() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

pub fn validate_art(c: &Content) -> Vec<String> {
    let known = known_art_keys(c);
    let mut problems = Vec::new();
    for a in c.art.defs() {
        let who = format!("art entry {:?}", a.key);
        match a.key.split_once(':') {
            Some((kind, _)) if ART_KINDS.contains(&kind) => {
                if !known.contains(&a.key) {
                    problems.push(format!("{who} refers to a definition that does not exist"));
                }
            }
            _ => problems.push(format!(
                "{who}: key must be \"<kind>:<id>\" with kind one of {ART_KINDS:?}"
            )),
        }
        for (what, sprite) in [("sprite", &a.sprite), ("icon", &a.icon)] {
            let Some(s) = sprite else { continue };
            if !safe_relative(&s.file) {
                problems.push(format!(
                    "{who}: {what} file {:?} must be a relative path inside art/",
                    s.file
                ));
            }
            if s.frames == 0 {
                problems.push(format!("{who}: {what} needs at least 1 frame"));
            }
            if s.frame_size.is_some_and(|(w, h)| w == 0 || h == 0) {
                problems.push(format!("{who}: {what} frame_size must be nonzero"));
            }
        }
    }
    problems
}

/// Definitions that have no art entry (they will use a placeholder), as art keys.
pub fn defs_without_art(c: &Content) -> Vec<String> {
    let mut keys: Vec<String> = known_art_keys(c)
        .into_iter()
        .filter(|k| c.art.id_of(k).is_none())
        .collect();
    keys.sort();
    keys
}

/// Sprite files referenced by art entries that do not exist on disk. Missing art is
/// not an error (the client falls back to the entry's colour), so this is advisory.
pub fn missing_art_files(c: &Content) -> Vec<String> {
    let mut missing = Vec::new();
    for a in c.art.defs() {
        let Some(pack) = c.packs.iter().find(|p| p.id == a.pack) else {
            continue;
        };
        for s in [&a.sprite, &a.icon].into_iter().flatten() {
            if safe_relative(&s.file) && !Path::new(&pack.dir).join("art").join(&s.file).exists() {
                missing.push(format!("{}: {} (pack {})", a.key, s.file, pack.id));
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{base_dir, has, Scratch};

    fn load_with_mod(files: &[(&str, &str)]) -> Result<Content, ContentError> {
        let m = Scratch::new();
        for (name, text) in files {
            m.write(name, text);
        }
        load_packs(&[base_dir(), m.0.clone()])
    }

    fn problems(r: Result<Content, ContentError>) -> Vec<String> {
        match r {
            Err(ContentError::Invalid(p)) => p,
            Err(e) => panic!("expected validation problems, got {e}"),
            Ok(_) => Vec::new(),
        }
    }

    const MANIFEST: &str = r#"(id: "mod", name: "Test Mod", version: "1.0", depends: ["base"])"#;

    #[test]
    fn base_pack_loads_with_art() {
        let c = load_content(&base_dir()).unwrap();
        assert_eq!(c.packs.len(), 1);
        assert_eq!(c.packs[0].id, "base");
        let art = c.art.by_name("unit:spearman").unwrap();
        assert_eq!(art.pack, "base");
        assert!(art.color.is_some());
    }

    #[test]
    fn mod_overrides_adds_and_keeps_order() {
        let c = load_with_mod(&[
            ("pack.ron", MANIFEST),
            (
                "units.ron",
                r#"[
                (id: "spearman", name: "Veteran Spearman", kind: Normal, race: Some("sunfolk"), cost: 25,
                 stats: {Melee: 4, Defense: 3, Resistance: 4, MaxHp: 1, Figures: 6, Moves: 1},
                 movement: Walking),
                (id: "pikeman", name: "Pikeman", kind: Normal, race: Some("sunfolk"), cost: 35,
                 stats: {Melee: 3, Defense: 4, Resistance: 4, MaxHp: 1, Figures: 6, Moves: 1},
                 movement: Walking),
            ]"#,
            ),
        ])
        .unwrap();
        let spear = c.rules.units.by_name("spearman").unwrap();
        assert_eq!((spear.name.as_str(), spear.cost), ("Veteran Spearman", 25));
        // Overriding keeps the original position; new units append.
        assert_eq!(c.rules.units.id_of("spearman").unwrap().index(), 0);
        assert!(c.rules.units.by_name("pikeman").is_some());
        assert_eq!(
            c.packs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["base", "mod"]
        );
    }

    #[test]
    fn mod_can_remove_and_replace() {
        let c = load_with_mod(&[
            (
                "pack.ron",
                r#"(id: "mod", name: "M", version: "1", depends: ["base"], remove: ["unit:cave_troll", "art:unit:cave_troll"])"#,
            ),
        ])
        .unwrap();
        assert!(c.rules.units.by_name("cave_troll").is_none());
        assert!(c.art.by_name("unit:cave_troll").is_none());
    }

    #[test]
    fn removing_something_still_referenced_is_an_error() {
        let p = problems(load_with_mod(&[(
            "pack.ron",
            r#"(id: "mod", name: "M", version: "1", depends: ["base"], remove: ["ability:armoured"])"#,
        )]));
        assert!(has(&p, "unknown ability \"armoured\""), "{p:?}");
    }

    #[test]
    fn manifest_problems_are_reported() {
        let p = problems(load_with_mod(&[(
            "pack.ron",
            r#"(id: "base", name: "M", version: "1", depends: ["missing"], remove: ["unit:nobody", "oops", "weird:thing"])"#,
        )]));
        assert!(has(&p, "loaded twice"), "{p:?}");
        assert!(has(&p, "depends on \"missing\""), "{p:?}");
        assert!(has(&p, "removes unknown unit \"nobody\""), "{p:?}");
        assert!(has(&p, "malformed remove entry"), "{p:?}");
        assert!(has(&p, "unknown kind"), "{p:?}");
        // A pack without a manifest is rejected outright.
        let empty = Scratch::new();
        assert!(matches!(
            load_packs(&[base_dir(), empty.0.clone()]),
            Err(ContentError::Io(_))
        ));
    }

    #[test]
    fn duplicate_ids_within_a_file_are_reported() {
        let p = problems(load_with_mod(&[
            ("pack.ron", MANIFEST),
            (
                "realms.ron",
                r#"[(id: "fire", name: "Fire", color: (1,2,3)), (id: "fire", name: "Fire2", color: (1,2,3))]"#,
            ),
        ]));
        assert!(has(&p, "duplicate id \"fire\" in realms.ron"), "{p:?}");
    }

    #[test]
    fn parse_errors_name_the_pack_and_file() {
        let r = load_with_mod(&[("pack.ron", MANIFEST), ("units.ron", "[(id: 7)]")]);
        let Err(ContentError::Parse(m)) = r else {
            panic!("expected parse error")
        };
        assert!(m.contains("\"mod\"") && m.contains("units.ron"), "{m}");
    }

    #[test]
    fn art_entries_are_validated() {
        let p = problems(load_with_mod(&[
            ("pack.ron", MANIFEST),
            (
                "art.ron",
                r#"[
                (key: "unit:nobody"),
                (key: "mystery:thing"),
                (key: "unit:spearman", sprite: Some((file: "../../etc/passwd"))),
                (key: "unit:archer",   sprite: Some((file: "/abs/path.png"))),
                (key: "unit:axeman",   sprite: Some((file: "ok.png", frames: 0))),
                (key: "unit:marksman", icon: Some((file: "ok.png", frame_size: Some((0, 8))))),
            ]"#,
            ),
        ]));
        assert!(
            has(
                &p,
                "\"unit:nobody\" refers to a definition that does not exist"
            ),
            "{p:?}"
        );
        assert!(has(&p, "\"mystery:thing\": key must be"), "{p:?}");
        assert_eq!(
            p.iter()
                .filter(|m| m.contains("must be a relative path"))
                .count(),
            2,
            "{p:?}"
        );
        assert!(has(&p, "at least 1 frame"), "{p:?}");
        assert!(has(&p, "frame_size must be nonzero"), "{p:?}");
    }

    #[test]
    fn missing_files_and_missing_art_are_advisory() {
        let m = Scratch::new();
        m.write("pack.ron", MANIFEST);
        m.write(
            "art.ron",
            r#"[(key: "unit:spearman", sprite: Some((file: "units/spearman.png")), icon: Some((file: "present.png")))]"#,
        );
        m.write("art/present.png", "x");
        let c = load_packs(&[base_dir(), m.0.clone()]).unwrap();
        let missing = missing_art_files(&c);
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert!(missing[0].contains("units/spearman.png"));
        let uncovered = defs_without_art(&c);
        assert!(uncovered.contains(&"ability:flying".to_string()));
        assert!(!uncovered.contains(&"unit:spearman".to_string()));
    }

    #[test]
    fn rules_config_is_replaced_by_a_later_pack() {
        let c = load_with_mod(&[
            ("pack.ron", MANIFEST),
            ("rules.ron", "(min_city_distance: 5)"),
        ])
        .unwrap();
        assert_eq!(c.rules.config.min_city_distance, 5);
        assert_eq!(
            c.rules.config.to_hit_percent, 30,
            "unspecified fields fall back to defaults"
        );
    }
}
