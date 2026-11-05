//! Which picture each unit uses (`content/unit_art.ron`): its sprite sheet `(file, entry)` of the
//! original and, for a hero, its portrait entry of `PORTRAIT.LBX`. The rules do not know it; the
//! client and the extractor read it, and the client checks its ids against the rules
//! (`asset_check`).

use std::collections::BTreeSet;
use std::path::Path;

/// What the placer and the client need to know of a unit: its id, its art `(file, entry)` and its
/// portrait.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct UnitArt {
    pub id: String,
    pub art: Option<(String, usize)>,
    pub portrait: Option<usize>,
}

/// Pictures in the original's `PORTRAIT.LBX`.
pub const HERO_PORTRAITS: usize = 40;

pub fn parse_unit_art(src: &str) -> Result<Vec<UnitArt>, String> {
    ron::from_str(src).map_err(|e| format!("unit_art.ron: {e}"))
}

/// Read `unit_art.ron` from the first of `dirs` that has it and check the table by itself.
pub fn load_unit_art(dirs: &[&Path]) -> Result<Vec<UnitArt>, String> {
    let path = dirs
        .iter()
        .map(|d| d.join("unit_art.ron"))
        .find(|p| p.exists())
        .ok_or_else(|| "unit_art.ron: not found".to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let units = parse_unit_art(&text)?;
    let problems = validate_unit_art(&units);
    if problems.is_empty() {
        Ok(units)
    } else {
        Err(format!(
            "invalid unit_art.ron:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// Problems with the table by itself: a unit twice, a portrait that is not in `PORTRAIT.LBX`.
pub fn validate_unit_art(units: &[UnitArt]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for u in units {
        if !seen.insert(&u.id) {
            problems.push(format!("unit_art: \"{}\" twice", u.id));
        }
        if u.portrait.is_some_and(|p| p >= HERO_PORTRAITS) {
            problems.push(format!(
                "unit_art: \"{}\": portrait {:?} is not one of the {HERO_PORTRAITS} hero portraits",
                u.id, u.portrait
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{content_dir, TestResult};

    #[test]
    fn the_shipped_table_reads_and_a_bad_portrait_or_a_repeat_is_reported() -> TestResult {
        let units = load_unit_art(&[&content_dir()])?;
        assert!(units.len() > 190);
        let dwarf = units.iter().find(|u| u.id == "dwarf").ok_or("no dwarf")?;
        assert_eq!(dwarf.art, Some(("units1".to_string(), 0)));
        assert_eq!(dwarf.portrait, Some(10));
        let bad = parse_unit_art(
            r#"[(id: "a", art: None, portrait: Some(40)), (id: "a", art: None, portrait: None)]"#,
        )?;
        assert_eq!(validate_unit_art(&bad).len(), 2);
        Ok(())
    }
}
