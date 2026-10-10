//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`Game::apply`] as [`Command`]s, which yield
//! [`Event`]s. The command log plus the seed fully reproduces a game.

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    EndTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TurnStarted { turn: u32 },
}

pub struct Game {
    pub world: hecs::World,
    pub turn: u32,
    rng: Pcg64Mcg,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            world: hecs::World::new(),
            turn: 1,
            rng: Pcg64Mcg::seed_from_u64(seed),
        }
    }

    pub fn apply(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::EndTurn => {
                self.turn += 1;
                vec![Event::TurnStarted { turn: self.turn }]
            }
        }
    }

    /// Next value from the game's seeded RNG (all randomness must come from here).
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_turn_advances_turn() {
        let mut g = Game::new(1);
        assert_eq!(
            g.apply(Command::EndTurn),
            vec![Event::TurnStarted { turn: 2 }]
        );
    }

    #[test]
    fn same_seed_same_rng_stream() {
        let (mut a, mut b) = (Game::new(42), Game::new(42));
        assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
    }
}
