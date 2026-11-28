//! Travel: how units get to a square, on foot or through a door (an Earth Gate, a tower or Astral
//! Gate, a unit's own planar travel, a square a Plane Shift is to be cast on), and the next step
//! of that way. The movement helper walks it (`unit/task_walk.rs`); the computer player asks it
//! how long a force needs (`f4x-ai`, `assembly.rs`).
//!
//! Tests: in `f4x-ai`, `assembly.rs`, and `tests/it/c9_task_walk.rs`.

use std::collections::HashMap;

use crate::city::gate::{ASTRAL_GATE, EARTH_GATE};
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use crate::unit::pathing::ReachSignature;
use crate::world::{Command, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::unit_flag::UnitFlag;

/// The squares of a way, each with the turn the units stand on it.
pub type Way = Vec<(Position, u32)>;

/// How a door is gone through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    /// A step from one Earth Gate city to another.
    EarthGate,
    /// A change of plane on the square: a tower, an Astral Gate, a unit's own planar travel, or
    /// the square a Plane Shift is cast on.
    Plane,
}

/// A way across the map that walking does not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Door {
    pub entry: Position,
    pub exit: Position,
    pub passage: Passage,
}

/// How units get to a square.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trip {
    pub turns: u32,
    /// The squares walked, the one the walk starts on first: the units' own, or the exit of the
    /// door.
    pub way: Way,
    /// The door gone through, when that is shorter than the walk.
    pub door: Option<Door>,
}

/// What units on their way do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Walk to the square: the target, or the entry of a door.
    Walk(Position),
    /// Go through the Earth Gate they stand in, to the city.
    EarthGate(Position),
    /// Change plane where they stand, coming out on the square.
    Shift(Position),
}

impl Step {
    /// The square the step takes the units to.
    pub fn to(self) -> Position {
        match self {
            Step::Walk(p) | Step::EarthGate(p) | Step::Shift(p) => p,
        }
    }

    /// The command that makes the step.
    pub fn command(self, units: Vec<hecs::Entity>) -> Command {
        match self {
            Step::Walk(to) => Command::GoTo { units, to },
            Step::EarthGate(to) => Command::EarthGate { units, to },
            Step::Shift(_) => Command::ShiftPlane { units },
        }
    }
}

/// Why units have no next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelError {
    /// There are no units.
    NoUnits,
    /// There is no way there, on foot or through a door.
    Unreachable,
}

/// The square on the other plane.
pub fn mirror(p: Position) -> Position {
    let plane = match p.plane {
        Plane::Arcanus => Plane::Myrror,
        Plane::Myrror => Plane::Arcanus,
    };
    Position { plane, ..p }
}

/// The doors every unit of the player may use: its Earth Gates and Astral Gates, and `door`.
fn doors(world: &World, player: PlayerId, door: Option<Position>) -> Vec<Door> {
    let view = PlayerView::new(world, player);
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    let has = |p: Position, gate| world.economy().has_gate(p, player, gate);
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
    found.extend(astral.chain(door).map(|entry| Door {
        entry,
        exit: mirror(entry),
        passage: Passage::Plane,
    }));
    found
}

/// The doors and the walks worked out for one look at the world: no unit may move while it is
/// kept.
pub struct Travel {
    doors: Vec<Door>,
    ways: HashMap<(ReachSignature, HalfInt, HalfInt, Position, Position), Option<Way>>,
}

impl Travel {
    /// For the units of `player`, who may change plane on `door` as well.
    pub fn new(world: &World, player: PlayerId, door: Option<Position>) -> Self {
        Self {
            doors: doors(world, player, door),
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
    pub fn any_gets_to(
        &self,
        view: &PlayerView,
        units: &[hecs::Entity],
        squares: &[Position],
    ) -> bool {
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

    /// What units standing together do next on their way to `to`: through the door when that is
    /// shorter, walking to it first. `None` when they are there, or stand on the square of a door
    /// that is not open yet (a Plane Shift not yet cast) and wait.
    pub fn step(
        &mut self,
        view: &PlayerView,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<Option<Step>, TravelError> {
        let from = units
            .first()
            .and_then(|&u| view.unit_position(u))
            .ok_or(TravelError::NoUnits)?;
        if from == to {
            return Ok(None);
        }
        let trip = self.trip(view, units, to).ok_or(TravelError::Unreachable)?;
        Ok(match trip.door {
            Some(door) if door.entry == from => match door.passage {
                Passage::EarthGate => Some(Step::EarthGate(door.exit)),
                Passage::Plane if view.can_shift_plane(units) => Some(Step::Shift(door.exit)),
                Passage::Plane => None,
            },
            door => Some(Step::Walk(door.map_or(to, |d| d.entry))),
        })
    }
}
