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

/// The city in a city battle stands on a square of four by four squares, defended end of the
/// field, mirrored from the original's (which has the defenders on the west). The twelve squares
/// round the edge are the walls when the city has them; the middle four are the inside.
pub const CITY_X0: i32 = 12;
pub const CITY_Y0: i32 = 10;
pub const CITY_SIZE: i32 = 4;
/// The gate in the wall, on the side facing the attackers. Nothing stops anyone passing it.
pub const GATE: Pos = (12, 12);

/// Whether a square is in the four by four city area.
pub fn in_city_area(pos: Pos) -> bool {
    (CITY_X0..CITY_X0 + CITY_SIZE).contains(&pos.0)
        && (CITY_Y0..CITY_Y0 + CITY_SIZE).contains(&pos.1)
}

/// Whether a square touches the city area from outside (the twenty squares round it).
pub fn in_city_ring(pos: Pos) -> bool {
    (CITY_X0 - 1..=CITY_X0 + CITY_SIZE).contains(&pos.0)
        && (CITY_Y0 - 1..=CITY_Y0 + CITY_SIZE).contains(&pos.1)
        && !in_city_area(pos)
}

/// Whether a square is one of the twelve round the edge of the city area (where a wall stands).
pub fn is_wall_site(pos: Pos) -> bool {
    in_city_area(pos)
        && (pos.0 == CITY_X0
            || pos.0 == CITY_X0 + CITY_SIZE - 1
            || pos.1 == CITY_Y0
            || pos.1 == CITY_Y0 + CITY_SIZE - 1)
}

/// The four corners of the wall, where towers stand: no one can enter them while they do.
pub fn is_wall_corner(pos: Pos) -> bool {
    is_wall_site(pos)
        && (pos.0 == CITY_X0 || pos.0 == CITY_X0 + CITY_SIZE - 1)
        && (pos.1 == CITY_Y0 || pos.1 == CITY_Y0 + CITY_SIZE - 1)
}

/// What stands on a wall site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wall {
    None,
    Standing,
    /// Knocked down: it can be walked over.
    Broken,
}

/// How a unit crosses the field: flying units pay one move everywhere, and some units pass
/// through the city's walls (fliers, teleporters, the merging and the non-corporeal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mover {
    pub flying: bool,
    pub through_walls: bool,
}

impl Mover {
    pub fn walker() -> Mover {
        Mover::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleMap {
    cells: Vec<Cell>,
    /// The walls of the city, indexed by the square's place in the city area; `None` outside
    /// a city battle, and in one without walls every site is `Wall::None`.
    walls: Option<[Wall; 16]>,
    /// The city is a Flying Fortress: it is shut to anyone who cannot fly or pass through walls,
    /// from the outside and from the inside alike.
    fortress: bool,
}

impl BattleMap {
    /// A map with nothing in the way.
    pub fn open() -> Self {
        Self {
            cells: vec![Cell::Open; (GRID_W * GRID_H) as usize],
            walls: None,
            fortress: false,
        }
    }

    /// Make the city a Flying Fortress (the original's `Update_Move_Map_City_Area_Restrictions`
    /// shuts every square of the city area to walkers outside, and the squares round it to
    /// walkers inside, the gate included).
    pub fn with_flying_fortress(mut self) -> Self {
        self.fortress = true;
        self
    }

    /// Whether the city is a Flying Fortress.
    pub fn is_flying_fortress(&self) -> bool {
        self.fortress
    }

    /// Put a city on the map: the city area is open ground, ringed by standing walls if
    /// `walled`.
    pub fn with_city(mut self, walled: bool) -> Self {
        for y in CITY_Y0..CITY_Y0 + CITY_SIZE {
            for x in CITY_X0..CITY_X0 + CITY_SIZE {
                self.set((x, y), Cell::Open);
            }
        }
        let mut walls = [Wall::None; 16];
        if walled {
            for y in CITY_Y0..CITY_Y0 + CITY_SIZE {
                for x in CITY_X0..CITY_X0 + CITY_SIZE {
                    if is_wall_site((x, y)) {
                        walls[Self::city_index((x, y))] = Wall::Standing;
                    }
                }
            }
        }
        self.walls = Some(walls);
        self
    }

    fn city_index(pos: Pos) -> usize {
        ((pos.1 - CITY_Y0) * CITY_SIZE + (pos.0 - CITY_X0)) as usize
    }

    /// Whether the battle is at a city.
    pub fn is_city(&self) -> bool {
        self.walls.is_some()
    }

    /// Whether the city has (or had) walls: some site has a wall that is standing or broken.
    pub fn is_walled(&self) -> bool {
        self.walls
            .is_some_and(|w| w.iter().any(|&w| w != Wall::None))
    }

    /// What stands on a square: `Wall::None` anywhere but a wall site of a walled city.
    pub fn wall(&self, pos: Pos) -> Wall {
        match &self.walls {
            Some(w) if in_city_area(pos) => w[Self::city_index(pos)],
            _ => Wall::None,
        }
    }

    /// Whether any section of the wall has been knocked down.
    pub fn is_breached(&self) -> bool {
        self.walls.is_some_and(|w| w.contains(&Wall::Broken))
    }

    /// A standing wall that can be attacked.
    pub fn has_wall(&self, pos: Pos) -> bool {
        self.wall(pos) == Wall::Standing
    }

    /// Knock a wall down. Does nothing where no wall stands.
    pub fn break_wall(&mut self, pos: Pos) {
        if self.has_wall(pos) {
            if let Some(w) = self.walls.as_mut() {
                w[Self::city_index(pos)] = Wall::Broken;
            }
        }
    }

    /// The standing wall sites, for drawing and for the units that attack walls.
    pub fn standing_walls(&self) -> Vec<Pos> {
        let mut out = Vec::new();
        for y in CITY_Y0..CITY_Y0 + CITY_SIZE {
            for x in CITY_X0..CITY_X0 + CITY_SIZE {
                if self.has_wall((x, y)) {
                    out.push((x, y));
                }
            }
        }
        out
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

    /// Half-moves to enter `pos`, or `None` if the unit cannot. (The walls of a city are not
    /// counted: see [`BattleMap::reachable_as`].)
    pub fn enter_cost(&self, pos: Pos, flying: bool) -> Option<u32> {
        match (self.get(pos)?, flying) {
            (_, true) => Some(OPEN_COST),
            (Cell::Open, false) => Some(OPEN_COST),
            (Cell::Rough, false) => Some(ROUGH_COST),
            (Cell::Blocked, false) => None,
        }
    }

    /// Whether the walls of the city shut `pos` to a unit that starts its move `inside` the
    /// city area, as the original's `Update_Move_Map_City_Area_Restrictions` does:
    /// - the standing corner towers are shut to everyone;
    /// - for the others, a unit outside cannot enter a standing wall (the gate is no wall), and
    ///   a unit inside cannot step out to a square beside a standing wall (only by the gate).
    fn shut_by_walls(&self, pos: Pos, mover: Mover, inside: bool) -> bool {
        if self.fortress {
            if mover.flying || mover.through_walls {
                return false;
            }
            // Walkers outside cannot enter the city area; walkers inside cannot step out to
            // any square round it.
            return if inside {
                !in_city_area(pos) && in_city_ring(pos)
            } else {
                in_city_area(pos)
            };
        }
        if !self.is_walled() {
            return false;
        }
        if is_wall_corner(pos) && self.has_wall(pos) {
            return true;
        }
        if mover.flying || mover.through_walls {
            return false;
        }
        if !inside {
            return pos != GATE && self.has_wall(pos);
        }
        // Inside: the squares just outside any standing wall (not the gate's) are shut.
        if in_city_area(pos) {
            return false;
        }
        let (x1, x2) = (CITY_X0, CITY_X0 + CITY_SIZE - 1);
        let (y1, y2) = (CITY_Y0, CITY_Y0 + CITY_SIZE - 1);
        for y in y1..=y2 {
            for x in x1..=x2 {
                let site = (x, y);
                if !is_wall_site(site) || site == GATE || !self.has_wall(site) {
                    continue;
                }
                let mut outside = Vec::new();
                if x == x1 {
                    outside.push((x1 - 1, y));
                }
                if x == x2 {
                    outside.push((x2 + 1, y));
                }
                if y == y1 {
                    outside.push((x, y1 - 1));
                }
                if y == y2 {
                    outside.push((x, y2 + 1));
                }
                if is_wall_corner(site) {
                    outside.push((
                        if x == x1 { x1 - 1 } else { x2 + 1 },
                        if y == y1 { y1 - 1 } else { y2 + 1 },
                    ));
                }
                if outside.contains(&pos) {
                    return true;
                }
            }
        }
        false
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
        let mover = Mover {
            flying,
            through_walls: flying,
        };
        self.reachable_as(from, points, mover, occupied)
    }

    /// Like [`BattleMap::reachable`], for a unit that also meets the city's walls.
    pub fn reachable_as(
        &self,
        from: Pos,
        points: u32,
        mover: Mover,
        occupied: &dyn Fn(Pos) -> bool,
    ) -> HashMap<Pos, (u32, Pos)> {
        let inside = in_city_area(from);
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
                    let Some(step) = self.enter_cost(next, mover.flying) else {
                        continue;
                    };
                    if occupied(next) || self.shut_by_walls(next, mover, inside) {
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

/// Squares where a side's units start in a city battle, as the original deploys them
/// (`Deploy_Battle_Units`, mirrored): the defenders inside the city in three columns of four
/// (the gate's first), skipping the wall's corner towers when there are walls, the attackers in
/// three columns of four outside. The first `count` places in order.
pub fn city_deployment(attacker: bool, count: usize, walled: bool) -> Vec<Pos> {
    // Rows from the front column back, and within a column the original's order of rows.
    let (columns, rows): ([i32; 3], [i32; 4]) = if attacker {
        ([6, 5, 4], [12, 11, 13, 10])
    } else {
        ([CITY_X0, CITY_X0 + 1, CITY_X0 + 2], [12, 11, 13, 10])
    };
    let mut out = Vec::new();
    for &x in &columns {
        for &y in &rows {
            if !attacker && walled && is_wall_corner((x, y)) {
                continue;
            }
            out.push((x, y));
        }
    }
    out.truncate(count);
    out
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
mod tests;
