//! What a battle gives and takes once it is over (the original's end-of-combat code):
//!
//! - **Experience.** The winner's units still standing (normal units and heroes, not creatures
//!   or the undead) each gain 2 xp for every enemy unit that died, summoned ones too. Units
//!   that fled give nothing, as in the original, which counts the dead.
//! - **Fame.** The winner gains 1 fame for killing four or more enemy units (8 xp), and 1 for a
//!   very rare foe (a dead enemy unit costing 600 or more). The loser loses half a hero's level
//!   rounded up (`(level + 1) / 2`) for every hero it lost, and a human loser with more
//!   than 20 fame also loses a point when the winner got its point for four kills (the
//!   original takes it back from the human attacker or defender who lost).
//!
//! A neutral winner (monsters) gets nothing.
//!
//! Tests: in battle/tests.rs (`the_winners_survivors_gain_experience_and_fame_for_four_kills` and the tests after it).

use super::{Battle, BattleUnit, Status};
use crate::game::{Event, Game};
use crate::rules::UnitKind;

/// Experience per dead enemy unit.
pub const XP_PER_KILL: u32 = 2;
/// Experience that earns the winner a point of fame (four kills).
pub const FAME_XP: u32 = 8;
/// An enemy unit that costs this much is a rare foe.
pub const RARE_FOE_COST: u32 = 600;
/// A human loser with more fame than this loses a point when the winner earned one for four kills.
const HUMAN_FAME_LIMIT: i32 = 20;

/// The result of a battle, worked out before the units are written back.
#[derive(Debug, Default)]
pub struct Spoils {
    /// Experience each surviving unit of the winner gains.
    pub experience: u32,
    /// The winner's fame change.
    pub winner_fame: i32,
    /// The loser's fame change (not above 0).
    pub loser_fame: i32,
}

impl Game {
    /// Work out the experience and fame of a finished battle. Reads the units, so call it
    /// before the dead are removed.
    pub(super) fn battle_spoils(&self, battle: &Battle, sources: &[hecs::Entity]) -> Spoils {
        let mut out = Spoils::default();
        let Some(winner) = battle.winner else {
            return out;
        };
        if self.is_neutral_player(battle.player_of(winner)) {
            return out;
        }
        let fallen = |u: &&BattleUnit| u.side != winner && u.status == Status::Dead;
        out.experience = battle.units.iter().filter(fallen).count() as u32 * XP_PER_KILL;
        if out.experience >= FAME_XP {
            out.winner_fame += 1;
        }
        let rare = battle.units.iter().filter(fallen).any(|u| {
            let id = self.content.rules.units.id_of(&u.unit_id);
            id.is_some_and(|id| self.content.rules.units.get(id).cost >= RARE_FOE_COST)
        });
        if rare {
            out.winner_fame += 1;
        }
        for u in battle.units.iter().filter(fallen) {
            let Some(entity) = u.source.and_then(|i| sources.get(i).copied()) else {
                continue;
            };
            if self
                .units()
                .unit_def(entity)
                .is_some_and(|d| d.kind == UnitKind::Hero)
            {
                out.loser_fame -= (self.units().unit_xp_level(entity) as i32 + 1) / 2;
            }
        }
        out
    }

    /// Give out what [`Game::battle_spoils`] worked out. `survivors` are the winner's units
    /// still on their feet.
    pub(super) fn give_spoils(
        &mut self,
        battle: &Battle,
        spoils: &Spoils,
        survivors: &[hecs::Entity],
    ) -> Vec<Event> {
        let Some(winner) = battle.winner else {
            return Vec::new();
        };
        let (won, lost) = (battle.player_of(winner), battle.player_of(winner.other()));
        let mut loser_fame = spoils.loser_fame;
        if spoils.experience >= FAME_XP && self.human() == Some(lost) {
            let fame = self.fame(lost);
            if fame + loser_fame > HUMAN_FAME_LIMIT {
                loser_fame -= 1;
            }
        }
        if spoils.experience == 0 && spoils.winner_fame == 0 && loser_fame == 0 {
            return Vec::new();
        }
        for &unit in survivors {
            self.gain_xp(unit, spoils.experience);
        }
        self.add_fame(won, spoils.winner_fame);
        self.add_fame(lost, loser_fame);
        vec![Event::BattleSpoils {
            player: won,
            experience: spoils.experience,
            fame: spoils.winner_fame,
            loser_fame,
        }]
    }
}
