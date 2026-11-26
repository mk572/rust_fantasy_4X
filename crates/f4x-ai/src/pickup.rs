//! Units that cannot walk to their target (`Crossing::Sea`: the target is on this plane and no
//! known land route leads to it) wait for a ship, wherever their job (a reinforcement, a settler,
//! a march) sent them. Here the ship comes for them (task 647).
//!
//! The units are grouped by destination and by the land they can walk over together; groups
//! bound for the same landmass, near each other, share a ship (the first group, settlers first,
//! sets the drop; the others walk on from there). For each
//! group one boarding tile is chosen, so that the units walk to the same beach: the coastal tile
//! (with a sea square beside it that a free ship can reach) where the last unit and the ship
//! arrive soonest, with a unit whose walk is far longer than the others' left for the next ship.
//! The units step aboard with an ordinary move, the ship sails to the sea square beside the
//! destination (or into the port, when the destination is a coastal city), the units step ashore
//! and their tag is cleared: the dispatcher sends them on to their target (`dispatch::revalidate`).
//!
//! The units tied to a ship hold their own role; `Assignment::with` is the ship and
//! `Assignment::boarding` the tile. A ship sent for them holds `TransportSpace` (the sea square as
//! its target, room for them), `TransportReady` when they fill it, and `Transporting` once it
//! sails. [`voyages`] reads them back.

use crate::roles::{Crossing, Role};
use crate::view::is_ship;
use crate::Ai;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event, World};
use f4x_master::Game;

/// Candidate boarding tiles that get a real path search, per group.
const CANDIDATES: usize = 12;
/// How many turns longer than the best a unit may walk and still be taken aboard.
const STRAGGLER_TURNS: u32 = 5;
/// How far apart (squares) two destinations on one landmass may be for their units to share a
/// ship: the units of the later group walk on from the first group's drop.
const MERGE_RANGE: u32 = 10;
/// How far from a unit a boarding tile is looked for, in squares.
const SHORE_RANGE: i32 = 12;

/// A ship on its way to fetch units waiting at `tile`, as the registry gives it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pickup {
    pub ship: Serial,
    /// The shore tile where the units board.
    pub tile: Position,
    /// The sea square the ship is sent to: beside the tile while it fetches, then beside the
    /// destination.
    pub sea: Position,
    /// Where the units want to go.
    pub dest: Position,
    /// The units it came for.
    pub group: Vec<Serial>,
}

/// The ships on their way to fetch units, with the units each came for.
pub fn voyages(ai: &Ai) -> Vec<Pickup> {
    let mut ships = ai.roles.members(Role::TransportSpace);
    ships.extend(ai.roles.members(Role::TransportReady));
    ships.extend(ai.roles.members(Role::Transporting));
    ships.sort();
    ships
        .into_iter()
        .filter_map(|ship| {
            let sea = ai.roles.get(ship).and_then(|a| a.to)?;
            let group = tied_to(ai, ship);
            let first = *group.first()?;
            Some(Pickup {
                ship,
                tile: ai.roles.boarding_of(first)?,
                sea,
                dest: ai.roles.destination_of(
                    group
                        .iter()
                        .copied()
                        .find(|&s| ai.roles.is(s, Role::Settle))
                        .unwrap_or(first),
                )?,
                group,
            })
        })
        .collect()
}

/// The units a ship comes for, in serial order.
fn tied_to(ai: &Ai, ship: Serial) -> Vec<Serial> {
    ai.roles
        .tied_with(ship)
        .into_iter()
        .filter(|&s| ai.roles.boarding_of(s).is_some())
        .collect()
}

/// The turns the unit takes to get to `to` (0 when it is there), by the game's own path.
fn turns(view: &PlayerView, unit: hecs::Entity, to: Position) -> Option<u32> {
    if view.unit_position(unit) == Some(to) {
        return Some(0);
    }
    let steps = view.path(&[unit], to)?.len() as u32;
    let moves = view.unit_stat(unit, f4x_def::rules::Stat::Moves).max(1) as u32;
    Some(steps.div_ceil(moves))
}

/// Whether units bound for `b` may share a ship with units bound for `a`: the squares lie on one
/// landmass, at most `range` apart.
fn rides_along(view: &PlayerView, a: Position, b: Position, range: u32) -> bool {
    view.distance(a, b).is_some_and(|d| d <= range) && same_landmass(view, a, b)
}

/// Whether `a` and `b` are on one landmass: the known land squares join them.
fn same_landmass(view: &PlayerView, a: Position, b: Position) -> bool {
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<Position> = HashSet::from([a]);
    let mut queue = VecDeque::from([a]);
    while let Some(here) = queue.pop_front() {
        if here == b {
            return true;
        }
        if seen.len() > 1500 {
            break;
        }
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let Some(next) = view.shift(here, dx, dy) else {
                    continue;
                };
                if view
                    .terrain_class(next)
                    .is_some_and(|t| t != TerrainClass::Ocean)
                    && seen.insert(next)
                {
                    queue.push_back(next);
                }
            }
        }
    }
    false
}

/// A sea square beside `land` that nobody hostile holds.
fn sea_beside(view: &PlayerView, land: Position) -> Option<Position> {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let Some(sea) = view.shift(land, dx, dy) else {
                continue;
            };
            if view.terrain_class(sea) == Some(TerrainClass::Ocean)
                && view.hostile_at(sea).is_none()
            {
                return Some(sea);
            }
        }
    }
    None
}

/// Where a ship drops units bound for `dest`: the city itself when it is a port the ship can
/// sail into, else a sea square beside it.
fn drop_for(view: &PlayerView, ship: hecs::Entity, dest: Position) -> Option<Position> {
    if view.city_at(dest).is_some()
        && (view.unit_position(ship) == Some(dest) || view.can_reach(&[ship], dest))
    {
        return Some(dest);
    }
    // A destination inland (a tower, a far city) is reached from the nearest coast: the units
    // walk the rest.
    let sea =
        sea_beside(view, dest).or_else(|| crate::naval::landing(view, dest).map(|(_, sea)| sea))?;
    (view.unit_position(ship) == Some(sea) || view.can_reach(&[ship], sea)).then_some(sea)
}

/// What a boarding plan costs: the later of the last unit and the ship, the walking turns in all,
/// the sail to the destination, and the tile's place for a stable order.
type PlanCost = (u32, u32, u32, u8, u32, u32);

/// A boarding plan for a group: the tile, the sea square beside it, the ship and the units that
/// go aboard.
#[derive(Debug, Clone)]
struct Plan {
    tile: Position,
    sea: Position,
    ship: hecs::Entity,
    units: Vec<hecs::Entity>,
}

/// The coastal tiles near the units that a ship could lie beside, the best few by the squares the
/// units must cover to get there.
fn candidates(view: &PlayerView, units: &[hecs::Entity]) -> Vec<Position> {
    let mut found: Vec<Position> = Vec::new();
    for &u in units {
        let Some(at) = view.unit_position(u) else {
            continue;
        };
        for dy in -SHORE_RANGE..=SHORE_RANGE {
            for dx in -SHORE_RANGE..=SHORE_RANGE {
                let Some(p) = view.shift(at, dx, dy) else {
                    continue;
                };
                if found.contains(&p)
                    || !view
                        .terrain_class(p)
                        .is_some_and(|t| t != TerrainClass::Ocean)
                    || sea_beside(view, p).is_none()
                {
                    continue;
                }
                found.push(p);
            }
        }
    }
    let spread = |p: &Position| -> u32 {
        units
            .iter()
            .filter_map(|&u| view.unit_position(u))
            .map(|q| view.distance(q, *p).unwrap_or(1000))
            .sum()
    };
    found.sort_by_key(|p| (spread(p), p.y, p.x));
    found.truncate(CANDIDATES);
    found
}

/// The best boarding plan for the units bound for `dest`, with the ships free to take them (each
/// with its room): the tile and ship where the last unit or the ship arrives soonest, then the
/// fewest walking turns in all, then the shortest sail to the destination.
fn plan_group(
    view: &PlayerView,
    units: &[hecs::Entity],
    lead: &[hecs::Entity],
    dest: Position,
    ships: &[(hecs::Entity, usize)],
) -> Option<Plan> {
    let mut best: Option<(PlanCost, Plan)> = None;
    for tile in candidates(view, units) {
        let Some(sea) = sea_beside(view, tile) else {
            continue;
        };
        let mut walks: Vec<(u32, hecs::Entity)> = units
            .iter()
            .filter_map(|&u| Some((turns(view, u, tile)?, u)))
            .collect();
        // The lead group's units come first, whatever they walk: they are why the ship sails.
        walks.sort_by_key(|&(t, u)| (!lead.contains(&u), t, view.serial_of(u)));
        let Some(fastest) = walks.iter().map(|&(t, _)| t).min() else {
            continue;
        };
        for &(ship, room) in ships {
            if view
                .unit_position(ship)
                .is_none_or(|p| p.plane != tile.plane)
            {
                continue;
            }
            let (Some(arrive), Some(drop)) = (turns(view, ship, sea), drop_for(view, ship, dest))
            else {
                continue;
            };
            let chosen: Vec<(u32, hecs::Entity)> = walks
                .iter()
                .copied()
                .filter(|&(t, _)| t <= fastest + STRAGGLER_TURNS)
                .take(room.max(1))
                .collect();
            let last = chosen.iter().map(|&(t, _)| t).max().unwrap_or(0);
            let total: u32 = chosen.iter().map(|&(t, _)| t).sum();
            // The longer the group waits for the slowest unit or the ship, the worse; a plan that
            // takes more units is better than one that leaves them for another ship.
            let key = (
                last.max(arrive),
                total,
                view.distance(sea, drop).unwrap_or(u32::MAX),
                tile.plane as u8,
                tile.y,
                tile.x,
            );
            let better = best.as_ref().is_none_or(|(k, p)| {
                chosen.len() > p.units.len() || (chosen.len() == p.units.len() && key < *k)
            });
            if better {
                best = Some((
                    key,
                    Plan {
                        tile,
                        sea,
                        ship,
                        units: chosen.into_iter().map(|(_, u)| u).collect(),
                    },
                ));
            }
        }
    }
    best.map(|(_, plan)| plan)
}

/// One turn of the waiting units and the ships that fetch them.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    tidy_unit_registry(ai, game);
    if ai.roles.waiting_for_ship().is_empty() && voyages(ai).is_empty() {
        return;
    }
    call_off_a_scout_ship(ai, game, events);
    adopt_passengers(ai, game);
    assign_ships(ai, game);
    walk_to_shore(ai, game, events);
    for pickup in voyages(ai) {
        advance(ai, game, &pickup, events);
    }
}

/// Carrying units comes before scouting: when units wait for a ship and every ship is scouting
/// (none is free and none is already on its way to them), the first scouting ship with nobody
/// aboard is called off and takes the job. The search by sea goes on when a ship is free again.
fn call_off_a_scout_ship(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    if ai.roles.waiting_for_ship().is_empty() || !voyages(ai).is_empty() {
        return;
    }
    let me = ai.player;
    let ship = {
        let view = PlayerView::new(game, me);
        let mut ships: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .collect();
        view.sort_by_serial(&mut ships);
        // A free ship takes the job as it is.
        if ships
            .iter()
            .any(|&u| !ai.roles.has_role(view.serial_of(u)) && view.passengers_of(u).is_empty())
        {
            return;
        }
        ships.into_iter().find(|&u| {
            ai.roles.is(view.serial_of(u), Role::Scout) && view.passengers_of(u).is_empty()
        })
    };
    let Some(ship) = ship else {
        return;
    };
    let serial = PlayerView::new(game, me).serial_of(ship);
    ai.roles.release(serial, Role::Scout);
    ai.order(game, Command::ClearOrders { units: vec![ship] }, events);
}

/// Whether a walker at its square needs a ship to reach `to`: no way on land, the sea cuts it off.
/// A way blocked by full squares only, another plane or a hostile square is no sea.
fn needs_the_sea(world: &World, unit: hecs::Entity, to: Position) -> bool {
    use f4x_core::unit::pathing::failure::NoPath;
    world.find_path(&[unit], to).is_none()
        && matches!(world.why_no_path(&[unit], to), NoPath::CutOff { .. })
}

/// The sweep of the ship ties in the unit registry, at the start of the turn (before any job has
/// moved a unit) and again before the pickups.
///
/// **The sea tag** (`Crossing::Sea`: the unit waits for a ship) survives only for a walker
/// (`PlayerView::is_walker`, the unit alone: a stack with a Wind Walker is left out on purpose, it
/// would give false alarms) that either rides a ship or has a destination that needs the sea. Any
/// other unit with the tag loses it, together with its place on a passenger list: the job that
/// owns the unit decides again (walk, a new tag, a new job).
///
/// **A unit booked on a ship** (a boarding tile and a ship) must also be where a passenger is:
/// aboard that ship, or on its way to the boarding tile (standing on it, under an order, or
/// waiting because the tile is full), and the ship must exist. One that is not is taken off the
/// list but keeps its tag, if the tag survives, and is booked again.
///
/// An ended plan (no destination, no tag) is dropped without a word. Any other fault of a booked
/// unit goes into the refused-order log (`BadPassenger`), so a soak shows the job that left a
/// passenger behind (the strike that walked one into a city) instead of a ship that waits for it
/// for hundreds of turns. Last, the ships that have nobody left to fetch are released.
pub(crate) fn tidy_unit_registry(ai: &mut Ai, world: &World) {
    // (unit, whether to clear its sea tag, what to say about it)
    let mut broken: Vec<(Serial, bool, Option<String>)> = Vec::new();
    {
        let view = PlayerView::new(world, ai.player);
        let mine = view.my_units();
        for &u in &mine {
            let s = view.serial_of(u);
            let tagged = ai.roles.crossing_of(s) == Some(Crossing::Sea);
            let booking = ai.roles.boarding_of(s).zip(ai.roles.with_of(s));
            if !tagged && booking.is_none() {
                continue;
            }
            let to = ai.roles.destination_of(s);
            // The plan is over: no tag, or no target to cross to and no ship under the unit.
            let on_ship = world.economy().carrier_of(u).is_some();
            if to.is_none() && !on_ship {
                broken.push((s, true, None));
                continue;
            }
            let mut faults: Vec<String> = Vec::new();
            // The tag: a walker that rides a ship or has a target over the sea.
            let walker = view.is_walker(u);
            let sea = on_ship || to.is_some_and(|t| needs_the_sea(world, u, t));
            let tag_dies = !tagged || !walker || !sea;
            if !walker {
                faults.push("it is no walker (it flies, walks on water or the like)".to_string());
            } else if !sea {
                faults.push(format!("its target {to:?} needs no sea crossing"));
            } else if !tagged {
                faults.push("it has no sea tag".to_string());
            }
            // The booking: on its ship, or on the way to board it.
            if let Some((tile, ship_serial)) = booking {
                match mine
                    .iter()
                    .copied()
                    .find(|&x| view.serial_of(x) == ship_serial)
                {
                    None => faults.push(format!("its ship {ship_serial:?} is gone")),
                    Some(ship) => {
                        let pos = view.unit_position(u);
                        let aboard = pos.is_some() && pos == view.unit_position(ship);
                        let on_its_way = pos == Some(tile)
                            || view.unit_has_order(u)
                            || !Ai::room_at(world, tile, 1);
                        if !aboard && !on_its_way {
                            faults.push(format!(
                                "not aboard ship {ship_serial:?} and not on the way to its boarding tile {tile:?} (at {pos:?})"
                            ));
                        }
                    }
                }
            }
            if faults.is_empty() {
                continue;
            }
            // Only a booked unit is reported: a tag that goes stale with the world is no news.
            let said = booking.is_some().then(|| faults.join("; "));
            broken.push((s, tag_dies, said));
        }
    }
    for (s, clear_tag, why) in broken {
        if let Some(why) = why {
            ai.note_failure(
                world,
                format!("passenger {s:?} booked on a ship"),
                crate::with_detail("BadPassenger".to_string(), Some(why)),
            );
        }
        ai.roles.unboard(s);
        if clear_tag {
            ai.roles.set_crossing(s, None);
        }
    }
    let mut planned = ai.roles.members(Role::TransportSpace);
    planned.extend(ai.roles.members(Role::TransportReady));
    planned.extend(ai.roles.members(Role::Transporting));
    for ship in planned {
        // A ship of the campaign has no party; one of a pickup has, and it needs its units.
        if ai.roles.party_of(ship).is_some() && tied_to(ai, ship).is_empty() {
            ai.roles.release_any(ship);
        }
    }
}

/// The units tied to a ship that stand elsewhere and have no order walk to their boarding tile.
fn walk_to_shore(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let mut sends: Vec<(Position, Position, Vec<hecs::Entity>)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        for u in view.my_units() {
            let serial = view.serial_of(u);
            let Some(tile) = ai.roles.boarding_of(serial) else {
                continue;
            };
            let Some(pos) = view.unit_position(u) else {
                continue;
            };
            if pos == tile || view.unit_has_order(u) || is_aboard(&view, u) {
                continue;
            }
            match sends.iter_mut().find(|(p, t, _)| *p == pos && *t == tile) {
                Some((_, _, list)) => list.push(u),
                None => sends.push((pos, tile, vec![u])),
            }
        }
    }
    sends.sort_by_key(|(p, t, _)| (p.plane as u8, p.y, p.x, t.y, t.x));
    for (_, tile, units) in sends {
        // A boarding tile with no room for them is not asked: they wait where they are.
        if !Ai::room_at(game, tile, units.len()) {
            continue;
        }
        // No way to the tile. Through full squares only: the traffic moves on, they wait. Else the
        // units were grouped by what one of them reaches and cannot walk there: they are let off
        // the plan, which is made again, and the log says why.
        if game.find_path(&units, tile).is_none() {
            let why = game.why_no_path(&units, tile);
            if why == f4x_core::unit::pathing::failure::NoPath::OnlyThroughFullSquares {
                continue;
            }
            let serials: Vec<_> = {
                let view = PlayerView::new(game, me);
                units.iter().map(|&u| view.serial_of(u)).collect()
            };
            ai.note_failure(
                game,
                format!("units {serials:?} cannot walk to their boarding tile {tile:?}"),
                format!("NoPath({}){}{why}", why.name(), crate::DETAIL_SEPARATOR),
            );
            for s in serials {
                ai.roles.unboard(s);
            }
            continue;
        }
        let _ = ai.send(game, Command::GoTo { units, to: tile }, events);
    }
}

/// Whether the unit rides a ship at its square.
fn is_aboard(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_position(unit).is_some_and(|p| {
        view.my_units().into_iter().any(|s| {
            is_ship(view, s)
                && view.unit_position(s) == Some(p)
                && view.passengers_of(s).contains(&unit)
        })
    })
}

/// A ship with no job that carries units waiting for a ship, at sea: it is theirs. They went
/// aboard on their own (a stack that walked onto a ship waiting at the shore) or the voyage that
/// carried them was ended; the pickup left the ship out of the free ones (it has passengers) and
/// booked the units on another ship, far away, for hundreds of turns. The ship takes them where
/// they want to go: a voyage whose units are all aboard already, with the shore beside the ship
/// as its boarding tile.
fn adopt_passengers(ai: &mut Ai, game: &mut Game) {
    let me = ai.player;
    let mut plans: Vec<(Serial, Position, Position, Vec<Serial>)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        let waiting = ai.roles.waiting_for_ship();
        let mut ships: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u) && !ai.roles.has_role(view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut ships);
        for ship in ships {
            let Some(at) = view.unit_position(ship) else {
                continue;
            };
            let passengers = view.passengers_of(ship);
            if passengers.is_empty() || view.terrain_class(at) != Some(TerrainClass::Ocean) {
                continue;
            }
            let serials: Vec<Serial> = passengers.iter().map(|&u| view.serial_of(u)).collect();
            // Every passenger waits for a ship, with a target of its own.
            if !serials.iter().all(|s| waiting.iter().any(|(w, _)| w == s)) {
                continue;
            }
            let Some(shore) = crate::military::neighbours(&view, at)
                .into_iter()
                .find(|&p| {
                    view.terrain_class(p)
                        .is_some_and(|t| t != TerrainClass::Ocean)
                })
            else {
                continue;
            };
            plans.push((view.serial_of(ship), at, shore, serials));
        }
    }
    for (ship, at, shore, serials) in plans {
        let capacity = {
            let view = PlayerView::new(game, me);
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == ship)
                .map_or(1, |u| game.economy().ship_room(u) as usize + serials.len())
        };
        if ai
            .roles
            .plan_ship(ship, serials.len(), capacity, game.round(), at)
        {
            let party = ai.roles.new_party();
            let role = ai.roles.role_of(ship).unwrap_or(Role::TransportSpace);
            ai.roles.group(&[ship], role, party, None);
            for s in serials {
                ai.roles.board_at(s, ship, shore);
            }
        }
    }
}

/// Each group of waiting units with no ship coming gets a boarding plan and a free ship.
fn assign_ships(ai: &mut Ai, game: &mut Game) {
    let me = ai.player;
    let mut plans: Vec<(Plan, Position)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        let mut free: Vec<(hecs::Entity, usize)> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .filter(|&u| {
                let s = view.serial_of(u);
                !ai.roles.has_role(s) && view.passengers_of(u).is_empty()
            })
            .map(|u| (u, game.economy().ship_room(u) as usize))
            .collect();
        free.sort_by_key(|&(u, _)| view.serial_of(u));
        let waiting: Vec<(hecs::Entity, Position)> = view
            .my_units()
            .into_iter()
            .filter_map(|u| {
                let s = view.serial_of(u);
                if !view.is_walker(u) {
                    return None;
                }
                let to = ai
                    .roles
                    .waiting_for_ship()
                    .into_iter()
                    .find(|&(w, _)| w == s)?
                    .1;
                Some((u, to))
            })
            .collect();
        // By destination, then by the land the units can walk over together.
        let mut dests: Vec<Position> = waiting.iter().map(|&(_, d)| d).collect();
        dests.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        dests.dedup();
        let mut groups: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        for dest in dests {
            let mut left: Vec<hecs::Entity> = waiting
                .iter()
                .filter(|&&(_, d)| d == dest)
                .map(|&(u, _)| u)
                .collect();
            while let Some(&first) = left.first() {
                let (region, rest): (Vec<_>, Vec<_>) = left.iter().partition(|&&u| {
                    u == first
                        || view.unit_position(u).is_some_and(|p| {
                            view.unit_position(first) == Some(p) || view.can_reach(&[first], p)
                        })
                });
                left = rest;
                groups.push((dest, region));
            }
        }
        // Settlers first: a city founded late costs more than a soldier delivered late.
        let settler = |u: &hecs::Entity| view.unit_is_settler(*u);
        groups.sort_by_key(|(_, units)| !units.iter().any(settler));
        let mut groups = groups
            .into_iter()
            .collect::<std::collections::VecDeque<_>>();
        while let Some((dest, lead)) = groups.pop_front() {
            // Later groups on the same landmass, near, and standing on the same ground, ride along.
            let mut region = lead.clone();
            let mut rest = std::collections::VecDeque::new();
            while let Some((d, units)) = groups.pop_front() {
                let together = rides_along(&view, dest, d, MERGE_RANGE)
                    && view.unit_position(units[0]).is_some_and(|p| {
                        view.unit_position(lead[0]) == Some(p) || view.can_reach(&[lead[0]], p)
                    });
                if together {
                    region.extend(units);
                } else {
                    rest.push_back((d, units));
                }
            }
            groups = rest;
            let Some(plan) = plan_group(&view, &region, &lead, dest, &free) else {
                continue;
            };
            free.retain(|&(s, _)| s != plan.ship);
            plans.push((plan, dest));
        }
    }
    for (plan, _) in plans {
        let (ship, units) = {
            let view = PlayerView::new(game, me);
            (
                view.serial_of(plan.ship),
                plan.units
                    .iter()
                    .map(|&u| view.serial_of(u))
                    .collect::<Vec<_>>(),
            )
        };
        let room = (game.economy().ship_room(plan.ship) as usize).max(1);
        let turn = game.round();
        if ai.roles.plan_ship(ship, units.len(), room, turn, plan.sea) {
            let party = ai.roles.new_party();
            let role = ai.roles.role_of(ship).unwrap_or(Role::TransportSpace);
            ai.roles.group(&[ship], role, party, None);
            for s in units {
                ai.roles.board_at(s, ship, plan.tile);
            }
        }
    }
}

/// The units of the group, aboard at `at`, step onto the land square beside it that is nearest to
/// `dest`. Whether they were sent.
fn step_ashore(
    ai: &Ai,
    game: &mut Game,
    group: &[Serial],
    at: Position,
    dest: Position,
    events: &mut Vec<Event>,
) -> bool {
    let (units, shore) = {
        let view = PlayerView::new(game, ai.player);
        let units: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| group.contains(&view.serial_of(u)) && view.unit_position(u) == Some(at))
            .collect();
        let shore = (-1i32..=1)
            .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
            .filter_map(|(dx, dy)| view.shift(at, dx, dy))
            .filter(|&p| {
                view.terrain_class(p)
                    .is_some_and(|t| t != TerrainClass::Ocean)
                    && view.hostile_at(p).is_none()
            })
            .min_by_key(|&p| (view.distance(p, dest).unwrap_or(u32::MAX), p.y, p.x));
        (units, shore)
    };
    let Some(shore) = shore.filter(|_| !units.is_empty()) else {
        return false;
    };
    ai.send(game, Command::GoTo { units, to: shore }, events)
        .is_ok()
}

/// One step of a voyage: sail to the sea square, take the units aboard, sail to the destination,
/// put them ashore.
fn advance(ai: &mut Ai, game: &mut Game, pickup: &Pickup, events: &mut Vec<Event>) {
    let me = ai.player;
    let (ship, ship_pos, aboard, total, on_land, drop, idle, free) = {
        let view = PlayerView::new(game, me);
        let Some(ship) = view
            .my_units()
            .into_iter()
            .find(|&u| view.serial_of(u) == pickup.ship)
        else {
            return;
        };
        let ship_pos = view.unit_position(ship);
        let group: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| pickup.group.contains(&view.serial_of(u)))
            .collect();
        let aboard: Vec<hecs::Entity> = group
            .iter()
            .copied()
            .filter(|&u| view.unit_position(u) == ship_pos && ship_pos != Some(pickup.tile))
            .collect();
        let on_land: Vec<hecs::Entity> = group
            .iter()
            .copied()
            .filter(|&u| view.unit_position(u) == Some(pickup.tile))
            .collect();
        (
            ship,
            ship_pos,
            aboard,
            group.len(),
            on_land,
            drop_for(&view, ship, pickup.dest),
            !view.unit_has_order(ship),
            game.economy().ship_room(ship) as usize,
        )
    };
    let Some(ship_pos) = ship_pos else {
        return;
    };
    let everyone = aboard.len() == total;
    if everyone {
        let Some(drop) = drop else {
            return;
        };
        if ship_pos == drop {
            let in_port = PlayerView::new(game, me).city_at(ship_pos).is_some();
            // In a port the units are in their city; at sea they step ashore first.
            if in_port || step_ashore(ai, game, &pickup.group, ship_pos, pickup.dest, events) {
                for s in &pickup.group {
                    ai.roles.set_crossing(*s, None);
                    ai.roles.unboard(*s);
                    // A landing party is free again once ashore: the war job judges it where it
                    // stands (`war::march_landed`). The other roles walk on to their target.
                    ai.roles.release(*s, Role::March);
                }
                ai.roles.release_any(pickup.ship);
            }
            return;
        }
        if idle && Ai::room_at(game, drop, 1) {
            let sailed = ai
                .send(
                    game,
                    Command::GoTo {
                        units: vec![ship],
                        to: drop,
                    },
                    events,
                )
                .is_ok();
            if sailed {
                ai.roles.depart(pickup.ship, drop);
            }
        }
        return;
    }
    if ship_pos == pickup.sea {
        // The square holds the ship, what is aboard and whatever else stands there, up to the
        // limit of a square: no more units are sent than it has room for.
        let room = game
            .max_tile()
            .saturating_sub(game.economy().army_at(pickup.sea).len());
        let mut on_land = on_land;
        // The ship is full: a unit sent to it is refused ("target is water") every turn, so the
        // ship sails with what it has and the units left behind are let off the plan, which is
        // made again for them with another ship.
        if free == 0 && !aboard.is_empty() {
            let left: Vec<Serial> = {
                let view = PlayerView::new(game, me);
                on_land.iter().map(|&u| view.serial_of(u)).collect()
            };
            for s in left {
                ai.roles.unboard(s);
            }
            return;
        }
        on_land.truncate(room.min(free));
        if !on_land.is_empty() {
            let _ = ai.send(
                game,
                Command::GoTo {
                    units: on_land,
                    to: pickup.sea,
                },
                events,
            );
        }
        return;
    }
    if idle && Ai::room_at(game, pickup.sea, 1) {
        let _ = ai.send(
            game,
            Command::GoTo {
                units: vec![ship],
                to: pickup.sea,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0};
    use f4x_core::map::terrain::TerrainClass;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Send the units to `dest` with no way to walk there: their job's target and the tag the
    /// dispatcher puts on them.
    fn strand(ai: &mut Ai, w: &World, units: &[hecs::Entity], dest: Position) {
        let view = PlayerView::new(w, P0);
        for &u in units {
            let s = view.serial_of(u);
            let _ = ai.roles.claim(s, Role::Reinforce, w.turn, Some(dest));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
    }

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    fn shore(w: &World) -> Result<Position, Box<dyn std::error::Error>> {
        let view = PlayerView::new(w, P0);
        let (land, _) = crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
            .ok_or("a shore tile near the far city")?;
        Ok(land)
    }

    /// Land squares the unit can walk to, nearest to `near` first, at most `n`.
    fn island_squares(w: &World, unit: hecs::Entity, near: Position, n: usize) -> Vec<Position> {
        let view = PlayerView::new(w, P0);
        let mut found: Vec<Position> = Vec::new();
        for r in 0..30i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let Some(p) = view.shift(near, dx, dy) else {
                        continue;
                    };
                    if view
                        .terrain_class(p)
                        .is_some_and(|t| t != TerrainClass::Ocean)
                        && (p == near || view.can_reach(&[unit], p))
                    {
                        found.push(p);
                    }
                }
            }
        }
        found.truncate(n);
        found
    }

    fn run_turns(ai: &mut Ai, w: &mut Game, n: usize) {
        for _ in 0..n {
            run(ai, w, &mut Vec::new());
            w.play_round();
            w.balance_farmers(P0);
        }
    }

    /// Soldiers on the far island, bound for our port, with a free ship in port: the ship comes,
    /// takes them aboard and puts them ashore in the port, and their tag is cleared.
    #[test]
    fn units_that_need_a_ship_are_fetched_and_brought_to_their_destination() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, 3);
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, port());
        for _ in 0..60 {
            run_turns(&mut ai, &mut w, 1);
            let view = PlayerView::new(&w, P0);
            if soldiers
                .iter()
                .all(|&u| ai.roles.crossing_of(view.serial_of(u)).is_none())
            {
                break;
            }
        }
        let view = PlayerView::new(&w, P0);
        for &u in &soldiers {
            let s = view.serial_of(u);
            assert_eq!(ai.roles.crossing_of(s), None, "{:?}", ai.roles);
            assert_eq!(view.unit_position(u), Some(port()), "in the port");
            assert_eq!(ai.roles.boarding_of(s), None);
        }
        assert!(voyages(&ai).is_empty(), "{:?}", ai.roles);
        Ok(())
    }

    /// Units standing on different squares are all sent to one boarding tile, and it is a tile
    /// where the last of them arrives as soon as it can anywhere on the coast near them.
    #[test]
    fn units_on_different_squares_board_at_one_tile_where_the_last_arrives_soonest() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let first = spawn(&mut w, P0, land, 104 /* spearmen */, 1);
        let squares = island_squares(&w, first[0], land, 60);
        // Five units, spread over the squares near the shore.
        let spots: Vec<Position> = squares.iter().copied().step_by(8).take(4).collect();
        assert!(spots.len() >= 3, "land to scatter the units on: {spots:?}");
        let mut soldiers: Vec<hecs::Entity> = first;
        soldiers.extend(
            spots
                .iter()
                .flat_map(|&p| spawn(&mut w, P0, p, 104 /* spearmen */, 1)),
        );
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, port());
        assign_ships(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        let tiles: Vec<Position> = soldiers
            .iter()
            .filter_map(|&u| ai.roles.boarding_of(view.serial_of(u)))
            .collect();
        assert!(!tiles.is_empty(), "a ship was assigned");
        let tile = tiles[0];
        assert!(tiles.iter().all(|&t| t == tile), "one tile: {tiles:?}");
        // The plan's cost, the later of the last unit and the ship, is no worse at the chosen
        // tile than at any other coastal tile near them.
        let ship = view
            .my_units()
            .into_iter()
            .find(|&u| is_ship(&view, u))
            .ok_or("the ship")?;
        let boarded: Vec<hecs::Entity> = soldiers
            .iter()
            .copied()
            .filter(|&u| ai.roles.boarding_of(view.serial_of(u)).is_some())
            .collect();
        let cost = |t: Position| -> Option<u32> {
            let last = boarded
                .iter()
                .map(|&u| turns(&view, u, t))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .max()
                .unwrap_or(0);
            Some(last.max(turns(&view, ship, sea_beside(&view, t)?)?))
        };
        let chosen = cost(tile).ok_or("the chosen tile is reachable")?;
        for p in candidates(&view, &boarded) {
            if let Some(other) = cost(p) {
                assert!(chosen <= other, "{tile:?} {chosen} vs {p:?} {other}");
            }
        }
        Ok(())
    }

    /// Units that wait for a ship and are already aboard an unassigned ship at sea (they walked
    /// onto it, or the voyage that carried them was ended): the ship takes them to their
    /// destination, where before it stood for hundreds of turns (the refused-order log of seed 31:
    /// three soldiers booked on a ship that was far away, carried by an idle one).
    #[test]
    fn units_aboard_an_idle_ship_at_sea_are_carried_to_their_destination() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, 2);
        let sea = {
            let view = PlayerView::new(&w, P0);
            sea_beside(&view, land).ok_or("a sea square beside the shore")?
        };
        let ship = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        w.move_stack(&soldiers, sea).map_err(|e| format!("{e:?}"))?;
        assert_eq!(w.economy().carrier_of(soldiers[0]), Some(ship), "aboard");
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, port());
        for _ in 0..40 {
            run_turns(&mut ai, &mut w, 1);
            let view = PlayerView::new(&w, P0);
            if soldiers
                .iter()
                .all(|&u| ai.roles.crossing_of(view.serial_of(u)).is_none())
            {
                break;
            }
        }
        let view = PlayerView::new(&w, P0);
        for &u in &soldiers {
            assert_eq!(view.unit_position(u), Some(port()), "{:?}", ai.roles);
        }
        Ok(())
    }

    /// A group larger than the ship holds: the ship takes what fits and sails, and the unit left
    /// on the shore is let off the plan (it was asked for 14 turns and more to step onto a full
    /// ship: "target is water") and is fetched by a later voyage.
    #[test]
    fn a_ship_that_is_full_sails_and_the_unit_left_behind_is_let_off_the_plan() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let ship = spawn(&mut w, P0, port(), 36 /* galley */, 1)[0];
        let room = w.economy().ship_room(ship) as usize;
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, room + 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, port());
        // The plan holds more units than the ship has room for.
        assign_ships(&mut ai, &mut w);
        let (ship_serial, tile) = {
            let view = PlayerView::new(&w, P0);
            let first = view.serial_of(soldiers[0]);
            (
                ai.roles.with_of(first).ok_or("a ship was sent")?,
                ai.roles.boarding_of(first).ok_or("a tile was chosen")?,
            )
        };
        {
            let view = PlayerView::new(&w, P0);
            for &u in &soldiers {
                ai.roles.board_at(view.serial_of(u), ship_serial, tile);
            }
        }
        // The turn the ship has put what it holds ashore in the port (it is free again then and
        // fetches the one left behind on a later voyage).
        let at_port = |w: &World| {
            let view = PlayerView::new(w, P0);
            soldiers
                .iter()
                .filter(|&&u| view.unit_position(u) == Some(port()))
                .count()
        };
        for _ in 0..30 {
            run_turns(&mut ai, &mut w, 1);
            if at_port(&w) >= room {
                break;
            }
        }
        let view = PlayerView::new(&w, P0);
        // The ship took what it holds to the port.
        assert_eq!(at_port(&w), room, "{:?}", ai.roles);
        // The one left on the shore is let off the plan, not asked to board a full ship.
        let left = soldiers
            .iter()
            .copied()
            .find(|&u| view.unit_position(u) != Some(port()))
            .ok_or("a unit stayed on the shore")?;
        let s = view.serial_of(left);
        assert_eq!(ai.roles.boarding_of(s), None);
        assert_eq!(ai.roles.with_of(s), None);
        assert_eq!(ai.roles.crossing_of(s), Some(Crossing::Sea));
        Ok(())
    }

    /// A unit is taken aboard only if its walk to the boarding tile is within a few turns of the
    /// fastest unit's: the others wait for the next ship.
    #[test]
    fn units_far_behind_the_fastest_are_left_for_the_next_ship() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let near = spawn(&mut w, P0, land, 104 /* spearmen */, 2);
        let squares = island_squares(&w, near[0], land, 100_000);
        let far = squares
            .iter()
            .copied()
            .max_by_key(|&p| {
                let view = PlayerView::new(&w, P0);
                turns(&view, near[0], p).unwrap_or(0)
            })
            .ok_or("land")?;
        let laggard = spawn(&mut w, P0, far, 104, 1);
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        let all: Vec<hecs::Entity> = near.iter().chain(&laggard).copied().collect();
        strand(&mut ai, &w, &all, port());
        assign_ships(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        let tile = all
            .iter()
            .find_map(|&u| ai.roles.boarding_of(view.serial_of(u)))
            .ok_or("a plan was made")?;
        let walks: Vec<(hecs::Entity, u32)> = all
            .iter()
            .map(|&u| Ok((u, turns(&view, u, tile).ok_or("a walk to the tile")?)))
            .collect::<Result<_, Box<dyn std::error::Error>>>()?;
        let fastest = walks.iter().map(|&(_, t)| t).min().unwrap_or(0);
        for (u, t) in walks {
            let aboard = ai.roles.boarding_of(view.serial_of(u)).is_some();
            assert_eq!(
                aboard,
                t <= fastest + STRAGGLER_TURNS,
                "{t} turns vs {fastest}"
            );
        }
        Ok(())
    }

    /// The only ship is out scouting: carrying the stranded units comes first, so it is called off
    /// and fetches them.
    #[test]
    fn a_scouting_ship_is_called_off_to_carry_units_that_wait_for_a_ship() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, 3);
        let boat = spawn(&mut w, P0, port(), 36 /* galley */, 1)[0];
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        let ship = PlayerView::new(&w, P0).serial_of(boat);
        let _ = ai.roles.claim(ship, Role::Scout, 1, None);
        strand(&mut ai, &w, &soldiers, port());
        for _ in 0..60 {
            run_turns(&mut ai, &mut w, 1);
            let view = PlayerView::new(&w, P0);
            if soldiers
                .iter()
                .all(|&u| ai.roles.crossing_of(view.serial_of(u)).is_none())
            {
                break;
            }
        }
        let view = PlayerView::new(&w, P0);
        for &u in &soldiers {
            assert_eq!(view.unit_position(u), Some(port()), "{:?}", ai.roles);
        }
        assert!(!ai.roles.is(ship, Role::Scout), "no longer scouting");
        Ok(())
    }

    /// A unit that flies (here by a spell) is no passenger: a ship is sent for the walkers of the
    /// group and the flier is left to cross by itself.
    #[test]
    fn a_ship_fetches_the_walkers_and_not_a_unit_with_flight() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, 2);
        let flier = spawn(&mut w, P0, land, 104 /* spearmen */, 1)[0];
        w.scenario_enchant_unit(flier, f4x_def::rules::SpellId::new(56 /* flight */), P0);
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        let mut all = soldiers.clone();
        all.push(flier);
        strand(&mut ai, &w, &all, port());
        run_turns(&mut ai, &mut w, 6);
        let view = PlayerView::new(&w, P0);
        let boarded = |u| ai.roles.boarding_of(view.serial_of(u)).is_some();
        assert!(soldiers.iter().any(|&u| boarded(u)), "{:?}", ai.roles);
        assert!(!boarded(flier), "the flier is not a passenger");
        Ok(())
    }

    /// With no ship nothing is tied and the units wait.
    #[test]
    fn without_a_free_ship_the_units_wait_where_they_stand() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let soldiers = spawn(&mut w, P0, land, 104, 2);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, port());
        run_turns(&mut ai, &mut w, 3);
        assert!(voyages(&ai).is_empty());
        let view = PlayerView::new(&w, P0);
        for &u in &soldiers {
            assert_eq!(view.unit_position(u), Some(land));
            assert_eq!(ai.roles.crossing_of(view.serial_of(u)), Some(Crossing::Sea));
        }
        Ok(())
    }

    /// Known land squares on the far island, in a fixed order.
    fn far_land(w: &World) -> Vec<Position> {
        let view = PlayerView::new(w, P0);
        let centre = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let mut found: Vec<Position> = Vec::new();
        for y in 0..14 {
            for x in 20..40 {
                let p = at(x, y);
                if view
                    .terrain_class(p)
                    .is_some_and(|t| t != TerrainClass::Ocean)
                    && same_landmass(&view, centre, p)
                {
                    found.push(p);
                }
            }
        }
        found
    }

    /// Two groups bound for squares of the far island, a few squares apart, share one ship.
    #[test]
    fn groups_for_the_same_landmass_ride_one_ship() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = far_land(&w);
        let view = PlayerView::new(&w, P0);
        let a = *land.first().ok_or("land")?;
        let b = *land
            .iter()
            .find(|&&p| {
                view.distance(a, p)
                    .is_some_and(|d| (3..=MERGE_RANGE).contains(&d))
            })
            .ok_or("a second destination")?;
        drop(view);
        let first = spawn(&mut w, P0, port(), 104 /* spearmen */, 2);
        let second = spawn(&mut w, P0, port(), 104 /* spearmen */, 1);
        spawn(&mut w, P0, port(), 36 /* galley */, 2);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &first, a);
        strand(&mut ai, &w, &second, b);
        run_turns(&mut ai, &mut w, 1);
        let trips = voyages(&ai);
        assert_eq!(trips.len(), 1, "one ship: {:?}", ai.roles);
        assert_eq!(trips[0].group.len(), 3);
        Ok(())
    }

    /// A settler's destination sets the drop, whatever the soldiers want.
    #[test]
    fn a_settler_sets_the_course_of_a_shared_ship() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = far_land(&w);
        let view = PlayerView::new(&w, P0);
        let a = *land.first().ok_or("land")?;
        let b = *land
            .iter()
            .find(|&&p| {
                view.distance(a, p)
                    .is_some_and(|d| (3..=MERGE_RANGE).contains(&d))
            })
            .ok_or("a second destination")?;
        drop(view);
        let soldiers = spawn(&mut w, P0, port(), 104, 2);
        let settler = spawn(&mut w, P0, port(), 109 /* settlers */, 1);
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &w, &soldiers, a);
        let view = PlayerView::new(&w, P0);
        let s = view.serial_of(settler[0]);
        let _ = ai.roles.claim(s, Role::Settle, w.turn, Some(b));
        ai.roles.set_crossing(s, Some(Crossing::Sea));
        drop(view);
        run_turns(&mut ai, &mut w, 1);
        let trips = voyages(&ai);
        assert_eq!(trips.len(), 1, "{:?}", ai.roles);
        assert_eq!(trips[0].dest, b, "the settler's site is the drop");
        assert!(trips[0].group.contains(&s));
        Ok(())
    }

    /// Destinations beyond the range do not ride along; a different plane never does.
    #[test]
    fn a_far_destination_does_not_ride_along() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = far_land(&w);
        let view = PlayerView::new(&w, P0);
        let a = *land.first().ok_or("land")?;
        let b = *land
            .iter()
            .find(|&&p| view.distance(a, p) == Some(3))
            .ok_or("a square 3 away")?;
        assert!(rides_along(&view, a, b, 3));
        assert!(!rides_along(&view, a, b, 2));
        let home = at(COAST_CITY.0, COAST_CITY.1);
        assert!(!rides_along(&view, a, home, 100), "another landmass");
        Ok(())
    }

    /// A settler gets the ship before soldiers that want it too.
    #[test]
    fn a_settler_gets_the_only_free_ship_before_soldiers() -> TestResult {
        use crate::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = {
            let view = PlayerView::new(&w, P0);
            crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
                .ok_or("a shore on the far island")?
                .0
        };
        // The settler's site is on the far island, on a tile that sorts after the port, so that the
        // soldiers' group would come first were it not for the priority.
        let site = {
            let view = PlayerView::new(&w, P0);
            (0..8i32)
                .flat_map(|dy| (-6..=6i32).map(move |dx| (dx, dy)))
                .filter_map(|(dx, dy)| view.shift(land, dx, dy))
                .find(|&p| {
                    p.y > port().y
                        && view.terrain_class(p) != Some(TerrainClass::Ocean)
                        && crate::view::is_coastal(&view, p)
                })
                .ok_or("a coastal tile on the far island south of the port")?
        };
        let soldiers = spawn(&mut w, P0, land, 104 /* spearmen */, 2);
        let settler = spawn(&mut w, P0, port(), 111 /* high_men_settlers */, 1)[0];
        spawn(&mut w, P0, port(), 36 /* galley */, 1);
        let mut ai = Ai::new(P0);
        let serials: Vec<(hecs::Entity, f4x_core::world::serial::Serial, Position)> = {
            let view = PlayerView::new(&w, P0);
            soldiers
                .iter()
                .map(|&u| (u, view.serial_of(u), port()))
                .chain([(settler, view.serial_of(settler), site)])
                .collect()
        };
        for &(_, s, to) in &serials {
            let _ = ai.roles.claim(s, Role::Settle, w.turn, Some(to));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
        crate::pickup::run(&mut ai, &mut w, &mut Vec::new());
        let boards = |u: hecs::Entity| {
            serials
                .iter()
                .find(|&&(e, _, _)| e == u)
                .and_then(|&(_, s, _)| ai.roles.boarding_of(s))
        };
        assert!(boards(settler).is_some(), "the settler has the ship");
        assert!(soldiers.iter().all(|&u| boards(u).is_none()));
        Ok(())
    }

    /// A unit booked on a galley, with its boarding tile and a target over the sea.
    fn booked(w: &mut Game, ai: &mut Ai, at_: Position, tile: Position) -> hecs::Entity {
        let soldier = spawn(w, P0, at_, 104 /* spearmen */, 1)[0];
        let ship = spawn(w, P0, port(), 36 /* galley */, 1)[0];
        let view = PlayerView::new(w, P0);
        let (s, ship_serial) = (view.serial_of(soldier), view.serial_of(ship));
        let _ = ai.roles.claim(s, Role::Reinforce, w.turn, Some(port()));
        ai.roles.set_crossing(s, Some(Crossing::Sea));
        ai.roles.board_at(s, ship_serial, tile);
        soldier
    }

    fn said(ai: &Ai) -> Vec<String> {
        ai.take_refusals().into_iter().map(|r| r.reason).collect()
    }

    #[test]
    fn a_passenger_on_its_way_to_the_ship_passes_the_check() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let soldier = booked(&mut w, &mut ai, land, land);
        tidy_unit_registry(&mut ai, &w);
        let s = PlayerView::new(&w, P0).serial_of(soldier);
        assert!(said(&ai).is_empty());
        assert!(ai.roles.boarding_of(s).is_some(), "still booked");
        Ok(())
    }

    /// A passenger that is not where one belongs is taken off the list; its target still needs the
    /// sea and it is a walker, so it keeps the sea tag and waits for a ship again.
    #[test]
    fn a_passenger_that_is_neither_aboard_nor_on_its_way_is_let_off_and_keeps_its_tag() -> TestResult
    {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        // It stands on the far island with no order, away from its boarding tile and the ship.
        let soldier = booked(&mut w, &mut ai, land, land);
        let s = PlayerView::new(&w, P0).serial_of(soldier);
        let elsewhere = island_squares(&w, soldier, land, 6)
            .into_iter()
            .last()
            .ok_or("another square on the island")?;
        let ship = ai.roles.with_of(s).ok_or("a ship")?;
        ai.roles.board_at(s, ship, elsewhere);
        tidy_unit_registry(&mut ai, &w);
        let said = said(&ai);
        assert!(
            said.iter().any(|r| r.contains("not aboard ship")),
            "{said:?}"
        );
        assert_eq!(ai.roles.boarding_of(s), None);
        assert_eq!(ai.roles.crossing_of(s), Some(Crossing::Sea), "still waits");
        assert!(ai
            .roles
            .waiting_for_ship()
            .iter()
            .any(|&(waiting, _)| waiting == s));
        Ok(())
    }

    /// A target a walker can reach on land needs no ship: the sea tag goes, with the place on the
    /// passenger list.
    #[test]
    fn a_passenger_whose_target_needs_no_sea_crossing_loses_the_tag_and_the_booking() -> TestResult
    {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let soldier = booked(&mut w, &mut ai, land, land);
        let s = PlayerView::new(&w, P0).serial_of(soldier);
        // The target lies on its own island.
        let near = island_squares(&w, soldier, land, 4)
            .into_iter()
            .last()
            .ok_or("another square on the island")?;
        ai.roles.set_target(s, Role::Reinforce, Some(near));
        tidy_unit_registry(&mut ai, &w);
        let said = said(&ai);
        assert!(
            said.iter().any(|r| r.contains("needs no sea crossing")),
            "{said:?}"
        );
        assert_eq!(ai.roles.boarding_of(s), None);
        assert_eq!(ai.roles.crossing_of(s), None, "no tag");
        Ok(())
    }

    /// A unit that waits for a ship with no booking is held to the same rule, without a log line.
    #[test]
    fn a_waiting_unit_with_a_target_it_can_walk_to_loses_its_tag_quietly() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let soldier = spawn(&mut w, P0, land, 104 /* spearmen */, 1)[0];
        let s = PlayerView::new(&w, P0).serial_of(soldier);
        let near = island_squares(&w, soldier, land, 4)
            .into_iter()
            .last()
            .ok_or("another square on the island")?;
        let _ = ai.roles.claim(s, Role::Reinforce, w.turn, Some(near));
        ai.roles.set_crossing(s, Some(Crossing::Sea));
        tidy_unit_registry(&mut ai, &w);
        assert!(said(&ai).is_empty());
        assert_eq!(ai.roles.crossing_of(s), None);
        // One whose target is over the sea keeps it.
        let far = spawn(&mut w, P0, land, 104 /* spearmen */, 1)[0];
        let f = PlayerView::new(&w, P0).serial_of(far);
        let _ = ai.roles.claim(f, Role::Reinforce, w.turn, Some(port()));
        ai.roles.set_crossing(f, Some(Crossing::Sea));
        tidy_unit_registry(&mut ai, &w);
        assert_eq!(ai.roles.crossing_of(f), Some(Crossing::Sea));
        Ok(())
    }

    /// A plan that simply ended (its target is gone) is no fault: the passenger is taken off the
    /// list and nothing is logged.
    #[test]
    fn a_passenger_whose_plan_ended_is_let_off_without_a_word() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let soldier = booked(&mut w, &mut ai, land, land);
        let s = PlayerView::new(&w, P0).serial_of(soldier);
        ai.roles.set_crossing(s, None);
        let _ = ai.roles.claim(s, Role::Reinforce, w.turn, None);
        ai.roles.set_target(s, Role::Reinforce, None);
        tidy_unit_registry(&mut ai, &w);
        assert!(said(&ai).is_empty());
        assert_eq!(ai.roles.boarding_of(s), None);
        assert_eq!(ai.roles.crossing_of(s), None);
        Ok(())
    }

    #[test]
    fn a_passenger_that_flies_loses_the_tag_and_the_booking() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let land = shore(&w)?;
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let flier = booked(&mut w, &mut ai, land, land);
        w.scenario_enchant_unit(flier, f4x_def::rules::SpellId::new(56 /* flight */), P0);
        tidy_unit_registry(&mut ai, &w);
        let said_flier = said(&ai);
        assert!(
            said_flier.iter().any(|r| r.contains("no walker")),
            "{said_flier:?}"
        );
        let s = PlayerView::new(&w, P0).serial_of(flier);
        assert_eq!(ai.roles.boarding_of(s), None);
        assert_eq!(ai.roles.crossing_of(s), None, "a flier waits for no ship");
        Ok(())
    }
}
