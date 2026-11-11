//! The neutral player of a race (task 522): every independent city of a race, on both planes,
//! belongs to one player, as if one wizard without magic ruled them all. It is made when the
//! first city of the race turns neutral and removed when it has no city and no unit left. Its
//! number is [`FIRST_RACE_PLAYER`] plus the race's number, so it is the same in every game and
//! save. The monsters' owner stays a player of its own.
//!
//! Tests: in player/race_player/tests.rs.

use crate::game::Game;
use crate::player::{PlayerId, MONSTERS};
use crate::rules::RaceId;

/// The number of the neutral player of the race with number 0; the others follow. Wizards have
/// numbers below this.
pub const FIRST_RACE_PLAYER: u8 = 100;

/// The player number of the neutral player of `race`.
pub fn race_player_id(race: RaceId) -> PlayerId {
    PlayerId(
        FIRST_RACE_PLAYER
            .saturating_add(race.num() as u8)
            .min(MONSTERS.0 - 1),
    )
}

impl Game {
    /// The race whose neutral player `id` is, if it is one (not a wizard, not the monsters).
    pub fn race_of_player(&self, id: PlayerId) -> Option<RaceId> {
        (id != MONSTERS && id.0 >= FIRST_RACE_PLAYER && self.is_neutral_player(id))
            .then(|| RaceId::new(u16::from(id.0 - FIRST_RACE_PLAYER)))
    }

    /// The neutral player of `race`, made now if the race has none: it starts with the gold
    /// that a computer wizard starts with (and its cities share the one treasury).
    pub(crate) fn race_player(&mut self, race: RaceId) -> PlayerId {
        let id = race_player_id(race);
        if !self.has_player(id) {
            self.add_neutral_player(id);
            let gold = self.settings.wizards.starting_gold.of(false);
            if let Some(t) = self.treasury_mut(id) {
                t.gold = f4x_def::half_int::HalfInt::from_int(gold);
            }
        }
        id
    }

    /// The neutral players that have no city and no unit are gone, with their treasury and the
    /// items in their vault. Called at the end of every round.
    pub(crate) fn retire_empty_race_players(&mut self) {
        let empty: Vec<PlayerId> = self
            .player_ids()
            .into_iter()
            .filter(|&id| self.race_of_player(id).is_some())
            .filter(|&id| {
                self.economy().city_count(id) == 0 && self.units().player_units(id).is_empty()
            })
            .collect();
        for id in empty {
            let vault = self
                .items()
                .held(|h| matches!(h, crate::item::entity::HeldBy::Vault(p, _) if p == id));
            for (item, _) in vault {
                self.despawn_item(item);
            }
            self.remove_player(id);
        }
    }
}

#[cfg(test)]
mod tests;
