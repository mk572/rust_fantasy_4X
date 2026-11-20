//! Creation numbers: a stable, never-reused order for entities.
//!
//! `hecs` entity ids can be reused after an entity is despawned, so sorting by id does not
//! give creation order and changes if a game is saved and reloaded. Every unit and city
//! carries a `Serial` from a counter in `Game`; all game logic that needs a stable order
//! sorts by it, and saves record it.
//!
//! Tests: in game/save/tests.rs.

use crate::game::Game;

pub use f4x_def::id::Serial;

impl Game {
    /// The next creation number.
    pub(crate) fn take_serial(&mut self) -> Serial {
        let s = Serial(self.next_serial);
        self.next_serial += 1;
        s
    }
}
