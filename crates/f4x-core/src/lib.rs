//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`Game::apply`] as [`Command`]s, which yield
//! [`Event`]s. The command log plus the seed fully reproduces a game.

pub mod art;
pub mod city;
pub mod content;
pub mod featuregen;
pub mod map;
pub mod mapgen;
pub mod production;
pub mod rules;
#[cfg(test)]
mod testutil;
pub mod units;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    EndTurn,
    FoundCity {
        owner: city::PlayerId,
        race: rules::RaceId,
        pos: city::Position,
        name: String,
    },
    MoveUnit {
        by: city::PlayerId,
        unit: hecs::Entity,
        to: city::Position,
    },
    DisbandUnit {
        by: city::PlayerId,
        unit: hecs::Entity,
    },
    /// Equip a hero. Items are not yet drawn from a player inventory (that arrives
    /// with treasuries and loot); any defined item can be equipped.
    EquipItem {
        by: city::PlayerId,
        unit: hecs::Entity,
        item: rules::ItemId,
    },
    UnequipItem {
        by: city::PlayerId,
        unit: hecs::Entity,
        item: rules::ItemId,
    },
    QueueProduction {
        by: city::PlayerId,
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    DequeueProduction {
        by: city::PlayerId,
        city: hecs::Entity,
        index: usize,
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
    UnitMoved {
        unit: hecs::Entity,
        from: city::Position,
        to: city::Position,
    },
    UnitDisbanded {
        unit: hecs::Entity,
    },
    UnitTrained {
        city: hecs::Entity,
        unit: hecs::Entity,
        pos: city::Position,
    },
    BuildingCompleted {
        city: hecs::Entity,
        building: rules::BuildingId,
    },
    ItemEquipped {
        unit: hecs::Entity,
        item: rules::ItemId,
    },
    ItemUnequipped {
        unit: hecs::Entity,
        item: rules::ItemId,
    },
    ProductionQueued {
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    ProductionDequeued {
        city: hecs::Entity,
        index: usize,
    },
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    FoundCity(city::FoundCityError),
    Move(units::MoveError),
    Equip(units::EquipError),
    Production(production::ProductionError),
    NoSuchUnit,
    NotYours,
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
                self.refresh_moves();
                let mut events = self.process_production();
                self.turn += 1;
                events.push(Event::TurnStarted { turn: self.turn });
                events
            }
            Command::FoundCity {
                owner,
                race,
                pos,
                name,
            } => match self.can_found_city(pos) {
                Ok(()) => {
                    let city = self.world.spawn((
                        city::City { name, owner, race },
                        pos,
                        production::CityBuildings::default(),
                        production::ProductionQueue::default(),
                    ));
                    vec![Event::CityFounded { city, pos }]
                }
                Err(e) => vec![Event::Rejected(Rejection::FoundCity(e))],
            },
            Command::MoveUnit { by, unit, to } => match self.move_unit(by, unit, to) {
                Ok(from) => vec![Event::UnitMoved { unit, from, to }],
                Err(e) => vec![Event::Rejected(Rejection::Move(e))],
            },
            Command::DisbandUnit { by, unit } => {
                if self.unit_def(unit).is_none() {
                    return vec![Event::Rejected(Rejection::NoSuchUnit)];
                }
                if self.unit_owner(unit) != Some(by) {
                    return vec![Event::Rejected(Rejection::NotYours)];
                }
                let _ = self.world.despawn(unit);
                vec![Event::UnitDisbanded { unit }]
            }
            Command::EquipItem { by, unit, item } => match self.equip_item(by, unit, item) {
                Ok(()) => vec![Event::ItemEquipped { unit, item }],
                Err(e) => vec![Event::Rejected(Rejection::Equip(e))],
            },
            Command::UnequipItem { by, unit, item } => match self.unequip_item(by, unit, item) {
                Ok(()) => vec![Event::ItemUnequipped { unit, item }],
                Err(e) => vec![Event::Rejected(Rejection::Equip(e))],
            },
            Command::QueueProduction { by, city, item } => {
                match self.queue_production(by, city, item) {
                    Ok(()) => vec![Event::ProductionQueued { city, item }],
                    Err(e) => vec![Event::Rejected(Rejection::Production(e))],
                }
            }
            Command::DequeueProduction { by, city, index } => {
                match self.dequeue_production(by, city, index) {
                    Ok(()) => vec![Event::ProductionDequeued { city, index }],
                    Err(e) => vec![Event::Rejected(Rejection::Production(e))],
                }
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
