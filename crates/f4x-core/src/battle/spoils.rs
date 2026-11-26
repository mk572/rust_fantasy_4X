//! Giving out what a battle won: the experience and the fame worked out by
//! [`f4x_battle::battle::spoils`] (the rules of them are described there).
//!
//! Tests: in battle/tests.rs (`the_winners_survivors_gain_experience_and_fame_for_four_kills` and the tests after it).

use super::Battle;
use crate::world::{Event, World};
pub use f4x_battle::battle::spoils::Spoils;

impl World {
    /// Give out what [`Battle::spoils`] worked out. `survivors` are the winner's units
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
