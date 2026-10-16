//! Finding routes for stacks (A* over the tile grid, using the slowest unit's cost).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::city::Position;
use crate::Game;

fn neighbours(game: &Game, p: Position) -> Vec<Position> {
    let Some(map) = game.map.as_ref() else {
        return Vec::new();
    };
    let (w, h) = (map.width() as i32, map.height() as i32);
    let mut out = Vec::with_capacity(8);
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let y = p.y as i32 + dy;
            if !(0..h).contains(&y) {
                continue;
            }
            let x = (p.x as i32 + dx).rem_euclid(w);
            out.push(Position {
                plane: p.plane,
                x: x as u32,
                y: y as u32,
            });
        }
    }
    out
}

fn estimate(width: u32, a: Position, b: Position) -> u32 {
    let dx = a.x.abs_diff(b.x);
    dx.min(width - dx).max(a.y.abs_diff(b.y))
}

impl Game {
    /// Cheapest path for the stack from where it stands to `to` on the same plane,
    /// excluding the start tile. Hostile tiles other than the destination are avoided.
    /// Returns `None` if there is no way there.
    pub fn find_path(&self, units: &[hecs::Entity], to: Position) -> Option<Vec<Position>> {
        let first = *units.first()?;
        let start = self.units().unit_position(first)?;
        let owner = self.units().unit_owner(first)?;
        let width = self.map.as_ref()?.width();
        if start.plane != to.plane || start == to {
            return None;
        }
        let flies = self.economy().stack_traits(units);
        let step_cost = |from: Position, next: Position| -> Option<u32> {
            let mut worst = 0;
            for &u in units {
                worst = worst.max(self.economy().step_cost(u, from, next, flies).ok()?);
            }
            Some(worst)
        };

        let mut best: HashMap<Position, u32> = HashMap::new();
        let mut came: HashMap<Position, Position> = HashMap::new();
        let mut open = BinaryHeap::new();
        best.insert(start, 0);
        open.push(Reverse((
            estimate(width, start, to),
            0u32,
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
            if cost > *best.get(&here).unwrap_or(&u32::MAX) {
                continue;
            }
            for next in neighbours(self, here) {
                if next != to && self.economy().hostile_at(next, owner).is_some() {
                    continue;
                }
                // A guarded site stops the stack: go round unless it is the destination.
                if next != to
                    && owner != crate::units::NEUTRAL
                    && self
                        .economy()
                        .lair_at(next)
                        .is_some_and(|(_, l)| !l.guards.is_empty())
                {
                    continue;
                }
                let Some(step) = step_cost(here, next) else {
                    continue;
                };
                let total = cost + step;
                if total < *best.get(&next).unwrap_or(&u32::MAX) {
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

    /// Every square on the stack's plane that it can walk to from where it stands (the squares
    /// `find_path` would find a way to), the start included: one search instead of one per
    /// target. Hostile squares and guarded sites are not entered.
    pub fn reachable_squares(&self, units: &[hecs::Entity]) -> std::collections::HashSet<Position> {
        let mut seen = std::collections::HashSet::new();
        let (Some(&first), true) = (units.first(), self.map.is_some()) else {
            return seen;
        };
        let (Some(start), Some(owner)) = (
            self.units().unit_position(first),
            self.units().unit_owner(first),
        ) else {
            return seen;
        };
        let flies = self.economy().stack_traits(units);
        let step_cost = |from: Position, next: Position| -> Option<u32> {
            let mut worst = 0;
            for &u in units {
                worst = worst.max(self.economy().step_cost(u, from, next, flies).ok()?);
            }
            Some(worst)
        };
        let mut best: HashMap<Position, u32> = HashMap::new();
        let mut open = BinaryHeap::new();
        best.insert(start, 0);
        open.push(Reverse((0u32, start.x, start.y)));
        while let Some(Reverse((cost, x, y))) = open.pop() {
            let here = Position {
                plane: start.plane,
                x,
                y,
            };
            if cost > *best.get(&here).unwrap_or(&u32::MAX) {
                continue;
            }
            seen.insert(here);
            for next in neighbours(self, here) {
                if self.economy().hostile_at(next, owner).is_some() {
                    continue;
                }
                if owner != crate::units::NEUTRAL
                    && self
                        .economy()
                        .lair_at(next)
                        .is_some_and(|(_, l)| !l.guards.is_empty())
                {
                    continue;
                }
                let Some(step) = step_cost(here, next) else {
                    continue;
                };
                let total = cost + step;
                if total < *best.get(&next).unwrap_or(&u32::MAX) {
                    best.insert(next, total);
                    open.push(Reverse((total, next.x, next.y)));
                }
            }
        }
        seen
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
            .unwrap_or(2)
            .max(1);
        let mut left = movers
            .iter()
            .map(|&u| self.units().unit_moves_left(u))
            .min()
            .unwrap_or(0);
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
                .unwrap_or(2);
            left = left.saturating_sub(cost);
            at = next;
        }
        turns
    }
}
