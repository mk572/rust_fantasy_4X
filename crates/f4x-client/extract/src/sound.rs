//! Which of the original's sounds each spell and unit plays (`content/sound.ron`).
//!
//! The numbers are entries of the original's sound files, which the client plays from
//! `assets-local/audio` when they are there (`docs/sound.md`). The file is optional.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

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

pub fn parse_sound(src: &str) -> Result<SoundTable, String> {
    ron::from_str(src).map_err(|e| format!("sound.ron: {e}"))
}

/// Read `sound.ron` from the first of `dirs` that has it; content without the file is silent. The ids
/// are checked against the rules by the client (`asset_check`).
pub fn load_sound(dirs: &[&Path]) -> Result<SoundTable, String> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("sound.ron"))
        .find(|p| p.exists())
    else {
        return Ok(SoundTable::default());
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_sound(&text)
}

/// Problems the table has by itself: a sound number out of range. Whether the ids name real spells and
/// units is checked by the client (`asset_check`).
pub fn validate_sound(table: &SoundTable) -> Vec<String> {
    table
        .units
        .iter()
        .filter(|(_, s)| s.r#move > 13 || s.melee > 8)
        .map(|(id, s)| format!("sound: \"{id}\" has {s:?}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestResult;

    #[test]
    fn the_table_reads_and_a_sound_out_of_range_is_reported() -> TestResult {
        let table = parse_sound(
            r#"(spells: {"fireball": 93, "web": -20}, units: {"dwarf": (move: 6, melee: 6)}, global_music: {"time_stop": 25})"#,
        )?;
        assert_eq!(table.spells["web"], -20);
        assert_eq!(
            table.units["dwarf"],
            UnitSound {
                r#move: 6,
                melee: 6
            }
        );
        assert!(validate_sound(&table).is_empty());
        let bad = parse_sound(r#"(units: {"dwarf": (move: 20, melee: 6)})"#)?;
        assert_eq!(validate_sound(&bad).len(), 1);
        Ok(())
    }

    #[test]
    fn the_shipped_table_reads() -> TestResult {
        let table = load_sound(&[&crate::testutil::content_dir()])?;
        assert!(validate_sound(&table).is_empty());
        assert_eq!(table.spells["fireball"], 93);
        assert_eq!(table.global_music["time_stop"], 25);
        Ok(())
    }
}
