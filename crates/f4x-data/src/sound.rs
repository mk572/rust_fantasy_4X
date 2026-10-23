//! Which of the original's sounds each spell and unit plays (`content/sound.ron`).
//!
//! The numbers are entries of the original's sound files, which the client plays from
//! `assets-local/audio` when they are there (`docs/sound.md`). The file is optional.

use std::collections::BTreeMap;
use std::path::Path;

use f4x_core::rules::Rules;
use serde::Deserialize;

use crate::ContentError;

/// A unit's two sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct UnitSound {
    /// A `SOUNDFX` entry played for each square it walks in a battle; 0 is silent and 13 is
    /// `NEWSOUND` entry 0.
    pub r#move: u8,
    /// A `CMBTSND` entry played when it strikes in melee; 8 is silent.
    pub melee: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SoundTable {
    /// The cast sound of a spell, by its id: positive is a `SOUNDFX` entry, negative a `NEWSOUND`
    /// entry.
    #[serde(default)]
    pub spells: BTreeMap<String, i16>,
    #[serde(default)]
    pub units: BTreeMap<String, UnitSound>,
    /// The `MUSIC` entry of a global enchantment's scene, by the spell's id.
    #[serde(default)]
    pub global_music: BTreeMap<String, u8>,
}

pub fn parse_sound(src: &str) -> Result<SoundTable, ContentError> {
    ron::from_str(src).map_err(|e| ContentError::Parse(format!("sound.ron: {e}")))
}

/// Read `sound.ron` from the first of `dirs` that has it and check its ids against the rules;
/// content without the file is silent.
pub fn load_sound(dirs: &[&Path], rules: &Rules) -> Result<SoundTable, ContentError> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("sound.ron"))
        .find(|p| p.exists())
    else {
        return Ok(SoundTable::default());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ContentError::Io(format!("{}: {e}", path.display())))?;
    let table = parse_sound(&text)?;
    let problems = validate_sound(&table, rules);
    if problems.is_empty() {
        Ok(table)
    } else {
        Err(ContentError::Invalid(problems))
    }
}

/// Problems with the table: a spell or unit that does not exist, a sound number out of range.
pub fn validate_sound(table: &SoundTable, rules: &Rules) -> Vec<String> {
    let mut problems = Vec::new();
    for id in table.spells.keys().chain(table.global_music.keys()) {
        if rules.spells.id_of(id).is_none() {
            problems.push(format!("sound: unknown spell \"{id}\""));
        }
    }
    for (id, sound) in &table.units {
        if rules.units.id_of(id).is_none() {
            problems.push(format!("sound: unknown unit \"{id}\""));
        }
        if sound.r#move > 13 || sound.melee > 8 {
            problems.push(format!("sound: \"{id}\" has {sound:?}"));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_reads_and_a_bad_id_is_reported() {
        let table = parse_sound(
            r#"(spells: {"fireball": 93, "web": -20}, units: {"dwarf": (move: 6, melee: 6)}, global_music: {"time_stop": 25})"#,
        )
        .unwrap();
        assert_eq!(table.spells["web"], -20);
        assert_eq!(
            table.units["dwarf"],
            UnitSound {
                r#move: 6,
                melee: 6
            }
        );
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::load_content_layers(&[&dir]).unwrap();
        assert!(validate_sound(&table, &content.rules).is_empty());
        let bad = parse_sound(
            r#"(spells: {"no_such_spell": 1}, units: {"dwarf": (move: 20, melee: 6)})"#,
        )
        .unwrap();
        assert_eq!(validate_sound(&bad, &content.rules).len(), 2);
    }

    #[test]
    fn the_shipped_table_matches_the_rules() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = crate::load_content_layers(&[&dir]).unwrap();
        let table = load_sound(&[&dir], &content.rules).unwrap();
        assert_eq!(table.units.len(), content.rules.units.len());
        assert_eq!(table.spells["fireball"], 93);
        assert_eq!(table.global_music["time_stop"], 25);
    }
}
