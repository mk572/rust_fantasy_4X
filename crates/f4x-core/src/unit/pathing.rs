//! Finding routes for stacks (A* over the tile grid, using the slowest unit's cost).
//!
//! Tests: in unit/movement/tests.rs (`paths_go_around_water_...`, `a_square_has_eight_neighbours_...`, `a_walker_reaches_...`, `a_path_takes_as_many_turns_...`).

pub mod failure;
pub(crate) mod grid;

use f4x_def::half_int::HalfInt;

use crate::fast::FastSet;

use crate::map::Position;
use crate::unit::movement::FULL_MOVE;
use crate::world::World;

/// See [`World::reach_signature`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReachSignature {
    at: Position,
    profiles: Vec<(u16, Vec<crate::rules::Ability>)>,
    traits: crate::unit::movement::StackTraits,
}

impl ReachSignature {
    /// Whether the two stacks move alike (the same units, the same traits), wherever they stand.
    pub fn moves_like(&self, other: &Self) -> bool {
        self.profiles == other.profiles && self.traits == other.traits
    }

    /// The square the stack stands on.
    pub fn at(&self) -> Position {
        self.at
    }
}

/// The squares round one, row by row from the north west (the map wraps east and west, and
/// stops at the north and south): what [`neighbours`] lists, without a vector.
pub(crate) struct Ring {
    plane: crate::map::Plane,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// The next of the nine squares of the block, 0 to 9; the middle one is skipped.
    next: u8,
}

impl Iterator for Ring {
    type Item = Position;

    fn next(&mut self) -> Option<Position> {
        while self.next < 9 {
            let (dx, dy) = (i32::from(self.next % 3) - 1, i32::from(self.next / 3) - 1);
            self.next += 1;
            let y = self.y + dy;
            if (dx == 0 && dy == 0) || !(0..self.h).contains(&y) {
                continue;
            }
            return Some(Position {
                plane: self.plane,
                x: (self.x + dx).rem_euclid(self.w) as u32,
                y: y as u32,
            });
        }
        None
    }
}

/// The squares next to `p`; none when the game has no map.
pub(crate) fn ring(world: &World, p: Position) -> Ring {
    let (w, h) = world
        .map
        .as_ref()
        .map_or((0, 0), |m| (m.width() as i32, m.height() as i32));
    Ring {
        plane: p.plane,
        x: p.x as i32,
        y: p.y as i32,
        w,
        h,
        // With no map there is nothing to give.
        next: if w == 0 { 9 } else { 0 },
    }
}

pub(crate) fn neighbours(world: &World, p: Position) -> Vec<Position> {
    ring(world, p).collect()
}

impl World {
    /// Cheapest path for the stack from where it stands to `to` on the same plane,
    /// excluding the start tile. Hostile tiles other than the destination are avoided.
    /// Returns `None` if there is no way there.
    pub fn find_path(&self, units: &[hecs::Entity], to: Position) -> Option<Vec<Position>> {
        let from = self.units().unit_position(*units.first()?)?;
        self.find_path_from(units, from, to)
    }

    /// [`World::find_path`] as if the stack stood on `from`.
    fn find_path_from(
        &self,
        units: &[hecs::Entity],
        from: Position,
        to: Position,
    ) -> Option<Vec<Position>> {
        let path = self.grid_path(units, from, to, false)?;
        // A step into a square that has no room for the stack fails, and the order is lost at its
        // edge for ever: when the path crosses one, search again without such squares. Looking at
        // the squares of the path is cheap; the set of all full squares is not, and only a path
        // like that needs it.
        if self.path_crosses_full_square(units, &path, to) {
            return self.grid_path(units, from, to, true);
        }
        Some(path)
    }

    /// Whether a square of `path` other than the destination is full for the stack.
    fn path_crosses_full_square(
        &self,
        units: &[hecs::Entity],
        path: &[Position],
        to: Position,
    ) -> bool {
        let mut count: Vec<(Position, usize)> =
            path.iter().filter(|&&p| p != to).map(|&p| (p, 0)).collect();
        if count.is_empty() {
            return false;
        }
        for (_, (_, at)) in self.ecs.query::<(&crate::unit::Unit, &Position)>().iter() {
            if let Some((_, n)) = count.iter_mut().find(|(p, _)| p == at) {
                *n += 1;
            }
        }
        count
            .iter()
            .any(|&(p, n)| n + units.len() > self.tile_limit(p))
    }

    /// Every square on the stack's plane that it can walk to from where it stands (the squares
    /// `find_path` would find a way to), the start included: one search instead of one per
    /// target. Hostile squares and guarded sites are not entered.
    pub fn reachable_squares(&self, units: &[hecs::Entity]) -> FastSet<Position> {
        self.grid_walk(units, &[]).0
    }

    /// Whether the stack can walk to any of `targets` (the answer of [`World::reachable_squares`]
    /// for them, but the search stops at the first one it reaches; the start does not count
    /// unless it is among the targets).
    pub fn reaches_any(&self, units: &[hecs::Entity], targets: &[Position]) -> bool {
        !targets.is_empty() && self.grid_reaches_any(units, targets)
    }

    /// Like [`World::reaches_any`], but when no target can be reached it gives every square the
    /// stack can walk to (the whole region the search ran over; `None` when a target is found).
    pub fn unreached_region(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> Option<FastSet<Position>> {
        let (seen, found) = self.grid_walk(units, targets);
        (!found).then_some(seen)
    }

    /// What decides which squares a stack can walk to: where it stands, the definition and
    /// abilities of each unit and what the stack gives them. Two stacks with one signature (and
    /// one owner) reach the same squares and paths, so one search serves them both.
    pub fn reach_signature(&self, units: &[hecs::Entity]) -> Option<ReachSignature> {
        let economy = self.economy();
        let mut profiles: Vec<_> = economy
            .step_profiles(units)?
            .iter()
            .map(|p| p.signature())
            .collect();
        profiles.sort();
        Some(ReachSignature {
            at: self.units().unit_position(*units.first()?)?,
            profiles,
            traits: economy.stack_traits(units),
        })
    }

    /// The way of the stack from `from` (where it stands, or any other square) to `to` on the
    /// same plane: every square after `from` with the turns that pass until the stack stands on
    /// it. Empty when `from` is `to`; `None` when there is no way. A stack that starts elsewhere
    /// than it stands has its full movement there.
    pub fn way(
        &self,
        units: &[hecs::Entity],
        from: Position,
        to: Position,
    ) -> Option<Vec<(Position, u32)>> {
        if from == to {
            return Some(Vec::new());
        }
        let path = self.find_path_from(units, from, to)?;
        let turns = self.step_turns(units, from, &path);
        Some(path.into_iter().zip(turns).collect())
    }

    /// Turns the stack needs for `path` at its slowest mover's full allowance. Land units that
    /// ride a ship of the stack (aboard it, or boarding as it sails out) cost nothing and do
    /// not slow it: the ship's speed decides.
    pub fn path_turns(&self, units: &[hecs::Entity], path: &[Position]) -> u32 {
        units
            .first()
            .and_then(|&u| self.units().unit_position(u))
            .and_then(|from| self.step_turns(units, from, path).last().copied())
            .unwrap_or(0)
    }

    /// For each square of `path`, walked from `from`, the turns that pass until the stack stands
    /// on it ([`World::path_turns`] is the last of them).
    fn step_turns(&self, units: &[hecs::Entity], from: Position, path: &[Position]) -> Vec<u32> {
        let Some(&first) = units.first() else {
            return Vec::new();
        };
        let Some(stands) = self.units().unit_position(first) else {
            return Vec::new();
        };
        let mut at = from;
        let flies = self.economy().stack_traits(units);
        let economy = self.economy();
        let sails = flies.sea_lift && path.iter().any(|&p| economy.is_sea(p));
        let movers: Vec<hecs::Entity> = units
            .iter()
            .copied()
            .filter(|&u| {
                let rides = economy.carrier_of(u).is_some_and(|c| units.contains(&c));
                let boards = sails && economy.needs_ship(u, flies.flies);
                !(rides || boards)
            })
            .collect();
        let movers = if movers.is_empty() { units } else { &movers };
        let allowance = movers
            .iter()
            .map(|&u| self.units().unit_max_moves(u))
            .min()
            .unwrap_or(FULL_MOVE)
            .max(HalfInt::HALF);
        let mut left = if from == stands {
            movers
                .iter()
                .map(|&u| self.units().unit_moves_left(u))
                .min()
                .unwrap_or(HalfInt::ZERO)
        } else {
            allowance
        };
        let mut turns = 0;
        path.iter()
            .map(|&next| {
                if left == 0 {
                    turns += 1;
                    left = allowance;
                }
                let cost = movers
                    .iter()
                    .filter_map(|&u| self.economy().step_cost(u, at, next, flies).ok())
                    .max()
                    .unwrap_or(FULL_MOVE);
                left = left.saturating_sub(cost);
                at = next;
                turns
            })
            .collect()
    }
}
