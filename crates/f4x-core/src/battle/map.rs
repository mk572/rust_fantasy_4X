//! The tactical map of a battle: a 21 by 22 grid of squares, of which the original plays on the
//! 200 of its screen's band.
//!
//! Units move in eight directions. Open ground costs one move (two half-moves), rough ground
//! two moves, a road half a move, trees and rocks add half a move each (to four at most),
//! squares outside the band and a lair's structure cannot be entered, and flying units pay
//! one move everywhere and cross everything but those. Ships sail the ocean and rivers (two
//! half-moves a square) and swimmers take the cheaper of the two ways. The ground comes from
//! [`layout`], the port of the original's field generator.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::content::TerrainClass;

pub mod layout;

use layout::{Layout, Look, Set, Spec};
use rand::Rng;

pub const GRID_W: i32 = 21;
pub const GRID_H: i32 = 22;

/// Half-moves to enter an open square.
pub const OPEN_COST: u32 = 2;
/// Half-moves to enter rough ground.
pub const ROUGH_COST: u32 = 4;
/// Half-moves to enter mud (Earth to Mud): the original's 12, which is more than any unit has.
pub const MUD_COST: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Open,
    /// Forest, swamp, rubble: slows walkers.
    Rough,
    /// An obstacle walkers cannot enter but fliers pass (made by hand, in tests).
    Blocked,
    /// Outside the band the original plays on, or a lair's structure: nobody can enter.
    Void,
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
    /// A ship: it sails the squares of water and nothing else.
    pub sails: bool,
    /// A swimmer: it takes the cheaper of the walkers' and the ships' cost of a square.
    pub swims: bool,
    pub through_walls: bool,
    /// Mud does not slow it: the non-corporeal (fliers pass over it anyway).
    pub ignores_mud: bool,
}

impl Mover {
    pub fn walker() -> Mover {
        Mover::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleMap {
    cells: Vec<Cell>,
    /// Half-moves to enter each square on foot (`layout::INF` for none).
    costs: Vec<u8>,
    /// Half-moves to enter each square by ship (`layout::INF` for none).
    sail: Vec<u8>,
    /// What the screen draws, in the original's frame; `None` on a map made by hand.
    look: Option<Box<Look>>,
    /// The walls of the city, indexed by the square's place in the city area; `None` outside
    /// a city battle, and in one without walls every site is `Wall::None`.
    walls: Option<[Wall; 16]>,
    /// The city is a Flying Fortress: it is shut to anyone who cannot fly or pass through walls,
    /// from the outside and from the inside alike.
    fortress: bool,
    /// Squares Earth to Mud turned to mud.
    mud: Vec<bool>,
}

impl BattleMap {
    /// A map with nothing in the way.
    pub fn open() -> Self {
        Self {
            cells: vec![Cell::Open; (GRID_W * GRID_H) as usize],
            costs: vec![OPEN_COST as u8; (GRID_W * GRID_H) as usize],
            sail: vec![layout::INF; (GRID_W * GRID_H) as usize],
            look: None,
            walls: None,
            fortress: false,
            mud: vec![false; (GRID_W * GRID_H) as usize],
        }
    }

    /// Whether the square is mud.
    pub fn is_mud(&self, pos: Pos) -> bool {
        Self::in_bounds(pos) && self.mud[Self::index(pos)]
    }

    /// Turn the square to mud if it is open ground (grass or dirt in the original; forest,
    /// rocks and the like stay as they are). Returns whether it changed.
    pub fn make_mud(&mut self, pos: Pos) -> bool {
        if self.get(pos) == Some(Cell::Open) && !self.is_mud(pos) {
            self.mud[Self::index(pos)] = true;
            return true;
        }
        false
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

    /// Put a city on the map, ringed by standing walls if `walled`. The ground of the city area
    /// stays as it is (the field generator keeps it clear).
    pub fn with_city(mut self, walled: bool) -> Self {
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

    /// The map for a field built by [`layout::generate`]. The layout is in the original's frame;
    /// the rules use the mirrored one (`x = 20 - cgx`).
    pub fn from_layout(layout: Layout) -> Self {
        let mut map = Self::open();
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let (cgx, cgy) = (GRID_W - 1 - x, y);
                let cost = layout.cost_at(cgx, cgy);
                let i = Self::index((x, y));
                map.costs[i] = cost;
                map.sail[i] = layout.sail_at(cgx, cgy);
                map.cells[i] = match cost {
                    _ if layout.is_void(cgx, cgy) => Cell::Void,
                    // Ocean: walkers cannot enter it, ships and fliers can.
                    layout::INF => Cell::Blocked,
                    c if c > OPEN_COST as u8 => Cell::Rough,
                    _ => Cell::Open,
                };
            }
        }
        map.look = Some(Box::new(layout.look));
        map
    }

    /// A map for the overland terrain a fight happens on: a field of that kind, with nothing on
    /// it. For tests; the game builds its field from the whole square (`Layout`).
    pub fn generate(terrain: Option<TerrainClass>, rng: &mut impl Rng) -> Self {
        Self::from_layout(layout::generate(&Spec::open(set_of(terrain)), rng))
    }

    /// What the screen draws, if the map was generated.
    pub fn look(&self) -> Option<&Look> {
        self.look.as_deref()
    }

    /// Whether a square is in the band the original plays on (always true on a map made by
    /// hand).
    pub fn is_playable(&self, pos: Pos) -> bool {
        self.get(pos).is_some_and(|c| c != Cell::Void)
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
            let i = Self::index(pos);
            self.cells[i] = cell;
            self.costs[i] = match cell {
                Cell::Open => OPEN_COST as u8,
                Cell::Rough => ROUGH_COST as u8,
                Cell::Blocked | Cell::Void => layout::INF,
            };
            self.sail[i] = layout::INF;
        }
    }

    /// Half-moves to enter `pos`, or `None` if the unit cannot. (The walls of a city are not
    /// counted: see [`BattleMap::reachable_as`].)
    pub fn enter_cost(&self, pos: Pos, flying: bool) -> Option<u32> {
        match (self.get(pos)?, flying) {
            (Cell::Void, _) => None,
            (_, true) => Some(OPEN_COST),
            (Cell::Blocked, false) => None,
            _ => Some(u32::from(self.costs[Self::index(pos)])),
        }
    }

    /// Half-moves for `mover` to enter `pos`, or `None` if it cannot: fliers pay two anywhere
    /// they can go, ships the sailing cost, swimmers the lesser of that and the walking one.
    pub fn enter_cost_as(&self, pos: Pos, mover: Mover) -> Option<u32> {
        let cell = self.get(pos)?;
        if cell == Cell::Void {
            return None;
        }
        if mover.flying {
            return Some(OPEN_COST);
        }
        let i = Self::index(pos);
        let (walk, sail) = (self.costs[i], self.sail[i]);
        let cost = if mover.swims {
            walk.min(sail)
        } else if mover.sails {
            sail
        } else {
            walk
        };
        (cost != layout::INF).then_some(u32::from(cost))
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
            ..Mover::default()
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
                    let Some(mut step) = self.enter_cost_as(next, mover) else {
                        continue;
                    };
                    if self.is_mud(next) && !mover.flying && !mover.ignores_mud {
                        step = MUD_COST;
                    }
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

/// The original's combat terrain set for an overland terrain class: swamp and grassland are
/// plains, and open water has no ground of its own.
pub fn set_of(terrain: Option<TerrainClass>) -> Set {
    match terrain {
        Some(TerrainClass::Forest | TerrainClass::NatureNode) => Set::Forest,
        Some(TerrainClass::Hills) => Set::Hills,
        Some(TerrainClass::Mountains | TerrainClass::ChaosNode) => Set::Mountains,
        Some(TerrainClass::Desert) => Set::Desert,
        Some(TerrainClass::Tundra) => Set::Tundra,
        Some(TerrainClass::Swamp | TerrainClass::Grassland | TerrainClass::SorceryNode) => {
            Set::Plains
        }
        Some(TerrainClass::Ocean) | None => Set::Water,
    }
}

/// Squares where a side's units start in a city battle, as the original deploys them
/// (`Deploy_Battle_Units`, mirrored): the defenders inside the city in three columns of four
/// (the gate's first), skipping the wall's corner towers when there are walls, the attackers in
/// three columns of four outside. At a wizard's fortress the defenders also keep off the square
/// the fortress stands on. The first `count` places in order.
pub fn city_deployment(attacker: bool, count: usize, walled: bool, fortress: bool) -> Vec<Pos> {
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
            if !attacker && fortress && (x, y) == STRUCTURE {
                continue;
            }
            out.push((x, y));
        }
    }
    out.truncate(count);
    out
}

/// Squares where a side's `count` units start outside a city: the same block of three columns
/// of four as at a city (the original deploys every battle alike), then two columns further
/// back for a stack beyond twelve. The defenders keep off the square of a lair's structure.
pub fn deployment(attacker: bool, count: usize) -> Vec<Pos> {
    let columns: [i32; 5] = if attacker {
        [6, 5, 4, 3, 2]
    } else {
        [12, 13, 14, 15, 16]
    };
    let mut out = Vec::new();
    for &x in &columns {
        for y in [12, 11, 13, 10] {
            if !attacker && (x, y) == STRUCTURE {
                continue;
            }
            out.push((x, y));
        }
    }
    out.truncate(count);
    out
}

/// The square of a lair's structure, an outpost's or a tower's, in the rules' frame.
pub const STRUCTURE: Pos = (GRID_W - 1 - layout::STRUCTURE.0, layout::STRUCTURE.1);

/// Distance in squares (diagonals count as one).
pub fn distance(a: Pos, b: Pos) -> u32 {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

#[cfg(test)]
mod tests;
