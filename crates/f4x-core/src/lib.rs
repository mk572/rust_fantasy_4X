//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`Game::apply`] as [`Command`]s, which yield
//! [`Event`]s. The command log plus the seed fully reproduces a game.

pub mod city;
pub mod content;
pub mod economy;
pub mod featuregen;
pub mod fog;
pub mod help;
pub mod map;
pub mod mapgen;
pub mod movement;
pub mod pathing;
pub mod player;
pub mod production;
pub mod rules;
#[cfg(test)]
mod testutil;
pub mod turn;
pub mod units;

use rand::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    EndTurn,
    /// Create a unit (summoning, production and scenario setup go through this).
    CreateUnit {
        owner: city::PlayerId,
        pos: city::Position,
        unit: rules::UnitId,
    },
    /// Move a stack one tile.
    MoveStack {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// Give a stack a standing order to walk to a tile.
    GoTo {
        units: Vec<hecs::Entity>,
        to: city::Position,
    },
    /// Cancel standing orders.
    ClearOrders {
        units: Vec<hecs::Entity>,
    },
    /// Move a stack to the same tile on the other plane.
    ShiftPlane {
        units: Vec<hecs::Entity>,
    },
    /// Found a city. No population makes an outpost; race decides what it can build.
    FoundCity {
        owner: city::PlayerId,
        pos: city::Position,
        name: String,
        race: Option<rules::RaceId>,
        population: u32,
    },
    SetProduction {
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    /// Pay gold to finish the current item at the next turn change.
    BuyProduction {
        city: hecs::Entity,
    },
    SetFarmers {
        city: hecs::Entity,
        farmers: u32,
    },
    SellBuilding {
        city: hecs::Entity,
        building: rules::BuildingId,
    },
    SetTaxRate {
        player: city::PlayerId,
        rate: u8,
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
    ProductionCompleted {
        city: hecs::Entity,
        item: production::ProductionItem,
    },
    CityGrew {
        city: hecs::Entity,
        figures: u32,
    },
    CityShrank {
        city: hecs::Entity,
        figures: u32,
    },
    /// Units dismissed because their owner could not feed or pay them.
    UnitsDisbanded {
        player: city::PlayerId,
        units: u32,
    },
    UnitCreated {
        unit: hecs::Entity,
        pos: city::Position,
    },
    UnitsMoved {
        units: Vec<hecs::Entity>,
        from: city::Position,
        to: city::Position,
    },
    /// Moving into hostile units or a hostile city: the attack is for the combat layer.
    AttackRequested {
        attackers: Vec<hecs::Entity>,
        from: city::Position,
        to: city::Position,
        defenders: Vec<hecs::Entity>,
    },
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    FoundCity(city::FoundCityError),
    Move(movement::MoveError),
    Production(production::ProductionError),
}

pub struct Game {
    pub world: hecs::World,
    pub turn: u32,
    pub map: Option<map::Map>,
    pub content: content::Content,
    pub players: Vec<player::Player>,
    pub fog: fog::Fog,
    rng: Pcg64Mcg,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            world: hecs::World::new(),
            turn: 1,
            map: None,
            content: content::Content::default(),
            players: Vec::new(),
            fog: fog::Fog::default(),
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

    /// Let `player` see the tiles within `radius` of `at`.
    pub fn explore(&mut self, player: city::PlayerId, at: city::Position, radius: u32) {
        if let (Some(map), true) = (self.map.as_ref(), self.fog.is_empty()) {
            self.fog = fog::Fog::new(map.width(), map.height());
        }
        self.fog.explore(player, at, radius);
    }

    pub fn player(&self, id: city::PlayerId) -> Option<&player::Player> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn player_mut(&mut self, id: city::PlayerId) -> Option<&mut player::Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    pub fn apply(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::EndTurn => self.end_turn(),
            Command::CreateUnit { owner, pos, unit } => {
                let unit = self.spawn_unit(owner, pos, unit);
                vec![Event::UnitCreated { unit, pos }]
            }
            Command::MoveStack { units, to } => self.do_move(&units, to),
            Command::GoTo { units, to } => match self.find_path(&units, to) {
                Some(path) => {
                    for &u in &units {
                        let _ = self.world.insert_one(u, units::Order::GoTo(path.clone()));
                    }
                    self.follow_orders(&units)
                }
                None => vec![Event::Rejected(Rejection::Move(
                    movement::MoveError::Impassable,
                ))],
            },
            Command::ClearOrders { units } => {
                for u in units {
                    let _ = self.world.remove_one::<units::Order>(u);
                }
                Vec::new()
            }
            Command::ShiftPlane { units } => {
                let from = units.first().and_then(|&u| self.unit_position(u));
                match (self.shift_plane(&units), from) {
                    (Ok(to), Some(from)) => vec![Event::UnitsMoved { units, from, to }],
                    (Err(e), _) => vec![Event::Rejected(Rejection::Move(e))],
                    (Ok(_), None) => Vec::new(),
                }
            }
            Command::FoundCity {
                owner,
                pos,
                name,
                race,
                population,
            } => match self.can_found_city(pos) {
                Ok(()) => {
                    let city = self.spawn_city(owner, pos, name, race, population);
                    vec![Event::CityFounded { city, pos }]
                }
                Err(e) => vec![Event::Rejected(Rejection::FoundCity(e))],
            },
            Command::SetProduction { city, item } => match self.set_production(city, item) {
                Ok(()) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::BuyProduction { city } => match self.buy_production(city) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SellBuilding { city, building } => match self.sell_building(city, building) {
                Ok(_) => Vec::new(),
                Err(e) => vec![Event::Rejected(Rejection::Production(e))],
            },
            Command::SetFarmers { city, farmers } => {
                let figures = self
                    .world
                    .get::<&economy::Population>(city)
                    .map_or(0, |p| p.figures);
                let min = self.city_min_farmers(city);
                let _ = self
                    .world
                    .insert_one(city, economy::Farmers(farmers.clamp(min, figures)));
                Vec::new()
            }
            Command::SetTaxRate { player, rate } => {
                let max = self
                    .content
                    .rules
                    .config
                    .tax_unrest_percent
                    .len()
                    .saturating_sub(1) as u8;
                if let Some(p) = self.player_mut(player) {
                    p.tax_rate = rate.min(max);
                }
                Vec::new()
            }
        }
    }

    fn do_move(&mut self, units: &[hecs::Entity], to: city::Position) -> Vec<Event> {
        let from = units.first().and_then(|&u| self.unit_position(u));
        match (self.move_stack(units, to), from) {
            (Ok(movement::Entry::Free), Some(from)) => {
                vec![Event::UnitsMoved {
                    units: units.to_vec(),
                    from,
                    to,
                }]
            }
            (Ok(movement::Entry::Hostile(defenders)), Some(from)) => vec![Event::AttackRequested {
                attackers: units.to_vec(),
                from,
                to,
                defenders,
            }],
            (Err(e), _) => vec![Event::Rejected(Rejection::Move(e))],
            (Ok(_), None) => Vec::new(),
        }
    }

    /// Advance each listed unit along its standing order as far as its moves allow.
    fn follow_orders(&mut self, units: &[hecs::Entity]) -> Vec<Event> {
        let mut events = Vec::new();
        // Units sharing a tile and an order walk together.
        let mut groups: Vec<Vec<hecs::Entity>> = Vec::new();
        for &u in units {
            let Some(pos) = self.unit_position(u) else {
                continue;
            };
            let order = self
                .world
                .get::<&units::Order>(u)
                .ok()
                .map(|o| (*o).clone());
            let Some(order) = order else { continue };
            match groups.iter_mut().find(|g| {
                let f = g[0];
                self.unit_position(f) == Some(pos)
                    && self
                        .world
                        .get::<&units::Order>(f)
                        .ok()
                        .map(|o| (*o).clone())
                        == Some(order.clone())
            }) {
                Some(g) => g.push(u),
                None => groups.push(vec![u]),
            }
        }
        for group in groups {
            let units::Order::GoTo(mut path) = self
                .world
                .get::<&units::Order>(group[0])
                .map(|o| (*o).clone())
                .expect("grouped by order");
            while let Some(&next) = path.first() {
                let from = self.unit_position(group[0]).expect("on map");
                match self.move_stack(&group, next) {
                    Ok(movement::Entry::Free) => {
                        path.remove(0);
                        events.push(Event::UnitsMoved {
                            units: group.clone(),
                            from,
                            to: next,
                        });
                    }
                    Ok(movement::Entry::Hostile(defenders)) => {
                        events.push(Event::AttackRequested {
                            attackers: group.clone(),
                            from,
                            to: next,
                            defenders,
                        });
                        path.clear();
                    }
                    Err(movement::MoveError::NoMovesLeft) => break,
                    Err(_) => {
                        path.clear();
                    }
                }
            }
            for &u in &group {
                if path.is_empty() {
                    let _ = self.world.remove_one::<units::Order>(u);
                } else {
                    let _ = self.world.insert_one(u, units::Order::GoTo(path.clone()));
                }
            }
        }
        events
    }

    fn end_turn(&mut self) -> Vec<Event> {
        self.turn += 1;
        for unit in self.all_units() {
            let moves = self.unit_max_moves(unit);
            let _ = self.world.insert_one(unit, units::MovesLeft(moves));
        }
        let mut events = vec![Event::TurnStarted { turn: self.turn }];
        events.extend(self.process_turn());
        let with_orders: Vec<_> = self
            .world
            .query::<&units::Order>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        let mut with_orders = with_orders;
        with_orders.sort_by_key(|e| e.id());
        events.extend(self.follow_orders(&with_orders));
        events
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
