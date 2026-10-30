//! Ships carrying land units.
//!
//! A ship's `Transport` stat is how many land units it holds. Land units cannot enter the
//! sea alone: they board by moving onto a sea square where a friendly ship has room, or by
//! moving with a ship they share a square (or city) with. Passengers ride at no cost and
//! follow the ship; stepping from a ship onto land is an ordinary move and unloads them. A
//! passenger left at sea without a ship drowns.

use crate::city::economy::Economy;
use crate::game::serial::Serial;
use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::{PlayerId, MONSTERS};
use crate::rules::{MovementType, Stat};

use crate::game::{Event, Game};

/// A ship let go for lack of gold with units still aboard (task 292): it belongs to nobody and
/// sails, a turn's worth at a time, for the nearest free shore, where it unloads and is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marooned;

/// The ship a land unit is aboard, by creation number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aboard(pub Serial);

impl Economy<'_> {
    /// A unit that carries others: it sails and has room.
    pub fn is_ship(&self, unit: hecs::Entity) -> bool {
        let units = self.units();
        units.unit_stat(unit, Stat::Transport) > 0
            && units
                .unit_def(unit)
                .is_some_and(|d| d.movement == MovementType::Sailing)
    }

    /// Whether the unit cannot cross the sea by itself: it walks and neither flies nor is
    /// carried through the air nor is non-corporeal nor walks on water.
    pub fn needs_ship(&self, unit: hecs::Entity, stack_flies: bool) -> bool {
        let units = self.units();
        !stack_flies
            && units.unit_def(unit).is_some_and(|d| {
                d.movement == MovementType::Walking
                    && !units
                        .unit_abilities(unit)
                        .iter()
                        .any(|a| a == "flying" || a == "non_corporeal" || a == "water_walking")
            })
    }

    /// The ship `unit` is aboard, if it still exists.
    pub fn carrier_of(&self, unit: hecs::Entity) -> Option<hecs::Entity> {
        let serial = self.world.get::<&Aboard>(unit).ok()?.0;
        self.units().entity_of_serial(serial)
    }

    /// Units aboard `ship`.
    pub fn passengers_of(&self, ship: hecs::Entity) -> Vec<hecs::Entity> {
        let units = self.units();
        let serial = units.serial_of(ship);
        let mut out: Vec<_> = self
            .world
            .query::<&Aboard>()
            .iter()
            .filter(|(_, a)| a.0 == serial)
            .map(|(e, _)| e)
            .collect();
        units.sort_by_serial(&mut out);
        out
    }

    /// Free places on a ship.
    pub fn ship_room(&self, ship: hecs::Entity) -> u32 {
        let cap = self.units().unit_stat(ship, Stat::Transport).max(0) as u32;
        cap.saturating_sub(self.passengers_of(ship).len() as u32)
    }

    /// `owner`'s ships standing on `at`, oldest first.
    pub fn ships_at(&self, at: Position, owner: PlayerId) -> Vec<hecs::Entity> {
        let units = self.units();
        units
            .units_at(at)
            .into_iter()
            .filter(|&u| units.unit_owner(u) == Some(owner) && self.is_ship(u))
            .collect()
    }

    /// Whether any of `owner`'s ships on `at` has a free place.
    pub fn ship_room_at(&self, at: Position, owner: PlayerId) -> bool {
        if let Some((o, squares)) = self.ship_room {
            if o == owner {
                return squares.contains(&at);
            }
        }
        // Path searches ask this for every sea square they look at: no list, no sorting, and the
        // cheap tests (owner, sailing) before the stat.
        let units = self.units();
        self.world
            .query::<(&crate::unit::Unit, &Position)>()
            .iter()
            .filter(|(_, (_, p))| **p == at)
            .any(|(u, _)| {
                units.unit_owner(u) == Some(owner)
                    && units
                        .unit_def(u)
                        .is_some_and(|d| d.movement == MovementType::Sailing)
                    && self.ship_room(u) > 0
            })
    }

    /// Every square where `owner` has a ship with a free place: one pass over the units, for a
    /// search that asks about many squares.
    pub fn ship_room_squares(&self, owner: PlayerId) -> std::collections::HashSet<Position> {
        let units = self.units();
        self.world
            .query::<(&crate::unit::Unit, &Position)>()
            .iter()
            .filter(|&(u, _)| {
                units.unit_owner(u) == Some(owner)
                    && units
                        .unit_def(u)
                        .is_some_and(|d| d.movement == MovementType::Sailing)
                    && self.ship_room(u) > 0
            })
            .map(|(_, (_, p))| *p)
            .collect()
    }

    /// Whether two squares touch (the map wraps east and west).
    pub(crate) fn next_to(&self, a: Position, b: Position) -> bool {
        self.map
            .is_some_and(|m| crate::unit::movement::adjacent(m.width(), a, b))
    }

    pub(crate) fn is_sea(&self, at: Position) -> bool {
        self.terrain_class_at(at) == Some(TerrainClass::Ocean)
    }

    /// Land units at sea with no ship under them: they are lost (`Game::drown_stranded`).
    pub(crate) fn stranded(&self) -> Vec<(PlayerId, hecs::Entity)> {
        let units = self.units();
        let mut lost = Vec::new();
        for unit in units.all_units() {
            let (Some(at), Some(owner)) = (units.unit_position(unit), units.unit_owner(unit))
            else {
                continue;
            };
            if !self.is_sea(at) || !self.needs_ship(unit, false) {
                continue;
            }
            // Under a ship, or held by one standing on this square.
            let held = self
                .carrier_of(unit)
                .is_some_and(|s| units.unit_position(s) == Some(at));
            if !held {
                lost.push((owner, unit));
            }
        }
        lost
    }
}

/// Where a stack's land units ride after it has moved to `to`: those that need a ship and are
/// not carried by one of the stack go aboard a ship of the stack, else one waiting there, if it
/// has room; on land nobody is aboard. One unit at a time, as each boarding fills a place.
pub(crate) fn update_aboard(
    world: &mut hecs::World,
    map: Option<&crate::map::Map>,
    content: &crate::content::Content,
    stack: &[hecs::Entity],
    to: Position,
    stack_flies: bool,
) {
    fn economy<'a>(
        world: &'a hecs::World,
        map: Option<&'a crate::map::Map>,
        content: &'a crate::content::Content,
    ) -> Economy<'a> {
        Economy::new(world, map, content)
    }
    let e = economy(world, map, content);
    let (to_sea, owner) = (e.is_sea(to), e.units().unit_owner(stack[0]));
    let Some(owner) = owner else {
        return;
    };
    for &u in stack {
        let board = {
            let e = economy(world, map, content);
            if !e.needs_ship(u, stack_flies) {
                continue;
            }
            if !to_sea {
                None
            } else if e.carrier_of(u).is_some_and(|c| stack.contains(&c)) {
                continue;
            } else {
                let mut ships: Vec<hecs::Entity> =
                    stack.iter().copied().filter(|&s| e.is_ship(s)).collect();
                ships.extend(
                    e.ships_at(to, owner)
                        .into_iter()
                        .filter(|s| !stack.contains(s)),
                );
                let ship = ships.into_iter().find(|&s| e.ship_room(s) > 0);
                match ship {
                    Some(ship) => Some(Aboard(e.units().serial_of(ship))),
                    None => continue,
                }
            }
        };
        match board {
            Some(aboard) => {
                let _ = world.insert_one(u, aboard);
            }
            None => {
                let _ = world.remove_one::<Aboard>(u);
            }
        }
    }
}

impl Game {
    /// Every marooned ship sails on towards the nearest shore with room for its passengers; the
    /// ones that arrive land them and are released. A ship with nowhere to land loses them.
    pub(crate) fn sail_marooned(&mut self) -> Vec<Event> {
        let mut ships: Vec<hecs::Entity> = self
            .world
            .query::<&Marooned>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        self.sort_by_serial(&mut ships);
        let mut events = Vec::new();
        for ship in ships {
            let passengers = self.economy().passengers_of(ship);
            let (Some(mut at), Some(&first)) =
                (self.units().unit_position(ship), passengers.first())
            else {
                self.remove_unit(ship, crate::hero::Fate::Released);
                continue;
            };
            let owner = self.units().unit_owner(first).unwrap_or(MONSTERS);
            let Some((path, landing)) = self.nearest_landing(ship, at, owner, &passengers) else {
                let lost = passengers.len() as u32;
                for p in passengers {
                    self.remove_unit(p, crate::hero::Fate::Slain);
                }
                self.remove_unit(ship, crate::hero::Fate::Released);
                events.push(Event::UnitsDrowned {
                    player: owner,
                    units: lost,
                });
                continue;
            };
            let traits = self.economy().stack_traits(&[ship]);
            let mut left = self.units().unit_max_moves(ship);
            for next in path {
                let cost = self
                    .economy()
                    .step_cost(ship, at, next, traits)
                    .unwrap_or(2);
                if cost > left {
                    break;
                }
                left -= cost;
                at = next;
                let _ = self.world.insert_one(ship, at);
                for &p in &passengers {
                    let _ = self.world.insert_one(p, at);
                }
            }
            if self.economy().next_to(at, landing) {
                for &p in &passengers {
                    let _ = self.world.remove_one::<Aboard>(p);
                    let _ = self.world.insert_one(p, landing);
                }
                self.remove_unit(ship, crate::hero::Fate::Released);
                events.push(Event::ShipLanded {
                    player: owner,
                    units: passengers.len() as u32,
                    at: landing,
                });
            }
        }
        events
    }

    /// The nearest sea square from which `passengers` can step ashore, by the way over open sea
    /// (breadth first, so ties fall in a fixed order): the squares to sail through, and the land
    /// square they will land on. Land is free when nobody else stands on it, no lair guards it,
    /// every passenger can enter it and the stack there stays within the limit.
    pub(crate) fn nearest_landing(
        &self,
        ship: hecs::Entity,
        from: Position,
        owner: PlayerId,
        passengers: &[hecs::Entity],
    ) -> Option<(Vec<Position>, Position)> {
        use std::collections::{HashMap, VecDeque};
        let _ = ship;
        let e = self.economy();
        let mut came: HashMap<Position, Option<Position>> = HashMap::new();
        let mut queue = VecDeque::new();
        came.insert(from, None);
        queue.push_back(from);
        while let Some(here) = queue.pop_front() {
            for next in crate::unit::pathing::neighbours(self, here) {
                if e.is_sea(next) {
                    if !came.contains_key(&next) && e.hostile_at(next, owner).is_none() {
                        came.insert(next, Some(here));
                        queue.push_back(next);
                    }
                    continue;
                }
                if self.free_landing(next, here, owner, passengers) {
                    let mut path = Vec::new();
                    let mut at = here;
                    while let Some(Some(prev)) = came.get(&at).copied() {
                        path.push(at);
                        at = prev;
                    }
                    path.reverse();
                    return Some((path, next));
                }
            }
        }
        None
    }

    pub(crate) fn free_landing(
        &self,
        land: Position,
        from: Position,
        owner: PlayerId,
        passengers: &[hecs::Entity],
    ) -> bool {
        let e = self.economy();
        if e.hostile_at(land, owner).is_some() || e.lair_at(land).is_some() {
            return false;
        }
        let there = self.units().units_at(land).len();
        if there + passengers.len() > self.max_tile() {
            return false;
        }
        let traits = crate::unit::movement::StackTraits::default();
        passengers
            .iter()
            .all(|&p| e.step_cost(p, from, land, traits).is_ok())
    }

    /// Land units at sea with no ship under them are lost, as the units of a defeated army are
    /// when they flee: where free land lies beside them they scramble ashore, half of the troops
    /// and three in four of the heroes making it (`overland_aftermath`'s odds); anywhere else
    /// they drown. Fliers and swimmers never need a ship, so they never come here (task 235).
    pub(crate) fn drown_stranded(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut drowned: Vec<(crate::player::PlayerId, u32)> = Vec::new();
        for (owner, unit) in self.economy().stranded() {
            let at = self.units().unit_position(unit);
            let landing = at.and_then(|at| {
                crate::unit::pathing::neighbours(self, at)
                    .into_iter()
                    .find(|&next| {
                        !self.economy().is_sea(next) && self.free_landing(next, at, owner, &[unit])
                    })
            });
            let hero = self
                .units()
                .unit_def(unit)
                .is_some_and(|d| d.kind == crate::rules::UnitKind::Hero);
            let percent = if hero { 75 } else { 50 };
            if let Some(land) = landing.filter(|_| self.roll(100) <= percent) {
                let _ = self.world.remove_one::<Aboard>(unit);
                let _ = self.world.insert_one(unit, land);
                continue;
            }
            self.remove_unit(unit, crate::hero::Fate::Slain);
            match drowned.iter_mut().find(|(p, _)| *p == owner) {
                Some((_, n)) => *n += 1,
                None => drowned.push((owner, 1)),
            }
        }
        for (player, units) in drowned {
            events.push(Event::UnitsDrowned { player, units });
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_units;
    use crate::game::Command;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::Registry;
    use crate::unit::movement::{Entry, MoveError};
    use crate::unit::MovesLeft;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(x: u32, y: u32) -> Position {
        Position {
            plane: Plane::Arcanus,
            x,
            y,
        }
    }

    /// 14x8: row 4 is grass x0-4, sea x5-9, grass x10-13; everything else grass.
    fn world() -> Game {
        let mut c = content_with_units();
        let mut units = c.rules.units.defs().to_vec();
        for u in units.iter_mut() {
            if u.id == "boat" {
                u.stats.insert(Stat::Transport, 2);
            }
        }
        units.push(crate::game::testutil::real_unit("floating_island"));
        c.rules.units = Registry::new_sorted(units);
        let (grass, ocean) = (
            c.terrains.id_of("grassland").unwrap(),
            c.terrains.id_of("ocean").unwrap(),
        );
        let build = || {
            let mut g = Grid::new(14, 8, grass);
            for x in 5..10 {
                g.set(x, 4, ocean);
            }
            g
        };
        let mut g = Game::new(2);
        g.content = c;
        g.map = Some(Map::new(build(), build()));
        for id in [P0, P1] {
            g.add_player(id, "");
        }
        g
    }

    fn unit(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
        let u = g.content.rules.units.id_of(id).unwrap();
        g.spawn_unit(P0, pos, u)
    }

    #[test]
    fn wind_mastery_speeds_the_owners_ships_and_slows_the_others() {
        let mut g = world();
        let wm = crate::game::testutil::real_spells()
            .into_iter()
            .find(|s| s.id == "wind_mastery")
            .unwrap();
        g.content.rules.spells = Registry::new(vec![wm]);
        let wm = g.content.rules.spells.id_of("wind_mastery").unwrap();
        for id in [P0, P1] {
            g.init_wizard(id, &[("life", 1)], &[], None).ok();
        }
        let boat = unit(&mut g, "boat", at(5, 4));
        let scout = unit(&mut g, "scout", at(4, 4));
        let walker = unit(&mut g, "walker", at(4, 3));
        let max = |g: &Game, u| g.units().unit_max_moves(u);
        assert_eq!((max(&g, boat), max(&g, scout)), (8, 6));
        // The owner's Wind Mastery: half as far again, for ships and fliers only.
        g.add_global(P0, wm);
        assert_eq!((max(&g, boat), max(&g, scout)), (12, 9));
        assert_eq!(max(&g, walker), 4);
        // A rival's: halved for the wizard without it.
        g.clear_globals(P0);
        g.add_global(P1, wm);
        assert_eq!((max(&g, boat), max(&g, scout)), (4, 3));
        assert_eq!(max(&g, walker), 4);
        // Both have it: both get the bonus, nothing stacks.
        g.add_global(P0, wm);
        assert_eq!((max(&g, boat), max(&g, scout)), (12, 9));
    }

    #[test]
    fn land_units_cannot_enter_the_sea_alone() {
        let mut g = world();
        let w = unit(&mut g, "walker", at(4, 4));
        assert_eq!(g.move_stack(&[w], at(5, 4)), Err(MoveError::Impassable));
    }

    #[test]
    fn a_unit_boards_a_waiting_ship_and_rides_with_it() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        assert_eq!(g.move_stack(&[w], at(5, 4)), Ok(Entry::Free));
        assert_eq!(g.economy().carrier_of(w), Some(boat));
        assert_eq!(g.economy().ship_room(boat), 1);
        let moves = g.units().unit_moves_left(w);
        // The ship sails alone; its passenger comes along at no cost.
        g.move_stack(&[boat], at(6, 4)).unwrap();
        assert_eq!(g.units().unit_position(w), Some(at(6, 4)));
        assert_eq!(g.units().unit_moves_left(w), moves);
        assert_eq!(g.economy().carrier_of(w), Some(boat));
        // Selected together, they sail as one stack too.
        g.move_stack(&[boat, w], at(7, 4)).unwrap();
        assert_eq!(g.units().unit_position(w), Some(at(7, 4)));
        assert_eq!(g.units().unit_moves_left(w), moves, "riding is free");
    }

    #[test]
    fn path_estimates_use_the_ships_speed_not_the_passengers() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[w], at(5, 4)).unwrap();
        let path: Vec<_> = (6..10).map(|x| at(x, 4)).collect();
        let alone = g.path_turns(&[boat], &path);
        assert_eq!(g.path_turns(&[boat, w], &path), alone);
        assert_eq!(
            alone, 0,
            "four sea squares fit in the ship's eight half-moves"
        );
    }

    #[test]
    fn a_ship_holds_only_so_many() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let three: Vec<_> = (0..3).map(|_| unit(&mut g, "walker", at(4, 4))).collect();
        assert_eq!(g.move_stack(&three, at(5, 4)), Err(MoveError::NoRoom));
        g.move_stack(&three[..2], at(5, 4)).unwrap();
        assert_eq!(g.economy().ship_room(boat), 0);
        assert_eq!(
            g.move_stack(&three[2..], at(5, 4)),
            Err(MoveError::Impassable),
            "a full ship is no place to step"
        );
    }

    #[test]
    fn units_load_in_port_and_unload_onto_land() {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(4, 4),
            name: "Port".into(),
            race: None,
            population: 3,
        });
        let boat = unit(&mut g, "boat", at(4, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        // The stack sails out of the port: the walker boards for free.
        g.move_stack(&[boat, w], at(5, 4)).unwrap();
        assert_eq!(g.economy().carrier_of(w), Some(boat));
        g.move_stack(&[boat], at(6, 4)).unwrap();
        g.move_stack(&[boat], at(7, 4)).unwrap();
        g.move_stack(&[boat], at(8, 4)).unwrap();
        let _ = g.world.insert_one(boat, MovesLeft(8));
        g.move_stack(&[boat], at(9, 4)).unwrap();
        assert_eq!(g.units().unit_position(w), Some(at(9, 4)));
        // Stepping ashore is an ordinary move.
        let _ = g.world.insert_one(w, MovesLeft(2));
        g.move_stack(&[w], at(10, 4)).unwrap();
        assert_eq!(g.economy().carrier_of(w), None, "no longer aboard");
        assert_eq!(g.units().unit_position(w), Some(at(10, 4)));
        assert_eq!(
            g.units().unit_moves_left(w),
            0,
            "grassland costs a whole move"
        );
        // The ship stays; its place is free again.
        assert_eq!(g.economy().ship_room(boat), 2);
    }

    #[test]
    fn passengers_ride_into_a_port_and_get_off_there() {
        let mut g = world();
        g.apply(Command::FoundCity {
            owner: P0,
            pos: at(10, 3),
            name: "Far Port".into(),
            race: None,
            population: 3,
        });
        let boat = unit(&mut g, "boat", at(9, 4));
        let w = unit(&mut g, "walker", at(10, 4));
        g.move_stack(&[w], at(9, 4)).unwrap();
        assert_eq!(g.economy().carrier_of(w), Some(boat));
        g.move_stack(&[boat, w], at(10, 3)).unwrap();
        assert_eq!(g.economy().carrier_of(w), None, "ashore in the port");
        assert_eq!(g.units().unit_position(w), Some(at(10, 3)));
    }

    #[test]
    fn at_sea_the_ships_are_decks_and_their_passengers_fight_on_them() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let walker = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[walker], at(5, 4)).unwrap();
        assert_eq!(g.economy().carrier_of(walker), Some(boat));
        let attacker_boat = {
            let u = g.content.rules.units.id_of("boat").unwrap();
            g.spawn_unit(P1, at(6, 4), u)
        };
        g.start_battle(&[attacker_boat], &[boat, walker]).unwrap();
        let b = g.battle().unwrap();
        assert_eq!(b.units.len(), 3, "the passenger is in the fight");
        let look = b.map.look().unwrap();
        assert_eq!(look.set, crate::battle::map::layout::Set::Water);
        // Every unit stands on a deck, and the ships stay where they are.
        for u in &b.units {
            assert!(b.map.is_ship_deck(u.cell), "{:?} is not on a deck", u.cell);
            assert_eq!(u.sails, u.moves == 0, "a ship does not sail off its deck");
        }
        let on_foot = b.units.iter().find(|u| !u.sails).unwrap();
        assert_eq!(b.map.enter_cost(on_foot.cell, false), Some(2));
        // The water between and round the decks is closed to a walker.
        assert_eq!(b.map.enter_cost((10, 10), false), None);
        // The attackers win and the defenders are all dead, the passenger with its ship.
        let active = &mut g.battle.as_mut().unwrap().battle;
        active.winner = Some(crate::battle::Side::Attacker);
        for u in &mut active.units {
            if u.side == crate::battle::Side::Defender {
                u.status = crate::battle::Status::Dead;
            }
        }
        g.finish_battle();
        assert!(g.units().unit_position(walker).is_none());
    }

    #[test]
    fn a_floating_island_is_a_ship_in_a_sea_fight_and_its_passengers_stand_on_its_deck() {
        let mut g = world();
        let island = {
            let u = g.content.rules.units.id_of("floating_island").unwrap();
            g.spawn_unit(P0, at(5, 4), u)
        };
        let walkers: Vec<_> = (0..2).map(|_| unit(&mut g, "walker", at(4, 4))).collect();
        g.move_stack(&walkers, at(5, 4)).unwrap();
        let enemy = {
            let u = g.content.rules.units.id_of("boat").unwrap();
            g.spawn_unit(P1, at(6, 4), u)
        };
        let mut defenders = vec![island];
        defenders.extend(&walkers);
        g.start_battle(&[enemy], &defenders).unwrap();
        let b = g.battle().unwrap();
        let own: Vec<_> = b
            .units
            .iter()
            .filter(|u| u.side == crate::battle::Side::Defender)
            .collect();
        assert_eq!(
            own.len(),
            3,
            "the island fights like a ship, with its passengers"
        );
        for u in own {
            assert!(b.map.is_ship_deck(u.cell), "{:?}", u.cell);
        }
    }

    #[test]
    fn land_units_without_a_ship_drown() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[w], at(5, 4)).unwrap();
        // Open sea: no shore beside the square.
        let ocean = g.content.terrains.id_of("ocean").unwrap();
        for (x, y) in [(4, 3), (4, 4), (4, 5), (5, 3), (5, 5), (6, 3), (6, 5)] {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(x, y, ocean);
        }
        g.despawn_with_spells(boat);
        let events = g.drown_stranded();
        assert_eq!(
            events,
            vec![Event::UnitsDrowned {
                player: P0,
                units: 1
            }]
        );
        assert!(g.world.get::<&crate::unit::Unit>(w).is_err());
        // Fliers and ships are never in danger.
        let flier = unit(&mut g, "scout", at(6, 4));
        let other = unit(&mut g, "boat", at(7, 4));
        assert!(g.drown_stranded().is_empty());
        assert!(
            g.units().unit_position(flier).is_some() && g.units().unit_position(other).is_some()
        );
    }

    #[test]
    fn passengers_of_a_lost_ship_beside_the_shore_may_scramble_ashore() {
        let mut g = world();
        let mut ashore = 0;
        let runs = 30;
        for _ in 0..runs {
            let boat = unit(&mut g, "boat", at(5, 4));
            let w = unit(&mut g, "walker", at(4, 4));
            g.move_stack(&[w], at(5, 4)).unwrap();
            g.despawn_with_spells(boat);
            g.drown_stranded();
            if let Some(p) = g.units().unit_position(w) {
                assert!(!g.economy().is_sea(p), "the survivors stand on land");
                ashore += 1;
                g.remove_unit(w, crate::hero::Fate::Released);
            }
        }
        assert!(ashore > 0 && ashore < runs, "{ashore} of {runs} made it");
    }

    #[test]
    fn who_is_aboard_survives_a_save() {
        let mut g = world();
        let boat = unit(&mut g, "boat", at(5, 4));
        let w = unit(&mut g, "walker", at(4, 4));
        g.move_stack(&[w], at(5, 4)).unwrap();
        let content = g.content.clone();
        let back = g.snapshot().restore(content).unwrap();
        let (boat2, w2) = (
            back.units().entity_of_serial(g.serial_of(boat)).unwrap(),
            back.units().entity_of_serial(g.serial_of(w)).unwrap(),
        );
        assert_eq!(back.economy().carrier_of(w2), Some(boat2));
        assert_eq!(back.economy().ship_room(boat2), 1);
    }
}
