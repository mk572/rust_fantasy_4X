//! Units that cannot walk to their target (`Crossing::Sea`: the target is on this plane and no
//! known land route leads to it) wait for a ship, wherever their job (a reinforcement, a settler,
//! a march) sent them. Here the ship comes for them (task 647).
//!
//! The units are grouped by destination and by the land they can walk over together. For each
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
use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

/// Candidate boarding tiles that get a real path search, per group.
const CANDIDATES: usize = 12;
/// How many turns longer than the best a unit may walk and still be taken aboard.
const STRAGGLER_TURNS: u32 = 5;
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
                dest: ai.roles.destination_of(first)?,
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
        walks.sort_by_key(|&(t, u)| (t, view.serial_of(u)));
        let Some(&(fastest, _)) = walks.first() else {
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
    tidy(ai, game);
    if ai.roles.waiting_for_ship().is_empty() && voyages(ai).is_empty() {
        return;
    }
    assign_ships(ai, game);
    walk_to_shore(ai, game, events);
    for pickup in voyages(ai) {
        advance(ai, game, &pickup, events);
    }
}

/// Forget the ties of units that no longer wait (their tag is gone: the target changed, they got
/// there another way) and the ships that have nobody left to fetch.
fn tidy(ai: &mut Ai, game: &Game) {
    let view = PlayerView::new(game, ai.player);
    for u in view.my_units() {
        let s = view.serial_of(u);
        if ai.roles.boarding_of(s).is_some() && ai.roles.crossing_of(s) != Some(Crossing::Sea) {
            ai.roles.unboard(s);
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
        let settler =
            |u: &hecs::Entity| view.unit_has_ability(*u, f4x_def::rules::Ability::new(12));
        groups.sort_by_key(|(_, units)| !units.iter().any(settler));
        for (dest, region) in groups {
            let Some(plan) = plan_group(&view, &region, dest, &free) else {
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
        let turn = game.turn;
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
    let (ship, ship_pos, aboard, total, on_land, drop, idle) = {
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
        if idle {
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
    if idle {
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Send the units to `dest` with no way to walk there: their job's target and the tag the
    /// dispatcher puts on them.
    fn strand(ai: &mut Ai, g: &Game, units: &[hecs::Entity], dest: Position) {
        let view = PlayerView::new(g, P0);
        for &u in units {
            let s = view.serial_of(u);
            let _ = ai.roles.claim(s, Role::Reinforce, g.turn, Some(dest));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
    }

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    fn shore(g: &Game) -> Result<Position, Box<dyn std::error::Error>> {
        let view = PlayerView::new(g, P0);
        let (land, _) = crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
            .ok_or("a shore tile near the far city")?;
        Ok(land)
    }

    /// Land squares the unit can walk to, nearest to `near` first, at most `n`.
    fn island_squares(g: &Game, unit: hecs::Entity, near: Position, n: usize) -> Vec<Position> {
        let view = PlayerView::new(g, P0);
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

    fn run_turns(ai: &mut Ai, g: &mut Game, n: usize) {
        for _ in 0..n {
            run(ai, g, &mut Vec::new());
            g.apply(Command::EndTurn);
            g.balance_farmers(P0);
        }
    }

    /// Soldiers on the far island, bound for our port, with a free ship in port: the ship comes,
    /// takes them aboard and puts them ashore in the port, and their tag is cleared.
    #[test]
    fn units_that_need_a_ship_are_fetched_and_brought_to_their_destination() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let soldiers = spawn(&mut g, P0, land, 104 /* spearmen */, 3);
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &g, &soldiers, port());
        for _ in 0..60 {
            run_turns(&mut ai, &mut g, 1);
            let view = PlayerView::new(&g, P0);
            if soldiers
                .iter()
                .all(|&u| ai.roles.crossing_of(view.serial_of(u)).is_none())
            {
                break;
            }
        }
        let view = PlayerView::new(&g, P0);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let first = spawn(&mut g, P0, land, 104 /* spearmen */, 1);
        let squares = island_squares(&g, first[0], land, 60);
        // Five units, spread over the squares near the shore.
        let spots: Vec<Position> = squares.iter().copied().step_by(8).take(4).collect();
        assert!(spots.len() >= 3, "land to scatter the units on: {spots:?}");
        let mut soldiers: Vec<hecs::Entity> = first;
        soldiers.extend(
            spots
                .iter()
                .flat_map(|&p| spawn(&mut g, P0, p, 104 /* spearmen */, 1)),
        );
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &g, &soldiers, port());
        assign_ships(&mut ai, &mut g);
        let view = PlayerView::new(&g, P0);
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

    /// A unit is taken aboard only if its walk to the boarding tile is within a few turns of the
    /// fastest unit's: the others wait for the next ship.
    #[test]
    fn units_far_behind_the_fastest_are_left_for_the_next_ship() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let near = spawn(&mut g, P0, land, 104 /* spearmen */, 2);
        let squares = island_squares(&g, near[0], land, 100_000);
        let far = squares
            .iter()
            .copied()
            .max_by_key(|&p| {
                let view = PlayerView::new(&g, P0);
                turns(&view, near[0], p).unwrap_or(0)
            })
            .ok_or("land")?;
        let laggard = spawn(&mut g, P0, far, 104, 1);
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        let all: Vec<hecs::Entity> = near.iter().chain(&laggard).copied().collect();
        strand(&mut ai, &g, &all, port());
        assign_ships(&mut ai, &mut g);
        let view = PlayerView::new(&g, P0);
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

    /// With no ship nothing is tied and the units wait.
    #[test]
    fn without_a_free_ship_the_units_wait_where_they_stand() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = shore(&g)?;
        let soldiers = spawn(&mut g, P0, land, 104, 2);
        g.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        strand(&mut ai, &g, &soldiers, port());
        run_turns(&mut ai, &mut g, 3);
        assert!(voyages(&ai).is_empty());
        let view = PlayerView::new(&g, P0);
        for &u in &soldiers {
            assert_eq!(view.unit_position(u), Some(land));
            assert_eq!(ai.roles.crossing_of(view.serial_of(u)), Some(Crossing::Sea));
        }
        Ok(())
    }
}
