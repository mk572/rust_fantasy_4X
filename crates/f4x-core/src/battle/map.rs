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
}

impl BattleMap {
    /// A map with nothing in the way.
    pub fn open() -> Self {
        Self {
            cells: vec![Cell::Open; (GRID_W * GRID_H) as usize],
            walls: None,
        }
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

    fn walled() -> BattleMap {
        BattleMap::open().with_city(true)
    }

    fn walker() -> Mover {
        Mover::walker()
    }

    fn flier() -> Mover {
        Mover {
            flying: true,
            through_walls: true,
        }
    }

    #[test]
    fn a_city_has_twelve_wall_sites_round_four_inner_squares() {
        let m = walled();
        assert!(m.is_city() && m.is_walled());
        assert_eq!(m.standing_walls().len(), 12);
        assert!(
            m.has_wall((12, 12)),
            "the gate square has a wall section too"
        );
        assert!(!m.has_wall((13, 11)), "the inside has none");
        assert!(is_wall_corner((15, 13)) && !is_wall_corner((15, 12)));
        // No walls: a city, but nothing to break or stand behind.
        let open = BattleMap::open().with_city(false);
        assert!(open.is_city() && !open.is_walled());
        assert!(open.standing_walls().is_empty());
        assert!(!BattleMap::open().is_city());
    }

    #[test]
    fn walkers_outside_cannot_enter_a_wall_except_by_the_gate() {
        let m = walled();
        let r = m.reachable_as((10, 12), 20, walker(), &none);
        // The gate is open, and from it the inside.
        assert!(r.contains_key(&GATE));
        assert!(r.contains_key(&(13, 12)), "inside, by the gate");
        // Every other wall section is shut, as are the corner towers.
        for w in m.standing_walls() {
            assert_eq!(r.contains_key(&w), w == GATE, "{w:?}");
        }
        // Around the walls the field is open.
        assert!(r.contains_key(&(11, 10)) && r.contains_key(&(16, 9)));
    }

    #[test]
    fn a_broken_wall_can_be_walked_over() {
        let mut m = walled();
        assert!(!m.is_breached());
        let to = (12, 11);
        assert!(!m
            .reachable_as((11, 11), 4, walker(), &none)
            .contains_key(&to));
        m.break_wall(to);
        assert!(m.is_breached());
        assert!(m
            .reachable_as((11, 11), 4, walker(), &none)
            .contains_key(&to));
        // Breaking a wall that is not there does nothing.
        m.break_wall((13, 11));
        assert_eq!(m.wall((13, 11)), Wall::None);
        assert_eq!(m.standing_walls().len(), 11);
    }

    #[test]
    fn fliers_pass_the_wall_but_nobody_enters_a_corner_tower() {
        let m = walled();
        let r = m.reachable_as((10, 11), 6, flier(), &none);
        assert!(r.contains_key(&(12, 11)), "over the wall");
        assert!(r.contains_key(&(13, 11)));
        assert!(
            !r.contains_key(&(12, 10)) && !r.contains_key(&(12, 13)),
            "the towers"
        );
        let teleporter = Mover {
            flying: false,
            through_walls: true,
        };
        let r = m.reachable_as((10, 11), 6, teleporter, &none);
        assert!(r.contains_key(&(12, 11)));
        assert!(!r.contains_key(&(12, 10)));
        // A broken tower can be entered.
        let mut m = m;
        m.break_wall((12, 10));
        assert!(m
            .reachable_as((10, 10), 4, walker(), &none)
            .contains_key(&(12, 10)));
    }

    #[test]
    fn a_unit_inside_can_only_leave_by_the_gate() {
        let m = walled();
        // Standing at the gate, it may go out; from the inside it may not step through a wall.
        let inside = m.reachable_as((13, 12), 20, walker(), &none);
        assert!(inside.contains_key(&(11, 12)), "out of the gate");
        assert!(inside.contains_key(&(10, 12)));
        // The squares beside the wall's outside are shut to it, except in front of the gate.
        assert!(!inside.contains_key(&(11, 11)) && !inside.contains_key(&(11, 13)));
        assert!(
            !inside.contains_key(&(16, 12)),
            "the far side is walled off"
        );
        // Standing on the wall at the side it can walk the wall but not step off it.
        let on_wall = m.reachable_as((14, 10), 4, walker(), &none);
        assert!(on_wall.contains_key(&(13, 10)));
        assert!(!on_wall.contains_key(&(14, 9)));
        // With a breach it can leave there.
        let mut m = m;
        m.break_wall((14, 10));
        let on_wall = m.reachable_as((14, 11), 4, walker(), &none);
        assert!(on_wall.contains_key(&(14, 9)), "out through the breach");
        assert!(
            !on_wall.contains_key(&(13, 9)),
            "other sections still stand"
        );
    }

    #[test]
    fn city_battles_deploy_the_defenders_inside_and_the_attackers_outside() {
        let def = city_deployment(false, 12, true);
        assert_eq!(def.len(), 10, "the two corner towers are skipped");
        assert_eq!(def[0], GATE, "the first defender holds the gate");
        assert!(def.iter().all(|&p| in_city_area(p) && !is_wall_corner(p)));
        let open = city_deployment(false, 12, false);
        assert_eq!(open.len(), 12);
        let att = city_deployment(true, 9, true);
        assert_eq!(att.len(), 9);
        assert!(att.iter().all(|&p| !in_city_area(p) && p.0 < CITY_X0));
        let mut all = att.clone();
        all.extend(def);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 19, "nobody shares a square");
    }
}
