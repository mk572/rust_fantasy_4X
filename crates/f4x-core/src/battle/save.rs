//! A battle written to a save and read back.
//!
//! The spell ids inside a battle are indexes into the rules' spell list, which change when a spell
//! is added or moved in `spells.ron`. A save names its spells instead: [`BattleSave`] carries the
//! table of spell ids as they were, and loading maps every number in the battle to the spell of
//! the same name in the rules in force (or refuses the save if the spell is gone).
//!
//! The battle holds the overland items its fighters carry (`ItemCharge`) by creation number only;
//! [`Battle::bind_items`] puts the entities back once the world is loaded.

use serde::{Deserialize, Serialize};

use super::game::ActiveBattle;
use super::{Battle, BattleUnit, Side};
use crate::game::save::LoadError;
use crate::game::serial::Serial;
use crate::game::Game;
use crate::rules::{Rules, SpellId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleSave {
    /// The id of every spell of the rules, in the order the numbers in `battle` refer to.
    pub spells: Vec<String>,
    pub battle: Battle,
}

/// Why a saved battle could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleLoadError {
    /// The save names a spell the rules do not have (spell id).
    UnknownSpell(String),
    /// The battle refers to a spell number the save's own table does not have.
    BadSpellNumber(usize),
}

impl Battle {
    /// The battle as a save holds it.
    pub fn to_save(&self, rules: &Rules) -> BattleSave {
        BattleSave {
            spells: rules.spells.defs().iter().map(|d| d.id.clone()).collect(),
            battle: self.clone(),
        }
    }

    /// The battle from a save, its spell numbers mapped to the rules in force. Items still need
    /// [`Battle::bind_items`].
    pub fn from_save(save: BattleSave, rules: &Rules) -> Result<Battle, BattleLoadError> {
        let BattleSave { spells, mut battle } = save;
        let mut map = |old: SpellId| -> Result<SpellId, BattleLoadError> {
            let name = spells
                .get(old.index())
                .ok_or(BattleLoadError::BadSpellNumber(old.index()))?;
            rules
                .spells
                .id_of(name)
                .ok_or_else(|| BattleLoadError::UnknownSpell(name.clone()))
        };
        for side in &mut battle.fields {
            for (_, spell) in side {
                *spell = map(*spell)?;
            }
        }
        for unit in &mut battle.units {
            map_unit_spells(unit, &mut map)?;
        }
        Ok(battle)
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

/// Map every spell number a unit holds, and the one it was before its lasting spells.
fn map_unit_spells(
    unit: &mut BattleUnit,
    map: &mut impl FnMut(SpellId) -> Result<SpellId, BattleLoadError>,
) -> Result<(), BattleLoadError> {
    let casting = &mut unit.casting;
    for spell in casting.innate.iter_mut().chain(&mut casting.abilities) {
        *spell = map(*spell)?;
    }
    for c in &mut casting.item_charges {
        c.spell = map(c.spell)?;
    }
    for (spell, _) in &mut unit.combat_spells {
        *spell = map(*spell)?;
    }
    if let Some(base) = unit.base.as_mut() {
        map_unit_spells(base, map)?;
    }
    Ok(())
}

/// A battle in progress as a save holds it, next to the world it is fought in: the battle itself,
/// and the world's entities it refers to by creation number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveBattleSave {
    pub battle: BattleSave,
    /// `BattleUnit::source` indexes this list.
    pub sources: Vec<Serial>,
    pub left_out: Vec<(Side, Serial)>,
    pub lair: Option<Serial>,
}

impl Game {
    /// The battle in progress, for a save. `None` outside a battle.
    pub(crate) fn battle_save(&self) -> Option<ActiveBattleSave> {
        let a = self.battle.as_ref()?;
        Some(ActiveBattleSave {
            battle: a.battle.to_save(&self.content.rules),
            sources: a.sources.iter().map(|&e| self.serial_of(e)).collect(),
            left_out: a
                .left_out
                .iter()
                .map(|&(side, e)| (side, self.serial_of(e)))
                .collect(),
            lair: a.lair.map(|e| self.serial_of(e)),
        })
    }

    /// Put a saved battle back, once the world is loaded. The battle must agree with the world:
    /// its players, the units and items it was copied from, and the side each unit fights for.
    pub(crate) fn restore_battle(&mut self, save: ActiveBattleSave) -> Result<(), LoadError> {
        let bad = |why: &str| LoadError::Inconsistent(format!("the saved battle {why}"));
        let mut battle =
            Battle::from_save(save.battle, &self.content.rules).map_err(|e| match e {
                BattleLoadError::UnknownSpell(s) => LoadError::UnknownId("spell", s),
                BattleLoadError::BadSpellNumber(_) => bad("names a spell the save does not list"),
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
            lair,
        });
        Ok(())
    }
}
