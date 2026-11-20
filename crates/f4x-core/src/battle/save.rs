//! A battle written to a save and read back.
//!
//! The checks that need no world (the spells, the items) are in `load.rs`; this file ties a saved
//! battle to the world it was fought in.
//!
//! Tests: in battle/tests.rs.

use serde::{Deserialize, Serialize};

use super::game::ActiveBattle;
pub use super::load::BattleLoadError;
use super::{Battle, Side};
use crate::game::save::LoadError;
use crate::game::serial::Serial;
use crate::game::Game;

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
            sources: a
                .sources
                .iter()
                .map(|&e| self.units().serial_of(e))
                .collect(),
            left_out: a
                .left_out
                .iter()
                .map(|&(side, e)| (side, self.units().serial_of(e)))
                .collect(),
            benched: a
                .benched
                .iter()
                .map(|&e| self.units().serial_of(e))
                .collect(),
            lair: a.lair.map(|e| self.units().serial_of(e)),
        })
    }

    /// Put a saved battle back, once the world is loaded. The battle must agree with the world:
    /// its players, the units and items it was copied from, and the side each unit fights for.
    pub(crate) fn restore_battle(&mut self, save: ActiveBattleSave) -> Result<(), LoadError> {
        let bad = |why: &str| LoadError::Inconsistent(format!("the saved battle {why}"));
        let mut battle = save.battle;
        battle.content = super::content_ref::ContentRef::new(self.content.clone());
        battle.bind_units();
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
