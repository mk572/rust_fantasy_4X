//! Loading and validation of game content files.

pub mod rules;
pub mod save;

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use f4x_core::content::{
    Content, FeatureDef, FeatureKind, Features, TerrainClass, TerrainDef, Terrains,
    REQUIRED_CLASSES,
};
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
        if d.name.trim().is_empty() {
            problems.push(format!("terrain {:?} has an empty name", d.id));
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
    for d in defs {
        problems.extend(id_problem("feature", &d.id));
        if !seen.insert(d.id.as_str()) {
            problems.push(format!("duplicate feature id {:?}", d.id));
        }
        if d.name.trim().is_empty() {
            problems.push(format!("feature {:?} has an empty name", d.id));
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

/// Load the optional help files (`help.ron`, `help_screens.ron`) from a content
/// directory. A directory without them yields an empty [`Help`].
pub fn load_help(dir: &Path) -> Result<f4x_core::help::Help, ContentError> {
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
    let help = f4x_core::help::Help {
        topics: optional(dir, "help.ron")?,
        screens: optional(dir, "help_screens.ron")?,
    };
    let problems = help.problems();
    if problems.is_empty() {
        Ok(help)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// Load and validate all content files from a content directory.
pub fn load_content(dir: &Path) -> Result<Content, ContentError> {
    let read = |name: &str| {
        let path = dir.join(name);
        std::fs::read_to_string(&path)
            .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))
    };
    let terrains = parse_terrains(&read("terrain.ron")?)?;
    let features = parse_features(&read("features.ron")?)?;
    let rules = rules::load_rules(dir, &features)?;
    Ok(Content {
        terrains,
        features,
        rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/terrain.ron");
        std::fs::read_to_string(path).expect("content/terrain.ron exists")
    }

    fn shipped_features() -> Features {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/features.ron");
        parse_features(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn shipped_content() -> Content {
        load_content(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")).unwrap()
    }

    #[test]
    fn shipped_terrain_file_is_valid() {
        let t = parse_terrains(&shipped()).unwrap();
        assert!(t.id_of("grassland").is_some());
    }

    #[test]
    fn shipped_content_generates_a_map() {
        let t = parse_terrains(&shipped()).unwrap();
        let c = Content {
            terrains: t,
            features: shipped_features(),
            ..Default::default()
        };
        let g = f4x_core::Game::with_generated_map(1, &Default::default(), &c).unwrap();
        assert!(g.map.is_some());
    }

    #[test]
    fn reports_every_problem() {
        let src = shipped()
            .replace("id: \"forest\"", "id: \"Forest\"")
            .replace("class: Forest", "class: Hills");
        let Err(ContentError::Invalid(p)) = parse_terrains(&src) else {
            panic!("expected invalid")
        };
        assert!(p.iter().any(|m| m.contains("must be lowercase")));
        assert!(p.iter().any(|m| m.contains("no Forest terrain")));
    }

    #[test]
    fn reports_parse_errors() {
        assert!(matches!(
            parse_terrains("[(id: 1)]"),
            Err(ContentError::Parse(_))
        ));
    }

    fn feature_ids(grid: &f4x_core::map::Grid, c: &Content) -> Vec<(u32, u32, String)> {
        let mut out = Vec::new();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if let Some(f) = grid.feature(x as i32, y as i32) {
                    out.push((x, y, c.features.get(f).unwrap().id.clone()));
                }
            }
        }
        out
    }

    #[test]
    fn features_validation_reports_problems() {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/features.ron"),
        )
        .unwrap();
        let broken = src
            .replace("id: \"coal\"", "id: \"iron_ore\"")
            .replace("kind: Tower", "kind: Mineral");
        let Err(ContentError::Invalid(p)) = parse_features(&broken) else {
            panic!("expected invalid")
        };
        assert!(p.iter().any(|m| m.contains("duplicate feature id")));
        assert!(p.iter().any(|m| m.contains("no Tower feature")));
        assert!(matches!(
            parse_features("[(id: 1)]"),
            Err(ContentError::Parse(_))
        ));
    }

    #[test]
    fn generated_maps_obey_placement_rules() {
        let c = shipped_content();
        let p = f4x_core::mapgen::GenParams::default();
        for seed in 0..5 {
            let map = f4x_core::mapgen::generate(seed, &p, &c).unwrap();
            let towers = |plane| -> Vec<(u32, u32)> {
                feature_ids(map.plane(plane), &c)
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
                let grid = map.plane(plane);
                let nodes = feature_ids(grid, &c)
                    .into_iter()
                    .filter(|(_, _, id)| id.ends_with("_node"))
                    .count();
                assert_eq!(
                    nodes,
                    p.planes[plane.index()].node_count as usize,
                    "seed {seed}"
                );
                // No feature sits on ocean; every feature is on terrain its rules allow.
                for (x, y, id) in feature_ids(grid, &c) {
                    let class = c
                        .terrains
                        .get(grid.get(x as i32, y as i32).unwrap())
                        .unwrap()
                        .class;
                    assert_ne!(class, TerrainClass::Ocean, "{id} on ocean, seed {seed}");
                    let def = c.features.get(c.features.id_of(&id).unwrap()).unwrap();
                    if def.kind != FeatureKind::Tower {
                        assert!(
                            c.features
                                .weighted(def.kind, class, plane)
                                .iter()
                                .any(|(f, _)| c.features.get(*f).unwrap().id == id),
                            "{id} not allowed on {class:?}/{plane:?}, seed {seed}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn myrror_has_its_own_minerals_and_more_of_them() {
        let c = shipped_content();
        let p = f4x_core::mapgen::GenParams::default();
        let (mut arc_minerals, mut myr_minerals) = (0, 0);
        for seed in 0..10 {
            let map = f4x_core::mapgen::generate(seed, &p, &c).unwrap();
            let arcanus = feature_ids(map.plane(Plane::Arcanus), &c);
            let myrror = feature_ids(map.plane(Plane::Myrror), &c);
            let is_myrror_only = |id: &str| id == "adamantium_ore" || id == "crysx_crystal";
            assert!(
                arcanus.iter().all(|(_, _, id)| !is_myrror_only(id)),
                "seed {seed}"
            );
            let is_mineral = |id: &String| {
                let f = c.features.get(c.features.id_of(id).unwrap()).unwrap();
                f.kind == FeatureKind::Mineral
            };
            arc_minerals += arcanus.iter().filter(|(_, _, id)| is_mineral(id)).count();
            myr_minerals += myrror.iter().filter(|(_, _, id)| is_mineral(id)).count();
        }
        assert!(
            myr_minerals > arc_minerals,
            "myrror {myr_minerals} vs arcanus {arc_minerals}"
        );
    }

    #[test]
    fn myrror_only_minerals_appear_across_many_seeds() {
        let c = shipped_content();
        let p = f4x_core::mapgen::GenParams::default();
        let found = (0..10).any(|seed| {
            let map = f4x_core::mapgen::generate(seed, &p, &c).unwrap();
            feature_ids(map.plane(Plane::Myrror), &c)
                .iter()
                .any(|(_, _, id)| id == "adamantium_ore")
        });
        assert!(found, "adamantium never generated on Myrror");
    }
}
