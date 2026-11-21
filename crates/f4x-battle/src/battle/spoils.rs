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
//! A neutral winner (monsters) gets nothing. The world gives the spoils out (`f4x-core`'s
//! `battle/spoils.rs`).
//!
//! Tests: in f4x-core's battle/tests.rs (`the_winners_survivors_gain_experience_and_fame_for_four_kills`
//! and the tests after it), fought on real battles.

use f4x_def::id::MONSTERS;
use f4x_def::rules::{Rules, UnitKind};

use super::Battle;
use crate::unit::{BattleUnit, Status};

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

/// A very rare unit: a fantastic one (summoned or built, not a hero or a normal unit) whose
/// cost, its mana budget, reaches `renown.rare_foe_cost`.
fn very_rare(rules: &Rules, u: &BattleUnit) -> bool {
    u.unit_id.is_some_and(|id| {
        let d = rules.units.get(id);
        matches!(d.kind, UnitKind::Fantastic | UnitKind::Summoned)
            && d.cost >= rules.config.renown.rare_foe_cost
    })
}

impl Battle {
    /// Work out the experience and fame of a finished battle from the units as they stand, the
    /// dead included.
    pub fn spoils(&self, rules: &Rules) -> Spoils {
        let mut out = Spoils::default();
        let Some(winner) = self.winner else {
            return out;
        };
        if self.player_of(winner) == MONSTERS {
            return out;
        }
        let renown = &rules.config.renown;
        // The loser's dead units decide both sides' fame: the winner gains, the loser loses, a
        // point for four or more of them and a point for a very rare one among them.
        let fallen = |u: &&BattleUnit| u.side != winner && u.status == Status::Dead;
        let killed = self.units.iter().filter(fallen).count();
        out.experience = killed as u32 * renown.xp_per_kill;
        let point = i32::from(killed >= renown.fame_kills)
            + i32::from(
                self.units
                    .iter()
                    .filter(fallen)
                    .any(|u| very_rare(rules, u)),
            );
        out.winner_fame += point;
        out.loser_fame -= point;
        // A fallen hero is one copied from a unit of the world (`source`).
        for u in self
            .units
            .iter()
            .filter(fallen)
            .filter(|u| u.source.is_some())
        {
            let Some(def) = u.unit_id.map(|id| rules.units.get(id)) else {
                continue;
            };
            if def.kind == UnitKind::Hero {
                out.loser_fame -= (rules.xp_level(def, u.sheet().xp) as i32 + 1) / 2;
            }
        }
        out
    }
}
