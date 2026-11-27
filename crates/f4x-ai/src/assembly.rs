//! Gathering a force (task 837, `docs/ai-design.md` "Gathering a force"): how long units need to a
//! square, on foot or through a gate; the one square beside each target that everything sent against
//! it goes to; the rally point on the way of the unit that arrives last; and the walk there. The
//! campaign (`war.rs`), the raids (`raid.rs`), the perimeter and the walk repair
//! (`dispatch::resend`) use it. The attack is `attack.rs`'s.
//!
//! Tests: below, on shipped units.

use std::collections::HashMap;

use f4x_core::city::gate::{ASTRAL_GATE, EARTH_GATE};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::unit::movement::MoveError;
use f4x_core::unit::pathing::ReachSignature;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::unit_flag::UnitFlag;
use f4x_master::Game;

use crate::roles::{MusterKind, Role};
use crate::view::distance;
use crate::{Ai, SendError};
use f4x_core::unit::task::{Assembly, Tasks};

/// The squares of a way, each with the turn the units stand on it.
type Way = Vec<(Position, u32)>;

/// How a door is gone through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Passage {
    /// A step from one Earth Gate city to another.
    EarthGate,
    /// A change of plane on the square: a tower, an Astral Gate, a unit's own planar travel, or
    /// the square a Plane Shift is cast on.
    Plane,
}

/// A way across the map that walking does not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Door {
    pub entry: Position,
    pub exit: Position,
    pub passage: Passage,
}

/// How units get to a square.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Trip {
    pub turns: u32,
    /// The squares walked, the one the walk starts on first: the units' own, or the exit of the
    /// door.
    pub way: Way,
    /// The door gone through, when that is shorter than the walk.
    pub door: Option<Door>,
}

/// Why units could not be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarchError {
    /// There is no party.
    NoUnits,
    /// No square next to the target is free of enemies, lairs and cities, with room.
    NoOpenSquare,
    /// There is no way there, on foot or through a gate.
    Unreachable,
    /// A way exists but the game refused the order, for the reason given.
    Refused(MoveError),
}

/// The square on the other plane.
pub(crate) fn mirror(p: Position) -> Position {
    let plane = match p.plane {
        Plane::Arcanus => Plane::Myrror,
        Plane::Myrror => Plane::Arcanus,
    };
    Position { plane, ..p }
}

/// The doors every unit of the AI may use: its Earth Gates and Astral Gates, and the square the
/// campaign crosses by.
fn doors(ai: &Ai, world: &World) -> Vec<Door> {
    let view = PlayerView::new(world, ai.player);
    let me = ai.player;
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let has = |p: Position, gate| world.economy().has_gate(p, me, gate);
    let earth: Vec<Position> = cities
        .iter()
        .copied()
        .filter(|&p| has(p, EARTH_GATE))
        .collect();
    let mut found = Vec::new();
    for &entry in &earth {
        for &exit in &earth {
            if entry != exit && entry.plane == exit.plane {
                found.push(Door {
                    entry,
                    exit,
                    passage: Passage::EarthGate,
                });
            }
        }
    }
    if world.units().planar_seal_active() {
        return found;
    }
    let astral = cities
        .iter()
        .copied()
        .filter(|&p| has(p, ASTRAL_GATE) && view.city_at(mirror(p)).is_none());
    found.extend(astral.chain(ai.crossing).map(|entry| Door {
        entry,
        exit: mirror(entry),
        passage: Passage::Plane,
    }));
    found
}

/// The doors and the walks worked out for one look at the world: no unit may move while it is
/// kept.
pub(crate) struct Travel {
    doors: Vec<Door>,
    ways: HashMap<(ReachSignature, HalfInt, HalfInt, Position, Position), Option<Way>>,
}

impl Travel {
    pub fn new(ai: &Ai, world: &World) -> Self {
        Self {
            doors: doors(ai, world),
            ways: HashMap::new(),
        }
    }

    /// The walk of the units from `from` to `to` ([`PlayerView::way`]); units of one kind on one
    /// square share the search.
    fn walk(
        &mut self,
        view: &PlayerView,
        units: &[hecs::Entity],
        from: Position,
        to: Position,
    ) -> Option<Way> {
        let Some(signature) = view.reach_signature(units) else {
            return view.way(units, from, to);
        };
        let least = |f: &dyn Fn(hecs::Entity) -> HalfInt| {
            units.iter().map(|&u| f(u)).min().unwrap_or(HalfInt::ZERO)
        };
        let left = least(&|u| view.unit_moves_left(u));
        let full = least(&|u| view.unit_max_moves(u));
        self.ways
            .entry((signature, left, full, from, to))
            .or_insert_with(|| view.way(units, from, to))
            .clone()
    }

    /// How units standing together get to `to`: the shorter of their walk and their way through a
    /// door. `None` when there is neither.
    pub fn trip(
        &mut self,
        view: &PlayerView,
        units: &[hecs::Entity],
        to: Position,
    ) -> Option<Trip> {
        let from = view.unit_position(*units.first()?)?;
        let turns_of = |way: &Way| way.last().map_or(0, |w| w.1);
        let mut best = self.walk(view, units, from, to).map(|walk| {
            let turns = turns_of(&walk);
            let mut way = vec![(from, 0)];
            way.extend(walk);
            Trip {
                turns,
                way,
                door: None,
            }
        });
        // Units that travel between the planes by themselves do so where they stand.
        let own = (units
            .iter()
            .all(|&u| view.unit_has_flag(u, UnitFlag::PlanarTravel))
            && view.can_shift_plane(units))
        .then_some(Door {
            entry: from,
            exit: mirror(from),
            passage: Passage::Plane,
        });
        let doors: Vec<Door> = own.into_iter().chain(self.doors.iter().copied()).collect();
        for door in doors {
            if door.entry.plane != from.plane || door.exit.plane != to.plane {
                continue;
            }
            // A walker does not come out on the sea.
            if view.terrain_class(door.exit) == Some(TerrainClass::Ocean)
                && units.iter().any(|&u| view.is_walker(u))
            {
                continue;
            }
            let Some(before) = self.walk(view, units, from, door.entry) else {
                continue;
            };
            // Going through takes a turn of its own.
            let through = turns_of(&before) + 1;
            if best.as_ref().is_some_and(|b| b.turns <= through) {
                continue;
            }
            let Some(after) = self.walk(view, units, door.exit, to) else {
                continue;
            };
            let turns = through + turns_of(&after);
            if best.as_ref().is_some_and(|b| b.turns <= turns) {
                continue;
            }
            let mut way = vec![(door.exit, through)];
            way.extend(after.into_iter().map(|(p, t)| (p, t + through)));
            best = Some(Trip {
                turns,
                way,
                door: Some(door),
            });
        }
        best
    }

    /// Whether any of the units could get to any of `squares`: one search for each kind of unit,
    /// that stops at the first square found, so that a target nothing can get to (across the
    /// water) costs no search for each square round it. A door may take a unit anywhere.
    fn any_gets_to(&self, view: &PlayerView, units: &[hecs::Entity], squares: &[Position]) -> bool {
        let mut asked: Vec<ReachSignature> = Vec::new();
        units.iter().any(|&u| {
            if !self.doors.is_empty()
                || view.unit_has_flag(u, UnitFlag::PlanarTravel)
                || view.unit_position(u).is_some_and(|p| squares.contains(&p))
            {
                return true;
            }
            let Some(signature) = view.reach_signature(&[u]) else {
                return true;
            };
            if asked.contains(&signature) {
                return false;
            }
            asked.push(signature);
            view.reaches_any(&[u], squares)
        })
    }

    /// Send units standing together on their way to `to`: through the door when that is shorter,
    /// walking to it first. Units on the square of a door that is not open yet (a Plane Shift not
    /// yet cast) wait there.
    pub fn send(
        &mut self,
        ai: &Ai,
        game: &mut Game,
        units: &[hecs::Entity],
        to: Position,
        events: &mut Vec<Event>,
    ) -> Result<(), MarchError> {
        let (command, dest) = {
            let view = PlayerView::new(game, ai.player);
            let from = units
                .first()
                .and_then(|&u| view.unit_position(u))
                .ok_or(MarchError::NoUnits)?;
            if from == to {
                return Ok(());
            }
            let trip = self.trip(&view, units, to).ok_or(MarchError::Unreachable)?;
            let units = units.to_vec();
            match trip.door {
                Some(door) if door.entry == from => match door.passage {
                    Passage::EarthGate => (
                        Command::EarthGate {
                            units,
                            to: door.exit,
                        },
                        door.exit,
                    ),
                    Passage::Plane if view.can_shift_plane(&units) => {
                        (Command::ShiftPlane { units }, door.exit)
                    }
                    Passage::Plane => return Ok(()),
                },
                door => {
                    let dest = door.map_or(to, |d| d.entry);
                    (Command::GoTo { units, to: dest }, dest)
                }
            }
        };
        // A square with no room for the party is left alone (it may be free later).
        if !Ai::room_at(game, dest, units.len()) {
            return Err(MarchError::Refused(MoveError::StackFull));
        }
        match ai.send(game, command, events) {
            Ok(()) => Ok(()),
            Err(SendError::Rejected(Rejection::Move(why)) | SendError::Stopped(why)) => {
                Err(MarchError::Refused(why))
            }
            Err(_) => Err(MarchError::Unreachable),
        }
    }
}

/// The units on each square, in the order the squares first appear.
pub(crate) fn by_square(
    view: &PlayerView,
    units: &[hecs::Entity],
) -> Vec<(Position, Vec<hecs::Entity>)> {
    let mut found: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
    for &u in units {
        let Some(p) = view.unit_position(u) else {
            continue;
        };
        match found.iter_mut().find(|(q, _)| *q == p) {
            Some((_, list)) => list.push(u),
            None => found.push((p, vec![u])),
        }
    }
    found
}

/// `units` and the units the registry has bound for `target` already.
fn force_for(view: &PlayerView, target: Position, units: &[hecs::Entity]) -> Vec<hecs::Entity> {
    let mut force = units.to_vec();
    let tasks = view.tasks();
    for u in view.my_units() {
        let s = view.serial_of(u);
        let bound = [Role::March, Role::Raid, Role::Perimeter]
            .into_iter()
            .any(|r| tasks.roles.target_of(s, r) == Some(target))
            || tasks
                .roles
                .muster_of(s)
                .is_some_and(|(_, m)| m.objective == Some(target));
        if bound && !force.contains(&u) {
            force.push(u);
        }
    }
    force
}

/// Whether a force may gather on `square`: no enemy, lair, tower or city on it, and room for the
/// force beside what else of the AI's stands there.
fn open(world: &World, view: &PlayerView, square: Position, force: &[hecs::Entity]) -> bool {
    if view.hostile_at(square).is_some_and(|u| !u.is_empty())
        || view.city_at(square).is_some()
        || view.has_lair(square)
        || view.towers().iter().any(|&(p, _)| p == square)
    {
        return false;
    }
    let others = world
        .economy()
        .army_at(square)
        .into_iter()
        .filter(|u| !force.contains(u))
        .count();
    others + force.len().min(world.max_tile()) <= world.max_tile()
}

/// Change the gathering kept against the target, which is made when there is none.
fn record(ai: &Ai, game: &mut Game, target: Position, change: impl FnOnce(&mut Assembly)) {
    let mut assembly = ai
        .tasks(game)
        .assemblies
        .iter()
        .find(|a| a.target == target)
        .copied()
        .unwrap_or(Assembly {
            target,
            beside: None,
            rally: None,
        });
    change(&mut assembly);
    ai.write(game).set_assembly(assembly);
}

/// The rally point of the force against `target`, when one is chosen.
pub(crate) fn rally_of(tasks: &Tasks, target: Position) -> Option<Position> {
    tasks
        .assemblies
        .iter()
        .find(|a| a.target == target)
        .and_then(|a| a.rally)
}

/// Make `square` the one the force against `target` attacks from (a landing).
pub(crate) fn set_beside(ai: &Ai, game: &mut Game, target: Position, square: Position) {
    record(ai, game, target, |a| a.beside = Some(square));
}

/// Forget the targets nothing is bound for any more.
pub(crate) fn tidy(ai: &Ai, game: &mut Game) {
    let gone: Vec<Position> = {
        let tasks = ai.tasks(game);
        let mut live: Vec<Position> = ai.campaign.into_iter().collect();
        for role in [Role::March, Role::Raid, Role::Perimeter] {
            live.extend(tasks.roles.targeted(role).into_iter().map(|(_, p)| p));
        }
        live.extend(
            tasks
                .roles
                .musters()
                .into_iter()
                .filter_map(|m| m.1.objective),
        );
        tasks
            .assemblies
            .iter()
            .map(|a| a.target)
            .filter(|t| !live.contains(t))
            .collect()
    };
    ai.write(game).drop_assemblies(gone);
}

/// The square beside `target` that everything sent against it goes to: the one chosen before,
/// while it is open; else the last square before the target on the way of the unit of `units` that
/// arrives last, or the open square nearest to that (to the one chosen before).
pub(crate) fn beside(
    ai: &mut Ai,
    game: &mut Game,
    travel: &mut Travel,
    target: Position,
    units: &[hecs::Entity],
) -> Result<Position, MarchError> {
    if units.is_empty() {
        return Err(MarchError::NoUnits);
    }
    let me = ai.player;
    let stored = ai
        .tasks(game)
        .assemblies
        .iter()
        .find(|a| a.target == target)
        .and_then(|a| a.beside);
    let world: &World = game;
    let view = PlayerView::new(world, me);
    let force = force_for(&view, target, units);
    if let Some(square) = stored.filter(|&s| open(world, &view, s, &force)) {
        return Ok(square);
    }
    let mut candidates: Vec<Position> = crate::military::neighbours(&view, target)
        .into_iter()
        .filter(|&p| open(world, &view, p, &force))
        .collect();
    if candidates.is_empty() {
        return Err(MarchError::NoOpenSquare);
    }
    if !travel.any_gets_to(&view, units, &candidates) {
        return Err(MarchError::Unreachable);
    }
    // The last unit to arrive, and the square it would step to the target from.
    let slowest = units
        .iter()
        .filter_map(|&u| Some((travel.trip(&view, &[u], target)?, view.serial_of(u))))
        .max_by_key(|(trip, serial)| (trip.turns, std::cmp::Reverse(*serial)))
        .map(|(trip, _)| trip);
    let wanted = stored.or_else(|| {
        slowest
            .as_ref()
            .and_then(|t| t.way.iter().rev().nth(1))
            .map(|&(p, _)| p)
    });
    let from = view.unit_position(units[0]).unwrap_or(target);
    let near = wanted.unwrap_or(from);
    candidates.sort_by_key(|&p| (distance(&view, near, p), p.y, p.x));
    // A square every unit that can get to the target can get to; else the first any of them can.
    let able: Vec<hecs::Entity> = units
        .iter()
        .copied()
        .filter(|&u| travel.trip(&view, &[u], target).is_some())
        .collect();
    let asked = if able.is_empty() { units } else { &able };
    let mut some = None;
    let mut chosen = None;
    for &square in &candidates {
        let reached = asked
            .iter()
            .filter(|&&u| {
                view.unit_position(u) == Some(square) || travel.trip(&view, &[u], square).is_some()
            })
            .count();
        if reached == asked.len() {
            chosen = Some(square);
            break;
        }
        if reached > 0 && some.is_none() {
            some = Some(square);
        }
    }
    let square = chosen.or(some).ok_or(MarchError::Unreachable)?;
    record(ai, game, target, |a| a.beside = Some(square));
    Ok(square)
}

/// Where the force `units` gathers on its way to `target`, which it attacks from `beside`: the
/// point chosen before, while it can still be used; else the first square on the way of the unit
/// that arrives last that every unit stands on no later than that one. Never a city, a lair, a
/// tower, a square with an enemy or without room, or one beside the target. `None` when no square
/// serves: the force then goes straight to `beside`.
pub(crate) fn rally(
    ai: &mut Ai,
    game: &mut Game,
    travel: &mut Travel,
    target: Position,
    beside: Position,
    units: &[hecs::Entity],
) -> Option<Position> {
    let me = ai.player;
    let stored = rally_of(&ai.tasks(game), target);
    let world: &World = game;
    let view = PlayerView::new(world, me);
    let force = force_for(&view, target, units);
    let usable = |view: &PlayerView, p: Position| {
        distance(view, p, target).is_none_or(|d| d > 1) && open(world, view, p, &force)
    };
    if let Some(point) = stored.filter(|&p| usable(&view, p)) {
        return Some(point);
    }
    let trips: Vec<(Trip, hecs::Entity)> = units
        .iter()
        .filter_map(|&u| Some((travel.trip(&view, &[u], beside)?, u)))
        .collect();
    let (slowest, last) = trips
        .iter()
        .max_by_key(|(trip, u)| (trip.turns, std::cmp::Reverse(view.serial_of(*u))))?;
    let point = slowest.way.iter().find_map(|&(square, by)| {
        let all_there = usable(&view, square)
            && trips.iter().filter(|(_, u)| u != last).all(|&(_, u)| {
                view.unit_position(u) == Some(square)
                    || travel
                        .trip(&view, &[u], square)
                        .is_some_and(|t| t.turns <= by)
            });
        all_there.then_some(square)
    });
    record(ai, game, target, |a| a.rally = point);
    point
}

/// Send a party against `target`: to the square beside it that everything bound for it goes to.
/// Units that stand there stay. `Ok` when any unit is there or on its way.
pub(crate) fn march(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    target: Position,
    events: &mut Vec<Event>,
) -> Result<(), MarchError> {
    let mut travel = Travel::new(ai, game);
    let square = beside(ai, game, &mut travel, target, units)?;
    let groups = by_square(&PlayerView::new(game, ai.player), units);
    let mut sent = false;
    let mut refused = None;
    for (at, group) in groups {
        if at == square {
            sent = true;
            continue;
        }
        match travel.send(ai, game, &group, square, events) {
            Ok(()) => sent = true,
            Err(why) => refused = refused.or(Some(why)),
        }
    }
    match refused {
        Some(why) if !sent => Err(why),
        _ => Ok(()),
    }
}

/// The facts of a march the game refused for want of room: how many the party is and how full
/// each square beside the target is.
pub(crate) fn march_detail(
    ai: &Ai,
    world: &World,
    units: &[hecs::Entity],
    target: Position,
) -> String {
    let view = PlayerView::new(world, ai.player);
    let squares: Vec<String> = crate::military::neighbours(&view, target)
        .into_iter()
        .filter(|&p| !world.economy().army_at(p).is_empty())
        .map(|p| {
            format!(
                "({}, {}) holds {}",
                p.x,
                p.y,
                world.economy().army_at(p).len()
            )
        })
        .collect();
    format!(
        "party of {} to ({}, {}), room for {} on a square; beside it: {}",
        units.len(),
        target.x,
        target.y,
        world.max_tile(),
        if squares.is_empty() {
            "no square with units".to_string()
        } else {
            squares.join(", ")
        }
    )
}

/// The muster of the campaign against `target` on `rally`: begun, or joined by the units. Musters
/// for other targets, or on squares not in `rallies`, end.
pub(crate) fn campaign_muster(
    ai: &Ai,
    game: &mut Game,
    target: Position,
    rally: Position,
    rallies: &[Position],
    members: &[f4x_core::world::serial::Serial],
) {
    let mut standing = None;
    let musters = ai.tasks(game).roles.musters();
    for (id, muster, _) in musters {
        if muster.kind != MusterKind::Campaign {
            continue;
        }
        if muster.objective != Some(target) || !rallies.contains(&muster.rally) {
            ai.write(game).end_muster(id);
        } else if muster.rally == rally {
            standing = Some(id);
        }
    }
    match standing {
        Some(id) => {
            ai.write(game).join_muster(id, members);
        }
        None => {
            ai.write(game)
                .begin_muster(MusterKind::Campaign, rally, Some(target), members);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_site, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::world::World;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const PALADINS: u16 = 113; // high_men_paladins
    const SPEARMEN: u16 = 104; // high_men_spearmen

    fn target() -> Position {
        at(4, 4)
    }

    /// The west wizard on its island with a site at the target.
    fn world() -> Result<Game, Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        spawn_site(&mut w, target(), Vec::new(), Vec::new())?;
        Ok(w)
    }

    fn turns(ai: &Ai, w: &World, unit: hecs::Entity, to: Position) -> Option<Trip> {
        Travel::new(ai, w).trip(&PlayerView::new(w, P0), &[unit], to)
    }

    fn earth_gates(w: &mut World, at_squares: &[Position]) -> TestResult {
        let gate = f4x_def::rules::SpellId::new(37 /* earth_gate */);
        for &p in at_squares {
            let city = w.economy().city_at(p).ok_or("a city")?;
            w.scenario_enchant_city(city, gate, P0);
        }
        Ok(())
    }

    /// The rally point lies on the way of the unit that arrives last, every other unit is there
    /// no later than it, and it is no city and not beside the target.
    #[test]
    fn the_rally_point_lies_on_the_way_of_the_last_to_arrive_and_is_no_city() -> TestResult {
        let mut w = world()?;
        let far = spawn(&mut w, P0, at(10, 10), PALADINS, 1)[0];
        let mut units = spawn(&mut w, P0, at(10, 3), PALADINS, 1);
        units.extend(spawn(&mut w, P0, at(7, 9), PALADINS, 1));
        units.push(far);
        let mut ai = Ai::new(P0);
        let mut travel = Travel::new(&ai, &w);
        let square =
            beside(&mut ai, &mut w, &mut travel, target(), &units).map_err(|e| format!("{e:?}"))?;
        let point =
            rally(&mut ai, &mut w, &mut travel, target(), square, &units).ok_or("a point")?;
        let view = PlayerView::new(&w, P0);
        let slowest = units
            .iter()
            .filter_map(|&u| Some((turns(&ai, &w, u, square)?, u)))
            .max_by_key(|(t, u)| (t.turns, std::cmp::Reverse(view.serial_of(*u))))
            .ok_or("a slowest unit")?;
        assert_eq!(slowest.1, far, "the far one arrives last");
        let by = slowest
            .0
            .way
            .iter()
            .find(|w| w.0 == point)
            .ok_or("the point is on its way")?
            .1;
        for &u in &units {
            let there = turns(&ai, &w, u, point).map_or(0, |t| t.turns);
            assert!(u == far || there <= by, "{there} turns against {by}");
        }
        assert!(view.city_at(point).is_none());
        assert!(distance(&view, point, target()).is_some_and(|d| d > 1));
        assert_eq!(rally_of(&ai.tasks(&w), target()), Some(point), "it is kept");
        // Other units do not move it while it can be used.
        let more = spawn(&mut w, P0, at(3, 9), PALADINS, 1);
        let mut travel = Travel::new(&ai, &w);
        assert_eq!(
            rally(&mut ai, &mut w, &mut travel, target(), square, &more),
            Some(point)
        );
        // No room on it: another is chosen.
        let full = w.max_tile();
        spawn(&mut w, P0, point, SPEARMEN, full);
        let mut travel = Travel::new(&ai, &w);
        assert_ne!(
            rally(&mut ai, &mut w, &mut travel, target(), square, &units),
            Some(point)
        );
        Ok(())
    }

    /// Everything sent against a target goes to one square beside it, wherever it comes from;
    /// when other units fill the square the nearest open one takes its place.
    #[test]
    fn all_units_sent_against_a_target_use_one_square_until_it_is_blocked() -> TestResult {
        let mut w = world()?;
        let south = spawn(&mut w, P0, at(4, 9), PALADINS, 2);
        let east = spawn(&mut w, P0, at(9, 3), PALADINS, 2);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        march(&mut ai, &mut w, &south, target(), &mut events).map_err(|e| format!("{e:?}"))?;
        let mut travel = Travel::new(&ai, &w);
        let first =
            beside(&mut ai, &mut w, &mut travel, target(), &south).map_err(|e| format!("{e:?}"))?;
        assert_eq!(PlayerView::new(&w, P0).distance(first, target()), Some(1));
        assert_eq!(
            beside(&mut ai, &mut w, &mut travel, target(), &east),
            Ok(first)
        );
        let full = w.max_tile();
        spawn(&mut w, P0, first, SPEARMEN, full);
        let mut travel = Travel::new(&ai, &w);
        let next =
            beside(&mut ai, &mut w, &mut travel, target(), &east).map_err(|e| format!("{e:?}"))?;
        let view = PlayerView::new(&w, P0);
        assert_ne!(next, first);
        assert_eq!(view.distance(next, target()), Some(1));
        assert_eq!(view.distance(next, first), Some(1), "the nearest to it");
        Ok(())
    }

    /// A unit by an Earth Gate city takes the gate when that is shorter, and so is no longer the
    /// last to arrive.
    #[test]
    fn a_gate_changes_who_arrives_last() -> TestResult {
        let mut w = world()?;
        let seat = at(4, 6);
        w.command(Command::FoundCity {
            owner: P0,
            pos: seat,
            name: "Seat".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 6,
        });
        let by_gate = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), SPEARMEN, 1)[0];
        let walker = spawn(&mut w, P0, at(INLAND_CITY.0, 10), SPEARMEN, 1)[0];
        let ai = Ai::new(P0);
        let square = at(4, 5);
        let walks = |w: &World, u| turns(&ai, w, u, square).ok_or("a way");
        let (before, other) = (walks(&w, by_gate)?, walks(&w, walker)?);
        assert!(before.door.is_none());
        assert!(before.turns > other.turns, "{before:?} {other:?}");
        earth_gates(&mut w, &[at(COAST_CITY.0, COAST_CITY.1), seat])?;
        let after = walks(&w, by_gate)?;
        assert_eq!(after.door.map(|d| d.exit), Some(seat));
        assert!(after.turns < walks(&w, walker)?.turns, "{after:?}");
        assert_eq!(
            after.way.first().map(|w| w.0),
            Some(seat),
            "walked from the exit"
        );
        // Sent there, it steps through the gate.
        let mut travel = Travel::new(&ai, &w);
        travel
            .send(&ai, &mut w, &[by_gate], square, &mut Vec::new())
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(w.units().unit_position(by_gate), Some(seat));
        Ok(())
    }

    /// No square serves as a rally point when the only way is over squares beside the target.
    #[test]
    fn units_beside_the_target_have_no_rally_point() -> TestResult {
        let mut w = world()?;
        let units = spawn(&mut w, P0, at(5, 5), PALADINS, 3);
        let mut ai = Ai::new(P0);
        let mut travel = Travel::new(&ai, &w);
        let square =
            beside(&mut ai, &mut w, &mut travel, target(), &units).map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            rally(&mut ai, &mut w, &mut travel, target(), square, &units),
            None
        );
        Ok(())
    }
}
