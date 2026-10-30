//! A battle written to a save and read back.
//!
//! The spell ids inside a battle are the spells' numbers, which stay the same from one version of
//! `spells.ron` to the next; loading only refuses a battle that holds a number the rules lack.
//!
//! The battle holds the overland items its fighters carry (`ItemCharge`) by creation number only;
//! [`Battle::bind_items`] puts the entities back once the world is loaded.
//!
//! Tests: in battle/tests.rs.

use serde::{Deserialize, Serialize};

use super::game::ActiveBattle;
use super::{Battle, BattleUnit, Side};
use crate::game::save::LoadError;
use crate::game::serial::Serial;
use crate::game::Game;
use crate::rules::{Rules, SpellId};

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
        entity_of: impl Fn(crate::game::serial::Serial) -> Option<hecs::Entity>,
    ) -> Result<(), crate::game::serial::Serial> {
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
    for &(spell, _) in &unit.combat_spells {
        check(spell)?;
    }
    if let Some(base) = unit.base.as_ref() {
        check_unit_spells(base, check)?;
    }
    Ok(())
}

/// A battle in progress as a save holds it, next to the world it is fought in: the battle itself,
/// and the world's entities it refers to by creation number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBattleSave {
    pub battle: Battle,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<Serial>,
    pub left_out: Vec<(Side, Serial)>,
    /// Defenders beyond the nine that fight.
    #[serde(default)]
    pub benched: Vec<Serial>,
    pub lair: Option<Serial>,
}

impl Game {
    /// The battle in progress, for a save. `None` outside a battle.
    pub(crate) fn battle_save(&self) -> Option<ActiveBattleSave> {
        let a = self.battle.as_ref()?;
        Some(ActiveBattleSave {
            battle: a.battle.clone(),
            sources: a.sources.iter().map(|&e| self.serial_of(e)).collect(),
            left_out: a
                .left_out
                .iter()
                .map(|&(side, e)| (side, self.serial_of(e)))
                .collect(),
            benched: a.benched.iter().map(|&e| self.serial_of(e)).collect(),
            lair: a.lair.map(|e| self.serial_of(e)),
        })
    }

    /// Put a saved battle back, once the world is loaded. The battle must agree with the world:
    /// its players, the units and items it was copied from, and the side each unit fights for.
    pub(crate) fn restore_battle(&mut self, save: ActiveBattleSave) -> Result<(), LoadError> {
        let bad = |why: &str| LoadError::Inconsistent(format!("the saved battle {why}"));
        let mut battle = save.battle;
        battle
            .check_spells(&self.content.rules)
            .map_err(|BattleLoadError::UnknownSpell(n)| {
                LoadError::UnknownId("spell", n.to_string())
            })?;
        for (side, player) in [
            (Side::Attacker, battle.attacker),
            (Side::Defender, battle.defender),
        ] {
            if self.player_entity(player).is_none() {
                return Err(bad(&format!("has a missing {side:?} player {}", player.0)));
            }
        }
        if battle.attacker == battle.defender {
            return Err(bad("has one player on both sides"));
        }
        let unit_of = |game: &Game, serial: Serial| {
            game.units()
                .entity_of_serial(serial)
                .filter(|&e| game.units().unit_def(e).is_some())
        };
        let sources = save
            .sources
            .iter()
            .map(|&s| unit_of(self, s).ok_or_else(|| bad("fights with a unit that is gone")))
            .collect::<Result<Vec<_>, _>>()?;
        let left_out = save
            .left_out
            .iter()
            .map(|&(side, s)| {
                unit_of(self, s)
                    .map(|e| (side, e))
                    .ok_or_else(|| bad("left a unit out that is gone"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let benched = save
            .benched
            .iter()
            .map(|&s| unit_of(self, s).ok_or_else(|| bad("benched a unit that is gone")))
            .collect::<Result<Vec<_>, _>>()?;
        let lair = save
            .lair
            .map(|s| {
                self.units()
                    .entity_of_serial(s)
                    .filter(|&e| self.economy().lair_of(e).is_some())
                    .ok_or_else(|| bad("assaults a lair that is gone"))
            })
            .transpose()?;
        for (i, u) in battle.units.iter().enumerate() {
            if u.id != i {
                return Err(bad("numbers its units out of order"));
            }
            let Some(source) = u.source else { continue };
            let entity = *sources
                .get(source)
                .ok_or_else(|| bad("copies a unit from a place that is not listed"))?;
            // A unit seized or bound by the other side still belongs to the side it came from.
            let home = u.bound_from.or(u.turned_from).unwrap_or(u.side);
            if self.units().unit_owner(entity) != Some(battle.player_of(home)) {
                return Err(bad("has a unit that belongs to the wrong player"));
            }
        }
        battle
            .bind_items(|serial| {
                self.units()
                    .entity_of_serial(serial)
                    .filter(|&e| self.items().item(e).is_some())
            })
            .map_err(|_| bad("uses an item that is gone"))?;
        self.battle = Some(ActiveBattle {
            battle,
            sources,
            left_out,
            benched,
            lair,
        });
        Ok(())
    }
}
