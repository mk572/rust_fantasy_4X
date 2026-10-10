//! Loading and validation of game content files.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use f4x_core::content::{TerrainClass, TerrainDef, Terrains, REQUIRED_CLASSES};
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

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/terrain.ron");
        std::fs::read_to_string(path).expect("content/terrain.ron exists")
    }

    #[test]
    fn shipped_terrain_file_is_valid() {
        let t = parse_terrains(&shipped()).unwrap();
        assert!(t.id_of("grassland").is_some());
    }

    #[test]
    fn shipped_content_generates_a_map() {
        let t = parse_terrains(&shipped()).unwrap();
        let g = f4x_core::Game::with_generated_map(1, &Default::default(), &t).unwrap();
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
}
