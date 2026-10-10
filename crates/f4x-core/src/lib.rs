//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`Game::apply`] as [`Command`]s, which yield
//! [`Event`]s. The command log plus the seed fully reproduces a game.

pub mod city;
pub mod content;
pub mod featuregen;
pub mod map;
pub mod mapgen;
#[cfg(test)]
mod testutil;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    EndTurn,
    FoundCity {
        owner: city::PlayerId,
        pos: city::Position,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TurnStarted {
        turn: u32,
    },
    CityFounded {
        city: hecs::Entity,
        pos: city::Position,
    },
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    FoundCity(city::FoundCityError),
}

pub struct Game {
    pub world: hecs::World,
    pub turn: u32,
    pub map: Option<map::Map>,
    pub content: content::Content,
    rng: Pcg64Mcg,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            world: hecs::World::new(),
            turn: 1,
            map: None,
            content: content::Content::default(),
            rng: Pcg64Mcg::seed_from_u64(seed),
        }
    }

    /// Start a game on a freshly generated map.
    pub fn with_generated_map(
        seed: u64,
        params: &mapgen::GenParams,
        content: &content::Content,
    ) -> Result<Self, mapgen::GenError> {
        let mut game = Self::new(seed);
        game.map = Some(mapgen::generate(seed, params, content)?);
        game.content = content.clone();
        Ok(game)
    }

    /// All cities with their positions.
    pub fn cities(&self) -> Vec<(hecs::Entity, city::City, city::Position)> {
        let mut out: Vec<_> = self
            .world
            .query::<(&city::City, &city::Position)>()
            .iter()
            .map(|(e, (c, p))| (e, c.clone(), *p))
            .collect();
        out.sort_by_key(|(e, ..)| e.id());
        out
    }

    pub fn city_at(&self, pos: city::Position) -> Option<hecs::Entity> {
        self.cities()
            .into_iter()
            .find(|(_, _, p)| *p == pos)
            .map(|(e, ..)| e)
    }

    /// Whether a city could be founded at `pos` right now.
    pub fn can_found_city(&self, pos: city::Position) -> Result<(), city::FoundCityError> {
        let existing = self.cities().into_iter().map(|(_, _, p)| p);
        city::check_site(self.map.as_ref(), &self.content, pos, existing)
    }

    pub fn apply(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::EndTurn => {
                self.turn += 1;
                vec![Event::TurnStarted { turn: self.turn }]
            }
            Command::FoundCity { owner, pos, name } => match self.can_found_city(pos) {
                Ok(()) => {
                    let city = self.world.spawn((city::City { name, owner }, pos));
                    vec![Event::CityFounded { city, pos }]
                }
                Err(e) => vec![Event::Rejected(Rejection::FoundCity(e))],
            },
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
