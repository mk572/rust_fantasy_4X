//! Creation numbers: a stable, never-reused order for entities.
//!
//! `hecs` entity ids can be reused after an entity is despawned, so sorting by id does not
//! give creation order and changes if a game is saved and reloaded. Every unit and city
//! carries a `Serial` from a counter in `Game`; all game logic that needs a stable order
//! sorts by it, and saves record it.
//!
//! Tests: in game/save/tests.rs.

use crate::game::Game;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Serial(pub u64);

impl Game {
    /// The next creation number.
    pub(crate) fn take_serial(&mut self) -> Serial {
        let s = Serial(self.next_serial);
        self.next_serial += 1;
        s
    }

    /// An entity's creation number (entities without one sort first, by id).
    pub fn serial_of(&self, entity: hecs::Entity) -> Serial {
        self.world.get::<&Serial>(entity).map_or(Serial(0), |s| *s)
    }

    /// Sort entities oldest first.
    pub fn sort_by_serial(&self, entities: &mut [hecs::Entity]) {
        entities.sort_by_key(|&e| (self.serial_of(e), e.id()));
    }
}
