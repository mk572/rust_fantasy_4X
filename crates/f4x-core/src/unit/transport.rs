//! Ships carrying land units.
//!
//! A ship's `Transport` stat is how many land units it holds. Land units cannot enter the
//! sea alone: they board by moving onto a sea square where a friendly ship has room, or by
//! moving with a ship they share a square (or city) with. Passengers ride at no cost and
//! follow the ship; stepping from a ship onto land is an ordinary move and unloads them. A
//! passenger left at sea without a ship drowns.

use crate::city::economy::Economy;
use crate::map::terrain::TerrainClass;
use crate::map::Position;
use crate::player::{PlayerId, MONSTERS};
use crate::rules::unit_flag::UnitFlag;
use crate::rules::{MovementType, Stat};
use crate::unit::movement::MoveError;
use crate::world::serial::Serial;
use f4x_def::half_int::HalfInt;

use crate::world::{Event, World};

/// A ship let go for lack of gold with units still aboard: it belongs to nobody and
/// sails, a turn's worth at a time, for the nearest free shore, where it unloads and is released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marooned;

/// A ship parked at sea as a bridge (task 739): it stays on its square, and the walkers of its
/// owner are routed over it as over a land square, as many at a time as it has places. Set and
/// taken off by `Command::SetBridge`; a ship that sails is no bridge any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bridge;

/// The ship a land unit is aboard, by creation number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aboard(pub Serial);

impl World {
    /// Park `ship` at sea as a bridge, or take it off its post (`Command::SetBridge`). It has to
    /// be a ship and, to become a bridge, to stand at sea; its standing order is dropped, since
    /// a bridge does not move. A ship that sails later is no bridge any more (`move_stack`).
    pub fn set_bridge(&mut self, ship: hecs::Entity, on: bool) -> Result<(), MoveError> {
        if !self.economy().is_ship(ship) {
            return Err(MoveError::BadSelection);
        }
        if !on {
            let _ = self.ecs.remove_one::<Bridge>(ship);
            return Ok(());
        }
        let at = self
            .units()
            .unit_position(ship)
            .ok_or(MoveError::BadSelection)?;
        if !self.economy().is_sea(at) {
            return Err(MoveError::Impassable);
        }
        let _ = self.ecs.remove_one::<crate::unit::Order>(ship);
        self.put(ship, Bridge);
        Ok(())
    }
}

impl Economy<'_> {
    /// A unit that carries others: it sails and has room.
    pub fn is_ship(&self, unit: hecs::Entity) -> bool {
        let units = self.units();
        units.unit_stat(unit, Stat::Transport) > 0
            && units
                .unit_def(unit)
                .is_some_and(|d| d.movement == MovementType::Sailing)
    }

    /// Whether the unit is a walker: it walks by nature and nothing it carries (an item) or has
    /// cast on it (an enchantment) lifts it off the ground: no flight (24), no Wind Walking (74),
    /// no Water Walking (71), no Non-Corporeal (49). Only a walker is fetched by a ship or counted
    /// on to walk the way a route plans; the abilities are the unit's own, its enchantments' and
    /// its items' (`Units::unit_abilities`). The unit alone: [`Economy::is_walker_in_stack`] adds
    /// the stack it stands in.
    pub fn is_walker(&self, unit: hecs::Entity) -> bool {
        let units = self.units();
        units
            .unit_def(unit)
            .is_some_and(|d| d.movement == MovementType::Walking)
            && ![
                UnitFlag::Flies,
                UnitFlag::NonCorporeal,
                UnitFlag::WaterWalking,
                UnitFlag::WindWalking,
            ]
            .iter()
            .any(|&f| units.flags_of(&units.unit_abilities(unit)).contains(f))
    }

    /// [`Economy::is_walker`] for a unit in the stack it stands in: a stack with a Wind Walker is
    /// carried through the air, so its units are no walkers while they stand together. A
    /// question for the moment, since the stack changes when a unit steps away: not one to keep
    /// as a fact about the unit.
    pub fn is_walker_in_stack(&self, unit: hecs::Entity) -> bool {
        self.is_walker(unit)
            && self
                .units()
                .unit_position(unit)
                .is_none_or(|at| !self.stack_flies(&self.army_at(at)))
    }

    /// Whether the unit cannot cross the sea by itself: it walks and neither flies nor is
    /// carried through the air nor is non-corporeal nor walks on water.
    pub fn needs_ship(&self, unit: hecs::Entity, stack_flies: bool) -> bool {
        let units = self.units();
        !stack_flies
            && units.unit_def(unit).is_some_and(|d| {
                d.movement == MovementType::Walking
                    && ![
                        UnitFlag::Flies,
                        UnitFlag::NonCorporeal,
                        UnitFlag::WaterWalking,
                    ]
                    .iter()
                    .any(|&f| units.flags_of(&units.unit_abilities(unit)).contains(f))
            })
    }

    /// The ship `unit` is aboard, if it still exists.
    pub fn carrier_of(&self, unit: hecs::Entity) -> Option<hecs::Entity> {
        let serial = self.ecs.get::<&Aboard>(unit).ok()?.0;
        self.units().entity_of_serial(serial)
    }

    /// Units aboard `ship`.
    pub fn passengers_of(&self, ship: hecs::Entity) -> Vec<hecs::Entity> {
        let units = self.units();
        let serial = units.serial_of(ship);
        let mut out: Vec<_> = self
            .ecs
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
        self.ecs
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

    /// Every square where `owner`'s ships have room for the walkers of `units` all together: the
    /// squares a stack of walkers may step onto at sea. A step needs a place for each walker that
    /// needs a ship (`plan_step`), so a square whose ship has room for one will not do for a party
    /// of six: a path through it is a path the order cannot walk (`MoveError::NoRoom`).
    pub fn ship_room_squares_for(
        &self,
        owner: PlayerId,
        units: &[hecs::Entity],
    ) -> crate::fast::FastSet<Position> {
        let flies = self.stack_traits(units).flies;
        let walkers = units
            .iter()
            .filter(|&&u| self.needs_ship(u, flies))
            .count()
            .max(1) as u32;
        let Some(me) = crate::player::find_entity(self.ecs, owner) else {
            return Default::default();
        };
        let mut room: crate::fast::FastMap<Position, u32> = Default::default();
        for (u, (unit, p, o)) in self
            .ecs
            .query::<(&crate::unit::Unit, &Position, &crate::player::Owner)>()
            .iter()
        {
            if o.0 == me && self.content.rules.units.get(unit.0).movement == MovementType::Sailing {
                *room.entry(*p).or_default() += self.ship_room(u);
            }
        }
        room.into_iter()
            .filter(|&(_, n)| n >= walkers)
            .map(|(p, _)| p)
            .collect()
    }

    /// Whether the ship is parked as a bridge (`Bridge`).
    pub fn is_bridge(&self, ship: hecs::Entity) -> bool {
        self.ecs.get::<&Bridge>(ship).is_ok()
    }

    /// The squares of `owner` where a ship is a bridge (`Bridge`).
    pub fn bridge_squares(&self, owner: PlayerId) -> crate::fast::FastSet<Position> {
        let units = self.units();
        self.ecs
            .query::<(&crate::unit::Unit, &Position, &Bridge)>()
            .iter()
            .filter(|&(u, _)| units.unit_owner(u) == Some(owner) && self.is_ship(u))
            .map(|(_, (_, p, _))| *p)
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

    /// Ships standing where their movement no longer lets them be: on land, with no flight or
    /// non-corporeality (their own or their stack's) and not in a city of their owner.
    pub(crate) fn stranded_ships(&self) -> Vec<(PlayerId, hecs::Entity)> {
        let units = self.units();
        let mut lost = Vec::new();
        for unit in units.all_units() {
            let (Some(at), Some(owner)) = (units.unit_position(unit), units.unit_owner(unit))
            else {
                continue;
            };
            let sails = units
                .unit_def(unit)
                .is_some_and(|d| d.movement == MovementType::Sailing);
            if !sails || self.is_sea(at) {
                continue;
            }
            let stack: Vec<hecs::Entity> = units
                .units_at(at)
                .into_iter()
                .filter(|&u| units.unit_owner(u) == Some(owner))
                .collect();
            let traits = self.stack_traits(&stack);
            let allowed = self
                .step_profile(unit)
                .is_ok_and(|p| self.step_entry(&p, at, traits).is_ok());
            if !allowed {
                lost.push((owner, unit));
            }
        }
        lost
    }

    /// Land units at sea with no ship under them: they are lost (`World::drown_stranded`).
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
            // A Wind Walker of the stack carries the others through the air.
            let stack: Vec<hecs::Entity> = units
                .units_at(at)
                .into_iter()
                .filter(|&u| units.unit_owner(u) == Some(owner))
                .collect();
            if self.stack_flies(&stack) {
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
    ecs: &mut hecs::World,
    map: Option<&crate::map::Map>,
    content: &f4x_def::content::Content,
    stack: &[hecs::Entity],
    to: Position,
    stack_flies: bool,
) {
    fn economy<'a>(
        ecs: &'a hecs::World,
        map: Option<&'a crate::map::Map>,
        content: &'a f4x_def::content::Content,
    ) -> Economy<'a> {
        Economy::new(ecs, map, content)
    }
    let e = economy(ecs, map, content);
    let (to_sea, owner) = (e.is_sea(to), e.units().unit_owner(stack[0]));
    let Some(owner) = owner else {
        return;
    };
    for &u in stack {
        let board = {
            let e = economy(ecs, map, content);
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
                crate::world::put_in(ecs, u, aboard);
            }
            None => {
                crate::world::strip_in::<Aboard>(ecs, u);
            }
        }
    }
}

impl World {
    /// Every marooned ship sails on towards the nearest shore with room for its passengers; the
    /// ones that arrive land them and are released. A ship with nowhere to land loses them.
    pub(crate) fn sail_marooned(&mut self, player: crate::player::PlayerId) -> Vec<Event> {
        let mut ships: Vec<hecs::Entity> = self
            .ecs
            .query::<&Marooned>()
            .iter()
            .map(|(e, _)| e)
            .filter(|&e| self.units().unit_owner(e) == Some(player))
            .collect();
        self.units().sort_by_serial(&mut ships);
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
                    .unwrap_or(HalfInt::from_int(1));
                if cost > left {
                    break;
                }
                left -= cost;
                at = next;
                self.put(ship, at);
                for &p in &passengers {
                    self.put(p, at);
                }
            }
            if self.economy().next_to(at, landing) {
                for &p in &passengers {
                    self.strip::<Aboard>(p);
                    self.put(p, landing);
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
        if there + passengers.len() > self.tile_limit(land) {
            return false;
        }
        let traits = crate::unit::movement::StackTraits::default();
        passengers
            .iter()
            .all(|&p| e.step_cost(p, from, land, traits).is_ok())
    }

    /// The passengers of `player`'s ships that lie in a city are put ashore: a unit in a port is in
    /// its city, not aboard. Called at the turn start, so that a ship that sailed on through a port
    /// kept its load, and one that stopped there does not keep a garrison "aboard" for hundreds of
    /// turns, which left it out of every ferry job (seed 20134 of the soak).
    pub(crate) fn unload_in_ports(&mut self, player: crate::player::PlayerId) {
        let aboard: Vec<hecs::Entity> = self
            .ecs
            .query::<&Aboard>()
            .iter()
            .map(|(e, _)| e)
            .filter(|&e| self.units().unit_owner(e) == Some(player))
            .collect();
        for unit in aboard {
            let in_port = self
                .units()
                .unit_position(unit)
                .is_some_and(|p| self.economy().city_at(p).is_some());
            if in_port && self.economy().carrier_of(unit).is_some() {
                self.strip::<Aboard>(unit);
            }
        }
    }

    /// Ships on land that lost their way there are destroyed (their passengers stay on the
    /// square); land units at sea with no ship under them are lost, as the units of a defeated army are
    /// when they flee: where free land lies beside them they scramble ashore, half of the troops
    /// and three in four of the heroes making it (`overland_aftermath`'s odds); anywhere else
    /// they drown. Fliers and swimmers never need a ship, so they never come here.
    pub(crate) fn drown_stranded(&mut self, only: Option<crate::player::PlayerId>) -> Vec<Event> {
        let mut events = Vec::new();
        // A ship on land that can no longer be there is destroyed, and the units it carried
        // stand on the square.
        for (owner, ship) in self.economy().stranded_ships() {
            if only.is_some_and(|p| p != owner) {
                continue;
            }
            let Some(at) = self.units().unit_position(ship) else {
                continue;
            };
            let passengers = self.economy().passengers_of(ship);
            for &p in &passengers {
                self.strip::<Aboard>(p);
            }
            self.remove_unit(ship, crate::hero::Fate::Slain);
            events.push(Event::ShipStranded {
                player: owner,
                units: passengers.len() as u32,
                at,
            });
        }
        let mut drowned: Vec<(crate::player::PlayerId, u32)> = Vec::new();
        for (owner, unit) in self.economy().stranded() {
            if only.is_some_and(|p| p != owner) {
                continue;
            }
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
                self.strip::<Aboard>(unit);
                self.put(unit, land);
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
