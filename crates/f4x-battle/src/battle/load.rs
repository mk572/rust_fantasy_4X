//! Checks on a battle read back from a save: the spells it holds must be in the rules in force, and its
//! item charges get their entities back. Nothing here needs the world.
//!
//! The spell ids inside a battle are the spells' numbers, which stay the same from one version of
//! `spells.ron` to the next; loading only refuses a battle that holds a number the rules lack.
//!
//! The battle holds the overland items its fighters carry (`ItemCharge`) by creation number only;
//! [`Battle::bind_items`] puts the entities back once the world is loaded.
//!
//! Tests: in battle/tests.rs.

use f4x_def::id::Serial;
use f4x_def::rules::{Rules, SpellId};

use super::Battle;
use crate::unit::BattleUnit;

/// Why a saved battle could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleLoadError {
    /// The battle holds a spell number the rules do not have.
    UnknownSpell(u16),
}

impl Battle {
    /// Check that every spell the battle holds is in the rules in force. Items still need
    /// [`Battle::bind_items`].
    pub fn check_spells(&self, rules: &Rules) -> Result<(), BattleLoadError> {
        let mut check = |spell: SpellId| -> Result<SpellId, BattleLoadError> {
            rules
                .spells
                .id_numbered(spell.num())
                .ok_or(BattleLoadError::UnknownSpell(spell.num()))
        };
        for side in &self.fields {
            for &(_, spell) in side {
                check(spell)?;
            }
        }
        for unit in &self.units {
            check_unit_spells(unit, &mut check)?;
        }
        Ok(())
    }

    /// Give each item charge its entity back, by creation number. An item that is gone is an
    /// `Err` with its number.
    pub fn bind_items(
        &mut self,
        entity_of: impl Fn(Serial) -> Option<hecs::Entity>,
    ) -> Result<(), Serial> {
        for unit in &mut self.units {
            for c in &mut unit.casting.item_charges {
                c.item = entity_of(c.serial).ok_or(c.serial)?;
            }
        }
        Ok(())
    }
}

/// Check every spell number a unit holds, and the one it was before its lasting spells.
fn check_unit_spells(
    unit: &BattleUnit,
    check: &mut impl FnMut(SpellId) -> Result<SpellId, BattleLoadError>,
) -> Result<(), BattleLoadError> {
    let casting = &unit.casting;
    for &spell in casting.innate.iter().chain(&casting.abilities) {
        check(spell)?;
    }
    for c in &casting.item_charges {
        check(c.spell)?;
    }
    for l in unit.lasting() {
        check(l.spell)?;
    }
    for &spell in &unit.sheet().enchantments {
        check(spell)?;
    }
    Ok(())
}
