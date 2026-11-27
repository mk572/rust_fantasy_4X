//! The neutral player of a race (task 522): every independent city of a race, on both planes,
//! belongs to one player, as if one wizard without magic ruled them all. Every race has its
//! player from the start of the game to its end. While one has no city and no unit it is dormant
//! ([`World::is_dormant`]): it takes no turn and nobody deals with it, but it keeps what it has
//! (treasury, vault). Its treaties and wars end when it loses its last city
//! (`docs/diplomacy.md`, "Neutral players"). Its number is [`FIRST_RACE_PLAYER`] plus the race's number, so
//! it is the same in every game and save. The monsters' owner stays a player of its own.
//!
//! Tests: in tests/it.

use crate::player::{PlayerId, MONSTERS};
use crate::rules::RaceId;
use crate::world::World;

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

impl World {
    /// The race whose neutral player `id` is, if it is one (not a wizard, not the monsters).
    pub fn race_of_player(&self, id: PlayerId) -> Option<RaceId> {
        (id != MONSTERS && id.0 >= FIRST_RACE_PLAYER && self.is_neutral_player(id))
            .then(|| RaceId::new(u16::from(id.0 - FIRST_RACE_PLAYER)))
    }

    /// Whether `id` is the neutral player of a race that has no city and no unit.
    pub fn is_dormant(&self, id: PlayerId) -> bool {
        self.race_of_player(id).is_some()
            && self.economy().city_count(id) == 0
            && self.units().player_units(id).is_empty()
    }

    /// Make the neutral player of every race that has none: it starts with the gold that a
    /// computer wizard starts with (and its cities share the one treasury). A game begins with
    /// all of them.
    pub fn add_race_players(&mut self) {
        let races: Vec<RaceId> = self.content.rules.races.iter().map(|(id, _)| id).collect();
        let gold = self.settings.wizards.starting_gold.of(false);
        for race in races {
            let id = race_player_id(race);
            if self.has_player(id) {
                continue;
            }
            self.add_neutral_player(id);
            if let Some(t) = self.treasury_mut(id) {
                t.gold = f4x_def::half_int::HalfInt::from_int(gold);
            }
        }
    }

    /// The neutral player of `race`, for something that is to be its own.
    pub(crate) fn race_player(&self, race: RaceId) -> PlayerId {
        let id = race_player_id(race);
        debug_assert!(self.has_player(id), "the game has no player for {race:?}");
        id
    }
}
