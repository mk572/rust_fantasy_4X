//! Finding routes for stacks (A* over the tile grid, using the slowest unit's cost).
//!
//! Tests: in unit/movement/tests.rs (`paths_go_around_water_...`, `a_square_has_eight_neighbours_...`, `a_walker_reaches_...`, `a_path_takes_as_many_turns_...`).

pub(crate) mod grid;

use f4x_def::half_int::HalfInt;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::fast::FastSet;

use crate::city::economy::Economy;
use crate::game::Game;
use crate::map::Position;
use crate::unit::movement::FULL_MOVE;

/// See [`Game::reach_signature`].
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

/// The tables of one search, kept between searches: a search is the best known cost of each
/// square of the plane and where it came from, and a flag whether the square stops the walk.
/// A square's entry is only valid when its stamp is the search's own, so nothing is cleared
/// between two searches (the plane is a few thousand squares; two hash maps made and grown for
/// each search cost more than the search).
#[derive(Default)]
struct Scratch {
    stamp: u32,
    best_stamp: Vec<u32>,
    best: Vec<HalfInt>,
    came: Vec<u32>,
    stop_stamp: Vec<u32>,
    stop: Vec<bool>,
}

/// The `came` of a square nothing led to (the start).
const NO_SQUARE: u32 = u32::MAX;

impl Scratch {
    fn begin(&mut self, squares: usize) {
        if self.best_stamp.len() != squares {
            *self = Scratch {
                best_stamp: vec![0; squares],
                best: vec![HalfInt::ZERO; squares],
                came: vec![0; squares],
                stop_stamp: vec![0; squares],
                stop: vec![false; squares],
                stamp: 0,
            };
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.best_stamp.fill(0);
            self.stop_stamp.fill(0);
            self.stamp = 1;
        }
    }

    /// The cost found so far to reach the square; the greatest when none has been.
    fn best(&self, i: usize) -> HalfInt {
        if self.best_stamp[i] == self.stamp {
            self.best[i]
        } else {
            HalfInt::MAX
        }
    }

    fn set_best(&mut self, i: usize, cost: HalfInt) {
        self.best_stamp[i] = self.stamp;
        self.best[i] = cost;
    }

    /// The square the best way to this one came from (an index), if it was set in this search.
    fn came(&self, i: usize) -> Option<u32> {
        (self.best_stamp[i] == self.stamp && self.came[i] != NO_SQUARE).then(|| self.came[i])
    }

    fn set_came(&mut self, i: usize, from: u32) {
        self.came[i] = from;
    }

    /// Whether the square stops the walk, asked of `work` the first time in a search.
    fn blocked(&mut self, i: usize, work: impl FnOnce() -> bool) -> bool {
        if self.stop_stamp[i] != self.stamp {
            self.stop[i] = work();
            self.stop_stamp[i] = self.stamp;
        }
        self.stop[i]
    }
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = std::cell::RefCell::new(Scratch::default());
}

/// Run a search with the tables for a plane of `squares` squares: the thread's own, or fresh
/// ones when a search is already running on this thread.
fn with_scratch<R>(squares: usize, search: impl FnOnce(&mut Scratch) -> R) -> R {
    SCRATCH.with(|cell| match cell.try_borrow_mut() {
        Ok(mut sc) => {
            sc.begin(squares);
            search(&mut sc)
        }
        Err(_) => {
            let mut sc = Scratch::default();
            sc.begin(squares);
            search(&mut sc)
        }
    })
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
pub(crate) fn ring(game: &Game, p: Position) -> Ring {
    let (w, h) = game
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

pub(crate) fn neighbours(game: &Game, p: Position) -> Vec<Position> {
    ring(game, p).collect()
}

/// The least a walk between two squares costs: half a move for each square (a road step).
fn estimate(width: u32, a: Position, b: Position) -> HalfInt {
    let dx = a.x.abs_diff(b.x);
    HalfInt::from_raw(dx.min(width - dx).max(a.y.abs_diff(b.y)) as i32)
}

impl Game {
    /// Cheapest path for the stack from where it stands to `to` on the same plane,
    /// excluding the start tile. Hostile tiles other than the destination are avoided.
    /// Returns `None` if there is no way there.
    pub fn find_path(&self, units: &[hecs::Entity], to: Position) -> Option<Vec<Position>> {
        let path = self.checked_grid_path(units, to, false)?;
        // A step into a square that has no room for the stack fails, and the order is lost at its
        // edge for ever: when the path crosses one, search again without such squares. Looking at
        // the squares of the path is cheap; the set of all full squares is not, and only a path
        // like that needs it.
        if self.path_crosses_full_square(units, &path, to) {
            return self.checked_grid_path(units, to, true);
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
        let room = self.max_tile().saturating_sub(units.len());
        let mut count: Vec<(Position, usize)> =
            path.iter().filter(|&&p| p != to).map(|&p| (p, 0)).collect();
        if count.is_empty() {
            return false;
        }
        for (_, (_, at)) in self.world.query::<(&crate::unit::Unit, &Position)>().iter() {
            if let Some((_, n)) = count.iter_mut().find(|(p, _)| p == at) {
                *n += 1;
            }
        }
        count.iter().any(|&(_, n)| n > room)
    }

    /// The search of `find_path`, with the squares that are full for the stack left out or not.
    fn search_path_squares(
        &self,
        units: &[hecs::Entity],
        to: Position,
        avoid_full: bool,
    ) -> Option<Vec<Position>> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        let width = self.map.as_ref()?.width();
        let height = self.map.as_ref()?.height();
        if start.plane != to.plane || start == to {
            return None;
        }
        let flies = self.economy().stack_traits(units);
        let economy = self.economy();
        let room = economy.ship_room_squares(owner);
        let economy = economy.with_ship_room(owner, &room);
        let profiles = Economy::distinct_step_profiles(economy.step_profiles(units)?);
        let step_cost = |from: Position, next: Position| -> Option<HalfInt> {
            economy.stack_step_cost(&profiles, from, next, flies)
        };
        // Which squares stop the walk (hostile units or city, a guarded site), worked out once
        // for every square the search looks at.
        let hostile = economy.hostile_squares(owner);
        let stops = |next: Position| -> bool {
            hostile.contains(&next)
                || (owner != crate::player::MONSTERS
                    && economy.lair_at(next).is_some_and(|l| !l.guards.is_empty()))
        };
        // Squares the stack cannot enter for want of room (its own units fill them): a path
        // through one stalls at its edge for ever.
        let full = if avoid_full {
            economy.full_squares(units.len())
        } else {
            Default::default()
        };
        let at = |p: Position| p.y as usize * width as usize + p.x as usize;
        let square = |i: u32| Position {
            plane: start.plane,
            x: i % width,
            y: i / width,
        };

        with_scratch(width as usize * height as usize, |sc| {
            let mut open = BinaryHeap::new();
            sc.set_best(at(start), HalfInt::ZERO);
            sc.set_came(at(start), NO_SQUARE);
            open.push(Reverse((
                estimate(width, start, to),
                HalfInt::ZERO,
                start.x,
                start.y,
            )));
            while let Some(Reverse((_, cost, x, y))) = open.pop() {
                let here = Position {
                    plane: start.plane,
                    x,
                    y,
                };
                if here == to {
                    let mut path = vec![here];
                    let mut now = here;
                    while let Some(prev) = sc.came(at(now)) {
                        let prev = square(prev);
                        if prev == start {
                            break;
                        }
                        path.push(prev);
                        now = prev;
                    }
                    path.reverse();
                    return Some(path);
                }
                if cost > sc.best(at(here)) {
                    continue;
                }
                for next in ring(self, here) {
                    // Hostile squares and guarded sites stop the stack: go round unless it is
                    // the destination.
                    if (next != to && sc.blocked(at(next), || stops(next))) || full.contains(&next)
                    {
                        continue;
                    }
                    let Some(step) = step_cost(here, next) else {
                        continue;
                    };
                    let total = cost + step;
                    if total < sc.best(at(next)) {
                        sc.set_best(at(next), total);
                        sc.set_came(at(next), at(here) as u32);
                        open.push(Reverse((
                            total + estimate(width, next, to),
                            total,
                            next.x,
                            next.y,
                        )));
                    }
                }
            }
            None
        })
    }

    /// Every square on the stack's plane that it can walk to from where it stands (the squares
    /// `find_path` would find a way to), the start included: one search instead of one per
    /// target. Hostile squares and guarded sites are not entered.
    pub fn reachable_squares(&self, units: &[hecs::Entity]) -> FastSet<Position> {
        self.checked_grid_walk(units, &[]).0
    }

    /// Whether the stack can walk to any of `targets` (the answer of [`Game::reachable_squares`]
    /// for them, but the search stops at the first one it reaches; the start does not count
    /// unless it is among the targets).
    pub fn reaches_any(&self, units: &[hecs::Entity], targets: &[Position]) -> bool {
        !targets.is_empty() && self.checked_grid_reaches_any(units, targets)
    }

    /// Like [`Game::reaches_any`], but when no target can be reached it gives every square the
    /// stack can walk to (the whole region the search ran over; `None` when a target is found).
    pub fn unreached_region(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> Option<FastSet<Position>> {
        let (seen, found) = self.checked_grid_walk(units, targets);
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

    /// The search of `reachable_squares`: every square it can walk to, and whether one of
    /// `targets` was among them (it then stops there; with no targets it goes on to the end).
    fn walk_squares(
        &self,
        units: &[hecs::Entity],
        targets: &[Position],
    ) -> (FastSet<Position>, bool) {
        let mut seen = FastSet::default();
        let mut found = false;
        let (Some(&first), Some(map)) = (units.first(), self.map.as_ref()) else {
            return (seen, found);
        };
        let (width, height) = (map.width(), map.height());
        let (Some(start), Some(owner)) = (
            self.units().unit_position(first),
            self.units().unit_owner(first),
        ) else {
            return (seen, found);
        };
        let flies = self.economy().stack_traits(units);
        let economy = self.economy();
        let room = economy.ship_room_squares(owner);
        let economy = economy.with_ship_room(owner, &room);
        let Some(profiles) = economy.step_profiles(units) else {
            return (seen, found);
        };
        let profiles = Economy::distinct_step_profiles(profiles);
        let step_cost = |from: Position, next: Position| -> Option<HalfInt> {
            economy.stack_step_cost(&profiles, from, next, flies)
        };
        // Which squares stop the walk, worked out once for every square the search looks at.
        let hostile = economy.hostile_squares(owner);
        let stops = |next: Position| -> bool {
            hostile.contains(&next)
                || (owner != crate::player::MONSTERS
                    && economy.lair_at(next).is_some_and(|l| !l.guards.is_empty()))
        };

        let at = |p: Position| p.y as usize * width as usize + p.x as usize;
        with_scratch(width as usize * height as usize, |sc| {
            let mut open = BinaryHeap::new();
            sc.set_best(at(start), HalfInt::ZERO);
            open.push(Reverse((HalfInt::ZERO, start.x, start.y)));
            while let Some(Reverse((cost, x, y))) = open.pop() {
                let here = Position {
                    plane: start.plane,
                    x,
                    y,
                };
                if cost > sc.best(at(here)) {
                    continue;
                }
                seen.insert(here);
                if targets.contains(&here) {
                    found = true;
                    break;
                }
                for next in ring(self, here) {
                    if sc.blocked(at(next), || stops(next)) {
                        continue;
                    }
                    let Some(step) = step_cost(here, next) else {
                        continue;
                    };
                    let total = cost + step;
                    if total < sc.best(at(next)) {
                        sc.set_best(at(next), total);
                        open.push(Reverse((total, next.x, next.y)));
                    }
                }
            }
        });
        (seen, found)
    }

    /// Turns the stack needs for `path` at its slowest mover's full allowance. Land units that
    /// ride a ship of the stack (aboard it, or boarding as it sails out) cost nothing and do
    /// not slow it: the ship's speed decides.
    pub fn path_turns(&self, units: &[hecs::Entity], path: &[Position]) -> u32 {
        let Some(&first) = units.first() else {
            return 0;
        };
        let Some(mut at) = self.units().unit_position(first) else {
            return 0;
        };
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
            .max(HalfInt::from_raw(1));
        let mut left = movers
            .iter()
            .map(|&u| self.units().unit_moves_left(u))
            .min()
            .unwrap_or(HalfInt::ZERO);
        let mut turns = 0;
        for &next in path {
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
        }
        turns
    }
}

/// The searches as they were before they used flat arrays (hash maps and a vector of
/// neighbours): the reference the tests hold the new ones to, on real maps.
#[cfg(test)]
mod reference {
    use super::*;
    use crate::fast::FastMap;

    type SquareMap<V> = FastMap<Position, V>;

    impl Game {
        pub(super) fn reference_find_path(
            &self,
            units: &[hecs::Entity],
            to: Position,
        ) -> Option<Vec<Position>> {
            let first = *units.first()?;
            let start = self.units().unit_position(first)?;
            let owner = self.units().unit_owner(first)?;
            let width = self.map.as_ref()?.width();
            if start.plane != to.plane || start == to {
                return None;
            }
            let flies = self.economy().stack_traits(units);
            let economy = self.economy();
            let room = economy.ship_room_squares(owner);
            let economy = economy.with_ship_room(owner, &room);
            let profiles = economy.step_profiles(units)?;
            let step_cost = |from: Position, next: Position| -> Option<HalfInt> {
                economy.stack_step_cost(&profiles, from, next, flies)
            };
            // Which squares stop the walk (hostile units or city, a guarded site), worked out once
            // for every square the search looks at.
            let hostile = economy.hostile_squares(owner);
            let mut stops: SquareMap<bool> = SquareMap::default();
            let mut blocked = |next: Position| -> bool {
                *stops.entry(next).or_insert_with(|| {
                    hostile.contains(&next)
                        || (owner != crate::player::MONSTERS
                            && economy.lair_at(next).is_some_and(|l| !l.guards.is_empty()))
                })
            };

            let mut best: SquareMap<HalfInt> = SquareMap::default();
            let mut came: SquareMap<Position> = SquareMap::default();
            let mut open = BinaryHeap::new();
            best.insert(start, HalfInt::ZERO);
            open.push(Reverse((
                estimate(width, start, to),
                HalfInt::ZERO,
                start.x,
                start.y,
            )));
            while let Some(Reverse((_, cost, x, y))) = open.pop() {
                let here = Position {
                    plane: start.plane,
                    x,
                    y,
                };
                if here == to {
                    let mut path = vec![here];
                    let mut at = here;
                    while let Some(&prev) = came.get(&at) {
                        if prev == start {
                            break;
                        }
                        path.push(prev);
                        at = prev;
                    }
                    path.reverse();
                    return Some(path);
                }
                if cost > *best.get(&here).unwrap_or(&HalfInt::MAX) {
                    continue;
                }
                for next in neighbours(self, here) {
                    // Hostile squares and guarded sites stop the stack: go round unless it is the
                    // destination.
                    if next != to && blocked(next) {
                        continue;
                    }
                    let Some(step) = step_cost(here, next) else {
                        continue;
                    };
                    let total = cost + step;
                    if total < *best.get(&next).unwrap_or(&HalfInt::MAX) {
                        best.insert(next, total);
                        came.insert(next, here);
                        open.push(Reverse((
                            total + estimate(width, next, to),
                            total,
                            next.x,
                            next.y,
                        )));
                    }
                }
            }
            None
        }

        pub(super) fn reference_walk(
            &self,
            units: &[hecs::Entity],
            targets: &[Position],
        ) -> (FastSet<Position>, bool) {
            let mut seen = FastSet::default();
            let mut found = false;
            let (Some(&first), true) = (units.first(), self.map.is_some()) else {
                return (seen, found);
            };
            let (Some(start), Some(owner)) = (
                self.units().unit_position(first),
                self.units().unit_owner(first),
            ) else {
                return (seen, found);
            };
            let flies = self.economy().stack_traits(units);
            let economy = self.economy();
            let room = economy.ship_room_squares(owner);
            let economy = economy.with_ship_room(owner, &room);
            let Some(profiles) = economy.step_profiles(units) else {
                return (seen, found);
            };
            let step_cost = |from: Position, next: Position| -> Option<HalfInt> {
                economy.stack_step_cost(&profiles, from, next, flies)
            };
            // Which squares stop the walk, worked out once for every square the search looks at.
            let hostile = economy.hostile_squares(owner);
            let mut stops: SquareMap<bool> = SquareMap::default();
            let mut blocked = |next: Position| -> bool {
                *stops.entry(next).or_insert_with(|| {
                    hostile.contains(&next)
                        || (owner != crate::player::MONSTERS
                            && economy.lair_at(next).is_some_and(|l| !l.guards.is_empty()))
                })
            };
            let mut best: SquareMap<HalfInt> = SquareMap::default();
            let mut open = BinaryHeap::new();
            best.insert(start, HalfInt::ZERO);
            open.push(Reverse((HalfInt::ZERO, start.x, start.y)));
            while let Some(Reverse((cost, x, y))) = open.pop() {
                let here = Position {
                    plane: start.plane,
                    x,
                    y,
                };
                if cost > *best.get(&here).unwrap_or(&HalfInt::MAX) {
                    continue;
                }
                seen.insert(here);
                if targets.contains(&here) {
                    found = true;
                    break;
                }
                for next in neighbours(self, here) {
                    if blocked(next) {
                        continue;
                    }
                    let Some(step) = step_cost(here, next) else {
                        continue;
                    };
                    let total = cost + step;
                    if total < *best.get(&next).unwrap_or(&HalfInt::MAX) {
                        best.insert(next, total);
                        open.push(Reverse((total, next.x, next.y)));
                    }
                }
            }
            (seen, found)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::setup::{new_game, NewGame};
    use crate::game::testutil::shipped;
    use crate::map::Position;
    use f4x_def::testdir::TestResult;

    /// The searches give what they gave before they used flat arrays, on the real maps of a
    /// few seeds: the same paths, the same reachable sets, for several stacks and goals.
    #[test]
    fn the_searches_match_the_reference_on_real_maps() -> TestResult {
        let content = shipped();
        for seed in [3u64, 11, 25] {
            let game = new_game(
                &content,
                &NewGame {
                    seed,
                    all_ai: true,
                    ..NewGame::with_wizards(3)
                },
            )?;
            let (w, h) = {
                let map = game.map.as_ref().ok_or("no map")?;
                (map.width(), map.height())
            };
            let mut stacks: Vec<Vec<hecs::Entity>> = Vec::new();
            for player in game.player_ids().into_iter().filter(|&p| game.is_wizard(p)) {
                let units = game.units().player_units(player);
                // One unit alone, and the units standing together on the first one's square.
                if let Some(&first) = units.first() {
                    stacks.push(vec![first]);
                    let at = game.units().unit_position(first);
                    let together: Vec<_> = units
                        .iter()
                        .copied()
                        .filter(|&u| game.units().unit_position(u) == at)
                        .collect();
                    stacks.push(together);
                }
            }
            assert!(!stacks.is_empty(), "seed {seed}: no units");
            let mut found = 0;
            let plane = |x, y, from: Position| Position {
                plane: from.plane,
                x,
                y,
            };
            for stack in &stacks {
                let start = game.units().unit_position(stack[0]).ok_or("no position")?;
                let goals: Vec<Position> = (0..h)
                    .step_by(3)
                    .flat_map(|y| (0..w).step_by(5).map(move |x| (x, y)))
                    .map(|(x, y)| plane(x, y, start))
                    .collect();
                for &goal in &goals {
                    let path = game.find_path(stack, goal);
                    let reference = game.reference_find_path(stack, goal);
                    assert_eq!(
                        path.is_some(),
                        reference.is_some(),
                        "seed {seed}: a path from {start:?} to {goal:?}"
                    );
                    // Equal ways may differ; the grid's costs no more.
                    if let (Some(new), Some(old)) = (&path, &reference) {
                        assert!(
                            game.path_cost(stack, new) <= game.path_cost(stack, old),
                            "seed {seed}: path from {start:?} to {goal:?}"
                        );
                    }
                    found += usize::from(path.is_some());
                }
                assert_eq!(
                    game.grid_walk(stack, &[]),
                    game.reference_walk(stack, &[]),
                    "seed {seed}: reach of {start:?}"
                );
                for targets in goals.chunks(7).take(6) {
                    let (new, old) = (
                        game.grid_walk(stack, targets),
                        game.reference_walk(stack, targets),
                    );
                    let what = format!("seed {seed}: reach of {start:?} with targets {targets:?}");
                    assert_eq!(new.1, old.1, "{what}");
                    // A walk that stops at a target has gone over the squares that came before
                    // it, which equal ways put in either order.
                    if !new.1 {
                        assert_eq!(new.0, old.0, "{what}");
                    }
                    assert_eq!(game.grid_reaches_any(stack, targets), old.1, "{what}");
                }
            }
            assert!(found > 50, "seed {seed}: only {found} paths found");
            // A second search after the first one reuses the tables: nothing is left over.
            let stack = &stacks[0];
            let start = game.units().unit_position(stack[0]).ok_or("no position")?;
            let goal = plane(w / 2, h / 2, start);
            assert_eq!(game.find_path(stack, goal), game.find_path(stack, goal));
        }
        Ok(())
    }
}
