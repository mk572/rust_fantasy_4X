//! The tactical map of a battle: a 21 by 22 grid of squares with obstacles and rough ground.
//!
//! Units move in eight directions. Open ground costs one move (two half-moves), rough ground
//! two moves, blocked squares (trees, rocks, water) cannot be entered, and flying units pay
//! one move everywhere and cross everything but other units.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use rand::{Rng, RngCore};

use crate::content::TerrainClass;

pub const GRID_W: i32 = 21;
pub const GRID_H: i32 = 22;

/// Half-moves to enter an open square.
pub const OPEN_COST: u32 = 2;
/// Half-moves to enter rough ground.
pub const ROUGH_COST: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Open,
    /// Forest, swamp, rubble: slows walkers.
    Rough,
    /// Trees, rocks: walkers cannot enter.
    Blocked,
}

pub type Pos = (i32, i32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleMap {
    cells: Vec<Cell>,
}

impl BattleMap {
    /// A map with nothing in the way.
    pub fn open() -> Self {
        Self {
            cells: vec![Cell::Open; (GRID_W * GRID_H) as usize],
        }
    }

    /// A map shaped by the overland terrain the fight happens on. The deployment columns
    /// at both edges stay open so every unit has a place to stand.
    pub fn generate(terrain: Option<TerrainClass>, rng: &mut dyn RngCore) -> Self {
        // Percent of squares that are (rough, blocked) per terrain.
        let (rough, blocked) = match terrain {
            Some(TerrainClass::Forest) => (25, 12),
            Some(TerrainClass::Hills) => (20, 6),
            Some(TerrainClass::Mountains) => (15, 15),
            Some(TerrainClass::Swamp) => (30, 3),
            Some(TerrainClass::Tundra) => (8, 4),
            Some(TerrainClass::Desert) => (4, 3),
            Some(TerrainClass::Grassland) => (3, 4),
            Some(TerrainClass::Ocean) | None => (0, 0),
        };
        let mut map = Self::open();
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                if !(4..=GRID_W - 5).contains(&x) {
                    continue;
                }
                let roll = rng.gen_range(0..100);
                let cell = if roll < blocked {
                    Cell::Blocked
                } else if roll < blocked + rough {
                    Cell::Rough
                } else {
                    Cell::Open
                };
                map.set((x, y), cell);
            }
        }
        map
    }

    pub fn in_bounds(pos: Pos) -> bool {
        (0..GRID_W).contains(&pos.0) && (0..GRID_H).contains(&pos.1)
    }

    fn index(pos: Pos) -> usize {
        (pos.1 * GRID_W + pos.0) as usize
    }

    pub fn get(&self, pos: Pos) -> Option<Cell> {
        Self::in_bounds(pos).then(|| self.cells[Self::index(pos)])
    }

    pub fn set(&mut self, pos: Pos, cell: Cell) {
        if Self::in_bounds(pos) {
            self.cells[Self::index(pos)] = cell;
        }
    }

    /// Half-moves to enter `pos`, or `None` if the unit cannot.
    pub fn enter_cost(&self, pos: Pos, flying: bool) -> Option<u32> {
        match (self.get(pos)?, flying) {
            (_, true) => Some(OPEN_COST),
            (Cell::Open, false) => Some(OPEN_COST),
            (Cell::Rough, false) => Some(ROUGH_COST),
            (Cell::Blocked, false) => None,
        }
    }

    /// Squares reachable from `from` with `points` half-moves, with the cheapest cost and the
    /// square each was reached from. `occupied` squares cannot be entered.
    pub fn reachable(
        &self,
        from: Pos,
        points: u32,
        flying: bool,
        occupied: &dyn Fn(Pos) -> bool,
    ) -> HashMap<Pos, (u32, Pos)> {
        let mut best: HashMap<Pos, (u32, Pos)> = HashMap::new();
        let mut queue = BinaryHeap::new();
        best.insert(from, (0, from));
        queue.push(Reverse((0u32, from)));
        while let Some(Reverse((cost, at))) = queue.pop() {
            if best.get(&at).is_some_and(|&(c, _)| c < cost) {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = (at.0 + dx, at.1 + dy);
                    let Some(step) = self.enter_cost(next, flying) else {
                        continue;
                    };
                    if occupied(next) {
                        continue;
                    }
                    let total = cost + step;
                    // A unit with any move left may take the step, as on the overland map.
                    if cost >= points || best.get(&next).is_some_and(|&(c, _)| c <= total) {
                        continue;
                    }
                    best.insert(next, (total, at));
                    queue.push(Reverse((total, next)));
                }
            }
        }
        best
    }
}

/// Squares where a side's `count` units start: attackers on the west edge, defenders on the
/// east, in a block centred on the middle rows.
pub fn deployment(attacker: bool, count: usize) -> Vec<Pos> {
    let columns: [i32; 3] = if attacker {
        [3, 2, 1]
    } else {
        [GRID_W - 4, GRID_W - 3, GRID_W - 2]
    };
    let mut out = Vec::new();
    let mid = GRID_H / 2;
    for i in 0..count {
        let col = columns[i / 9 % 3];
        let k = (i % 9) as i32;
        // 0, +1, -1, +2, -2, ... around the middle row.
        let offset = if k % 2 == 0 { k / 2 } else { -(k + 1) / 2 };
        out.push((col, mid + offset));
    }
    out
}

/// Distance in squares (diagonals count as one).
pub fn distance(a: Pos, b: Pos) -> u32 {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;

    fn none(_: Pos) -> bool {
        false
    }

    #[test]
    fn the_map_has_the_original_size() {
        let m = BattleMap::open();
        assert_eq!((GRID_W, GRID_H), (21, 22));
        assert_eq!(m.get((0, 0)), Some(Cell::Open));
        assert_eq!(m.get((20, 21)), Some(Cell::Open));
        assert_eq!(m.get((21, 0)), None);
        assert_eq!(m.get((0, -1)), None);
    }

    #[test]
    fn walkers_pay_by_ground_and_flyers_pay_one_move_everywhere() {
        let mut m = BattleMap::open();
        m.set((5, 5), Cell::Rough);
        m.set((6, 5), Cell::Blocked);
        assert_eq!(m.enter_cost((4, 5), false), Some(2));
        assert_eq!(m.enter_cost((5, 5), false), Some(4));
        assert_eq!(m.enter_cost((6, 5), false), None);
        assert_eq!(m.enter_cost((5, 5), true), Some(2));
        assert_eq!(m.enter_cost((6, 5), true), Some(2));
        assert_eq!(m.enter_cost((-1, 5), true), None, "nobody leaves the map");
    }

    #[test]
    fn reach_follows_move_points_diagonals_and_obstacles() {
        let mut m = BattleMap::open();
        // Two moves = four half-moves: two squares in any direction, diagonals included.
        let r = m.reachable((10, 10), 4, false, &none);
        assert_eq!(r.get(&(12, 12)).map(|c| c.0), Some(4));
        assert!(!r.contains_key(&(13, 10)));
        // A wall of rocks forces a detour (or stops the unit).
        for y in 8..=12 {
            m.set((11, y), Cell::Blocked);
        }
        let r = m.reachable((10, 10), 4, false, &none);
        assert!(
            !r.contains_key(&(12, 10)),
            "cannot cross the rocks in two moves"
        );
        // Flyers pass over them.
        let r = m.reachable((10, 10), 4, true, &none);
        assert!(r.contains_key(&(12, 10)));
        // Occupied squares block the way.
        let m = BattleMap::open();
        let blocked = |p: Pos| p == (11, 10);
        let r = m.reachable((10, 10), 2, false, &blocked);
        assert!(!r.contains_key(&(11, 10)));
        assert!(r.contains_key(&(11, 11)));
        assert!(r.contains_key(&(9, 10)));
    }

    #[test]
    fn rough_ground_costs_double_and_a_last_move_may_overrun() {
        let mut m = BattleMap::open();
        m.set((11, 10), Cell::Rough);
        // One move (two half-moves) is enough to step into rough ground, ending at zero.
        let r = m.reachable((10, 10), 2, false, &none);
        assert_eq!(r.get(&(11, 10)).map(|c| c.0), Some(4));
        // But nothing lies beyond it.
        assert!(!r.contains_key(&(12, 10)));
    }

    #[test]
    fn the_generated_map_keeps_the_edges_open_and_follows_the_terrain() {
        let count = |t, seed| {
            let mut r = Pcg64Mcg::seed_from_u64(seed);
            let m = BattleMap::generate(t, &mut r);
            for y in 0..GRID_H {
                for x in [0, 1, 2, 3, GRID_W - 4, GRID_W - 1] {
                    assert_eq!(m.get((x, y)), Some(Cell::Open));
                }
            }
            let mut rough = 0;
            let mut blocked = 0;
            for y in 0..GRID_H {
                for x in 0..GRID_W {
                    match m.get((x, y)).unwrap() {
                        Cell::Rough => rough += 1,
                        Cell::Blocked => blocked += 1,
                        Cell::Open => {}
                    }
                }
            }
            (rough, blocked)
        };
        assert_eq!(count(None, 1), (0, 0), "no terrain, no obstacles");
        let forest: i32 = (0..20)
            .map(|s| count(Some(TerrainClass::Forest), s).0)
            .sum();
        let grass: i32 = (0..20)
            .map(|s| count(Some(TerrainClass::Grassland), s).0)
            .sum();
        assert!(forest > grass * 3, "{forest} vs {grass}");
        let mountains: i32 = (0..20)
            .map(|s| count(Some(TerrainClass::Mountains), s).1)
            .sum();
        assert!(mountains > 0);
        let a = BattleMap::generate(Some(TerrainClass::Hills), &mut Pcg64Mcg::seed_from_u64(5));
        let b = BattleMap::generate(Some(TerrainClass::Hills), &mut Pcg64Mcg::seed_from_u64(5));
        assert_eq!(a, b);
    }

    #[test]
    fn deployment_blocks_are_on_opposite_edges_and_never_overlap() {
        let att = deployment(true, 9);
        let def = deployment(false, 9);
        assert_eq!(att.len(), 9);
        assert!(att
            .iter()
            .all(|p| p.0 < GRID_W / 2 && BattleMap::in_bounds(*p)));
        assert!(def
            .iter()
            .all(|p| p.0 > GRID_W / 2 && BattleMap::in_bounds(*p)));
        let mut seen = att.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 9);
        assert_eq!(deployment(true, 1), vec![(3, 11)]);
        assert_eq!(distance((0, 0), (3, 2)), 3);
    }
}
