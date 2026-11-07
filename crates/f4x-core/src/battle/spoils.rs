//! What a battle gives and takes once it is over (the original's end-of-combat code):
//!
//! - **Experience.** The winner's units still standing (normal units and heroes, not creatures
//!   or the undead) each gain 2 xp for every enemy unit that died, summoned ones too. Units
//!   that fled give nothing, as in the original, which counts the dead.
//! - **Fame** (the Master of Magic wiki's article on Fame). The winner gains 1 fame for killing
//!   four or more enemy units (8 xp; summoned ones count) and 1 for a very rare foe (a dead
//!   fantastic unit that costs 600 or more). The loser loses the same two points the same way
//!   (four or more of its units dead, a very rare unit of its own dead) and, for every hero it
//!   lost, half a level rounded down with the lowest level counted as 1 (`(level + 1) / 2` of our
//!   level from 0): a Myrmidon or Captain costs 1, a Commander or Champion 2.
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
/// A fantastic unit that costs this much (its mana budget) is a very rare foe.
pub const RARE_FOE_COST: u32 = 600;
/// Units dead on one side that cost it a point of fame, and earn the other side one.
pub const FAME_KILLS: usize = 4;

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
    /// A very rare unit: a fantastic one (summoned or built, not a hero or a normal unit) whose
    /// cost, its mana budget, reaches [`RARE_FOE_COST`].
    fn very_rare(&self, u: &BattleUnit) -> bool {
        u.unit_id.is_some_and(|id| {
            let d = self.content.rules.units.get(id);
            matches!(d.kind, UnitKind::Fantastic | UnitKind::Summoned) && d.cost >= RARE_FOE_COST
        })
    }

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
        // The loser's dead units decide both sides' fame: the winner gains, the loser loses, a
        // point for four or more of them and a point for a very rare one among them.
        let fallen = |u: &&BattleUnit| u.side != winner && u.status == Status::Dead;
        let killed = battle.units.iter().filter(fallen).count();
        out.experience = killed as u32 * XP_PER_KILL;
        let point = i32::from(killed >= FAME_KILLS)
            + i32::from(
                battle
                    .units
                    .iter()
                    .filter(fallen)
                    .any(|u| self.very_rare(u)),
            );
        out.winner_fame += point;
        out.loser_fame -= point;
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
        let loser_fame = spoils.loser_fame;
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
