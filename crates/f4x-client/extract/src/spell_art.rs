//! Which pictures of the original's art belong to which spell (`content/spell_art.ron`).
//!
//! The table names each picture by its source `(lbx, entry)` and what it is used for. The client
//! reads the pictures to draw from it and `f4x-extract art` files them by it: a picture of one spell
//! goes to `assets-local/art/spells/<spell id>/`, one used by several spells (or by none, like the
//! realm pictures and the summon backgrounds) to `spells/_shared/`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

/// What a picture is used for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub enum Role {
    /// The spell's own picture, playing where it lands (in a battle, on the map or on a wizard's
    /// portrait) or fixed in a box.
    Cast,
    /// The figure that stands on a square of the battlefield after the spell (Magic Vortex).
    Vortex,
    /// The picture of a global enchantment in the diplomacy mirror.
    Global,
    /// What Call Chaos does to a unit: outcome 1 to 7 of its eight (0 does nothing).
    Chaos(u8),
    /// The picture a spell without one of its own plays: the realm's.
    Realm(String),
    /// The flame frame over the summon scene.
    Summon,
    /// The scroll of a city spell.
    CityScroll,
    /// The sparkle the learn-spell animation decodes the new spell's entry under.
    Sparkle,
    /// The "You have mastered the spell of X" box.
    Learned,
    /// The box about a spell that fizzled.
    Fizzle,
    /// The box about a city's spell that was dispelled.
    DispelledCity,
    /// The box about a unit's spell that was dispelled.
    DispelledUnit,
    /// One of the four vortex animations of the Spell of Mastery announcement, in the order they play.
    Mastery(u8),
    /// The mask the mirror dissolves the wizard's face through, first or second.
    Mask(u8),
}

/// One picture: its source file and entry, and what it is for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Picture {
    pub lbx: String,
    pub entry: usize,
    pub role: Role,
}

/// The folder for pictures of no single spell.
pub const SHARED: &str = "_shared";
/// The folder for the spell book's pictures.
pub const BOOK: &str = "_book";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SpellArt {
    /// Pictures that belong to no one spell.
    #[serde(default)]
    pub shared: Vec<Picture>,
    /// The pictures of each spell, by its id.
    #[serde(default)]
    pub spells: BTreeMap<String, Vec<Picture>>,
}

impl SpellArt {
    /// The `(lbx, entry)` of a spell's picture for `role`.
    pub fn picture(&self, spell: &str, role: &Role) -> Option<(&str, usize)> {
        self.spells
            .get(spell)?
            .iter()
            .find(|p| p.role == *role)
            .map(|p| (p.lbx.as_str(), p.entry))
    }

    /// The `(lbx, entry)` of a shared picture for `role`.
    pub fn shared(&self, role: &Role) -> Option<(&str, usize)> {
        self.shared
            .iter()
            .find(|p| p.role == *role)
            .map(|p| (p.lbx.as_str(), p.entry))
    }

    /// The picture a realm's spells play when they have none of their own.
    pub fn realm(&self, realm: &str) -> Option<(&str, usize)> {
        self.shared(&Role::Realm(realm.to_string()))
    }

    /// The folder under `art/spells/` each picture goes to: the spell's id when only that spell
    /// uses it, [`SHARED`] when several do or it is listed as shared. Call Chaos's outcomes
    /// (`Role::Chaos`) borrow the pictures of other spells (Warp Lightning, Disintegrate, Fire
    /// Storm): they stay in the folder of the spell they belong to, and Call Chaos reads them
    /// from there.
    pub fn folders(&self) -> BTreeMap<(String, usize), String> {
        let mut users: BTreeMap<(String, usize), BTreeSet<&str>> = BTreeMap::new();
        for (id, pictures) in &self.spells {
            for p in pictures {
                let spells = users.entry((p.lbx.clone(), p.entry)).or_default();
                if !matches!(p.role, Role::Chaos(_)) {
                    spells.insert(id.as_str());
                }
            }
        }
        for p in &self.shared {
            users.entry((p.lbx.clone(), p.entry)).or_default();
        }
        let shared: BTreeSet<_> = self.shared.iter().map(|p| (&p.lbx, p.entry)).collect();
        users
            .into_iter()
            .map(|(key, spells)| {
                let own = match (spells.len(), shared.contains(&(&key.0, key.1))) {
                    (1, false) => spells.into_iter().next().map(str::to_string),
                    _ => None,
                };
                (key, own.unwrap_or_else(|| SHARED.to_string()))
            })
            .collect()
    }
}

pub fn parse_spell_art(src: &str) -> Result<SpellArt, String> {
    ron::from_str(src).map_err(|e| format!("spell_art.ron: {e}"))
}

/// Read `spell_art.ron` from the first of `dirs` that has it and check the table by itself; the client
/// checks its ids against the rules (`asset_check`).
pub fn load_spell_art(dirs: &[&Path]) -> Result<SpellArt, String> {
    let path = dirs
        .iter()
        .map(|d| d.join("spell_art.ron"))
        .find(|p| p.exists())
        .ok_or_else(|| "spell_art.ron: not found".to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let art = parse_spell_art(&text)?;
    let problems = validate_spell_art(&art);
    if problems.is_empty() {
        Ok(art)
    } else {
        Err(format!(
            "invalid spell_art.ron:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// Problems with the table: a spell with the same role twice, a shared role twice, a Call Chaos
/// outcome out of range, a spell with no picture.
pub fn validate_spell_art(art: &SpellArt) -> Vec<String> {
    let mut problems = Vec::new();
    for (id, pictures) in &art.spells {
        let mut seen = BTreeSet::new();
        for p in pictures {
            if !seen.insert(format!("{:?}", p.role)) {
                problems.push(format!("spell_art: \"{id}\" has {:?} twice", p.role));
            }
            if matches!(p.role, Role::Chaos(n) if !(1..=7).contains(&n)) {
                problems.push(format!("spell_art: \"{id}\" has {:?}", p.role));
            }
        }
        if pictures.is_empty() {
            problems.push(format!("spell_art: \"{id}\" has no picture"));
        }
    }
    let mut seen = BTreeSet::new();
    for p in &art.shared {
        if !seen.insert(format!("{:?}", p.role)) {
            problems.push(format!("spell_art: shared {:?} twice", p.role));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestResult;

    fn picture(lbx: &str, entry: usize, role: Role) -> Picture {
        Picture {
            lbx: lbx.into(),
            entry,
            role,
        }
    }

    #[test]
    fn a_picture_of_one_spell_goes_to_its_folder_and_one_of_several_is_shared() -> TestResult {
        let art = parse_spell_art(
            r#"(
                shared: [(lbx: "specfx", entry: 49, role: Sparkle)],
                spells: {
                    "fire_bolt": [(lbx: "cmbtfx", entry: 0, role: Cast)],
                    "petrify": [(lbx: "specfx", entry: 12, role: Cast)],
                    "call_the_void": [(lbx: "specfx", entry: 12, role: Cast)],
                },
            )"#,
        )?;
        let folders = art.folders();
        assert_eq!(folders[&("cmbtfx".into(), 0)], "fire_bolt");
        assert_eq!(folders[&("specfx".into(), 12)], SHARED);
        assert_eq!(folders[&("specfx".into(), 49)], SHARED);
        assert_eq!(art.picture("petrify", &Role::Cast), Some(("specfx", 12)));
        assert_eq!(art.shared(&Role::Sparkle), Some(("specfx", 49)));
        Ok(())
    }

    #[test]
    fn call_chaos_reads_the_pictures_it_borrows_from_the_folders_of_their_spells() -> TestResult {
        let art = parse_spell_art(
            r#"(
                spells: {
                    "warp_lightning": [(lbx: "cmbtfx", entry: 3, role: Cast)],
                    "call_chaos": [
                        (lbx: "cmbtfx", entry: 3, role: Chaos(5)),
                        (lbx: "cmbtfx", entry: 3, role: Chaos(6)),
                    ],
                },
            )"#,
        )?;
        assert_eq!(art.folders()[&("cmbtfx".into(), 3)], "warp_lightning");
        assert_eq!(
            art.picture("call_chaos", &Role::Chaos(6)),
            Some(("cmbtfx", 3))
        );
        Ok(())
    }

    #[test]
    fn a_picture_listed_as_shared_stays_shared_when_one_spell_uses_it() -> TestResult {
        let art = parse_spell_art(
            r#"(
                shared: [(lbx: "specfx", entry: 3, role: Realm("life"))],
                spells: {"chaos_channels": [(lbx: "specfx", entry: 3, role: Chaos(1))]},
            )"#,
        )?;
        assert_eq!(art.folders()[&("specfx".into(), 3)], SHARED);
        assert_eq!(art.realm("life"), Some(("specfx", 3)));
        Ok(())
    }

    #[test]
    fn the_committed_table_reads_and_has_a_picture_for_each_realm() -> TestResult {
        let art = load_spell_art(&[&crate::testutil::content_dir()])?;
        for realm in ["nature", "sorcery", "chaos", "life", "death", "arcane"] {
            assert!(art.realm(realm).is_some(), "no picture for {realm}");
        }
        // Every spell with art has its own folder or shares all its pictures.
        let folders = art.folders();
        for (id, pictures) in &art.spells {
            for p in pictures {
                let folder = &folders[&(p.lbx.clone(), p.entry)];
                // Call Chaos's outcomes live in the folders of the spells they borrow from.
                let borrowed = matches!(p.role, Role::Chaos(_));
                assert!(
                    folder == id || folder == SHARED || borrowed,
                    "{id}: {folder}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_table_with_a_repeated_role_is_refused() -> TestResult {
        let art = SpellArt {
            shared: vec![],
            spells: BTreeMap::from([(
                "fire_bolt".to_string(),
                vec![
                    picture("cmbtfx", 0, Role::Cast),
                    picture("cmbtfx", 1, Role::Cast),
                ],
            )]),
        };
        let problems = validate_spell_art(&art);
        assert_eq!(problems.len(), 1, "{problems:?}");
        Ok(())
    }
}
