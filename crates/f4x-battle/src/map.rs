//! The tactical map of a battle: a 21 by 22 grid of squares, of which the original plays on the
//! 200 of its screen's band.
//!
//! Units move in eight directions. Open ground costs one move, rough ground
//! two moves, a road half a move, trees and rocks add half a move each (to four at most),
//! squares outside the band and a lair's structure cannot be entered, and flying units pay
//! one move everywhere and cross everything but those. Ships sail the ocean and rivers (one
//! move a square) and swimmers take the cheaper of the two ways. The ground comes from
//! [`layout`], the port of the original's field generator.
//!
//! Tests: in battle/map/tests.rs.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use f4x_def::map::TerrainClass;

pub mod layout;

use f4x_def::half_int::HalfInt;
use layout::{Layout, Look, Set, Spec};
use rand::Rng;

use crate::unit::BattleUnit;

/// Moves to jump the square of water between two ships' decks.
pub const JUMP_COST: HalfInt = HalfInt::from_int(2);

/// What a diagonal step costs beyond the square's own cost: half a move.
pub const DIAGONAL_EXTRA: HalfInt = HalfInt::HALF;
/// The field is 24 by 24 squares, all of them playable (task 735). The original's is 21 by 22 with
/// its corners shut (`layout::W`, `layout::H`); its squares keep their places in the core's frame
/// (`x = 20 - cgx`, `y = cgy`) and the new ones lie beyond them: columns 21 to 23 (`cgx` -1 to -3) and
/// rows 22 and 23.
pub const GRID_W: i32 = 24;
pub const GRID_H: i32 = 24;

/// The original's cell (`cgx`, `cgy`) of a square of the field; `cgx` is negative on the columns the
/// original does not have.
pub const fn cg_of(pos: Pos) -> (i32, i32) {
    (layout::W - 1 - pos.0, pos.1)
}

/// The square of the field the original's cell `(cgx, cgy)` is.
pub const fn pos_of_cg(cg: (i32, i32)) -> Pos {
    (layout::W - 1 - cg.0, cg.1)
}

/// Moves to enter an open square.
pub const OPEN_COST: HalfInt = HalfInt::from_int(1);
/// Moves to enter rough ground.
pub const ROUGH_COST: HalfInt = HalfInt::from_int(2);
/// Moves to enter mud (Earth to Mud): 6, which is more than any unit has.
pub const MUD_COST: HalfInt = HalfInt::from_int(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Cell {
    Open,
    /// Forest, swamp, rubble: slows walkers.
    Rough,
    /// An obstacle walkers cannot enter but fliers pass (made by hand, in tests).
    Blocked,
    /// Outside the band the battle is played on, or a lair's structure: nobody can enter.
    Void,
}

pub type Pos = (i32, i32);

/// The city in a city battle stands on an area of four by four squares (`layout::MIN_CGX_CITY` and
/// the like: the houses), ringed by the squares of its wall, so the city area with its walls is six by
/// six squares, defended end of the field (task 733). The twenty squares round the edge of the six by
/// six are the walls when the city has them; the sixteen inside are the city.
pub const CITY_X0: i32 = 11;
pub const CITY_Y0: i32 = 9;
pub const CITY_SIZE: i32 = 6;

/// Whether a square is in the six by six city area, walls included.
pub fn in_city_area(pos: Pos) -> bool {
    (CITY_X0..CITY_X0 + CITY_SIZE).contains(&pos.0)
        && (CITY_Y0..CITY_Y0 + CITY_SIZE).contains(&pos.1)
}

/// Whether a square touches the city area from outside (the twenty-eight squares round it).
pub fn in_city_ring(pos: Pos) -> bool {
    (CITY_X0 - 1..=CITY_X0 + CITY_SIZE).contains(&pos.0)
        && (CITY_Y0 - 1..=CITY_Y0 + CITY_SIZE).contains(&pos.1)
        && !in_city_area(pos)
}

/// Whether a square is one of the twenty round the edge of the city area (where a wall stands).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BattleMap {
    cells: Vec<Cell>,
    /// The moves to enter each square on foot (`None` for none).
    costs: Vec<Option<HalfInt>>,
    /// The moves to enter each square by ship (`None` for none).
    sail: Vec<Option<HalfInt>>,
    /// What the screen draws, in the original's frame; `None` on a map made by hand.
    look: Option<Box<Look>>,
    /// The walls of the city, indexed by the square's place in the city area; `None` outside
    /// a city battle, and in one without walls every site is `Wall::None`.
    walls: Option<Vec<Wall>>,
    /// The city is a Flying Fortress: it is shut to anyone who cannot fly or pass through walls,
    /// from the outside and from the inside alike.
    fortress: bool,
    /// Squares Earth to Mud turned to mud.
    mud: Vec<bool>,
    /// The squares of a ship's deck in a sea battle.
    #[serde(default)]
    ship_deck: Vec<bool>,
    /// What the walls do (`city_wall` of `battle.ron`); `None` on a map that was not given the rules,
    /// where the walls are shut to everyone and no square is a gate.
    #[serde(default)]
    wall_rules: Option<f4x_def::rules::CityWallRules>,
}

impl BattleMap {
    /// A map with nothing in the way.
    pub fn open() -> Self {
        Self {
            cells: vec![Cell::Open; (GRID_W * GRID_H) as usize],
            costs: vec![Some(OPEN_COST); (GRID_W * GRID_H) as usize],
            sail: vec![None; (GRID_W * GRID_H) as usize],
            look: None,
            walls: None,
            fortress: false,
            mud: vec![false; (GRID_W * GRID_H) as usize],
            ship_deck: vec![false; (GRID_W * GRID_H) as usize],
            wall_rules: None,
        }
    }

    /// Set what the walls do, as `battle.ron` says.
    pub fn with_wall_rules(mut self, rules: f4x_def::rules::CityWallRules) -> Self {
        self.wall_rules = Some(rules);
        self
    }

    /// The square of the gate in the wall, where `battle.ron` puts it.
    pub fn gate(&self) -> Option<Pos> {
        let (dx, dy) = self.wall_rules.as_ref()?.gate;
        Some((CITY_X0 + dx, CITY_Y0 + dy))
    }

    /// Whether the square is a gate of the wall.
    pub fn is_gate(&self, pos: Pos) -> bool {
        self.gate() == Some(pos)
    }

    /// The squares of the standing towers (the corners of the wall that still stand).
    pub fn towers(&self) -> Vec<Pos> {
        (0..GRID_H)
            .flat_map(|y| (0..GRID_W).map(move |x| (x, y)))
            .filter(|&p| is_wall_corner(p) && self.has_wall(p))
            .collect()
    }

    /// Whether a standing tower is on the square and the rules shut it to everyone.
    pub fn is_shut_tower(&self, pos: Pos) -> bool {
        !self.wall_rules.as_ref().is_some_and(|r| r.tower_enterable)
            && is_wall_corner(pos)
            && self.has_wall(pos)
    }

    /// Whether the square is a ship's deck.
    pub fn is_ship_deck(&self, pos: Pos) -> bool {
        Self::in_bounds(pos)
            && self
                .ship_deck
                .get(Self::index(pos))
                .copied()
                .unwrap_or(false)
    }

    /// Whether the square is mud.
    pub fn is_mud(&self, pos: Pos) -> bool {
        Self::in_bounds(pos) && self.mud[Self::index(pos)]
    }

    /// Turn the square to mud if it is open ground (grass or dirt; forest, rocks and the like stay
    /// as they are). Returns whether it changed.
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
        let mut walls = vec![Wall::None; (CITY_SIZE * CITY_SIZE) as usize];
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
            .as_ref()
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
        self.walls
            .as_ref()
            .is_some_and(|w| w.contains(&Wall::Broken))
    }

    /// A standing wall that can be attacked.
    pub fn has_wall(&self, pos: Pos) -> bool {
        self.wall(pos) == Wall::Standing
    }

    /// Whether every wall site of the city has a standing wall.
    pub fn is_fully_walled(&self) -> bool {
        self.walls.as_ref().is_some_and(|w| {
            (CITY_Y0..CITY_Y0 + CITY_SIZE)
                .flat_map(|y| (CITY_X0..CITY_X0 + CITY_SIZE).map(move |x| (x, y)))
                .filter(|&p| is_wall_site(p))
                .all(|p| w[Self::city_index(p)] == Wall::Standing)
        })
    }

    /// Raise a wall on every wall site (Wall of Stone), whole or broken before. A unit that stands
    /// on a site stays there.
    pub fn raise_walls(&mut self) {
        let Some(walls) = self.walls.as_mut() else {
            return;
        };
        for y in CITY_Y0..CITY_Y0 + CITY_SIZE {
            for x in CITY_X0..CITY_X0 + CITY_SIZE {
                if is_wall_site((x, y)) {
                    walls[Self::city_index((x, y))] = Wall::Standing;
                }
            }
        }
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

    /// The map for a field built by [`layout::generate`]. The rules use the mirrored one (`x = 20 -
    /// cgx`).
    pub fn from_layout(layout: Layout) -> Self {
        let mut map = Self::open();
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let (cgx, cgy) = cg_of((x, y));
                let cost = layout.cost_at(cgx, cgy);
                let i = Self::index((x, y));
                map.costs[i] = cost;
                map.sail[i] = layout.sail_at(cgx, cgy);
                map.ship_deck[i] = layout.look.roads[layout::cell(cgx, cgy)] == layout::SHIP_DECK;
                map.cells[i] = match cost {
                    _ if layout.is_void(cgx, cgy) => Cell::Void,
                    // Ocean: walkers cannot enter it, ships and fliers can.
                    None => Cell::Blocked,
                    Some(c) if c > OPEN_COST => Cell::Rough,
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

    /// Whether a square can be entered by anyone (all of them but a structure's, on a map made by
    /// hand and on a generated one).
    pub fn is_playable(&self, pos: Pos) -> bool {
        self.get(pos).is_some_and(|c| c != Cell::Void)
    }

    /// Shut every square outside the original's band (`layout::in_band`) to everyone, as the original
    /// does (`RuleSettings::original_battlefield`); the field keeps its 24 by 24 squares and its look.
    pub fn shut_beyond_original(&mut self) {
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let (cgx, cgy) = cg_of((x, y));
                if !layout::in_band(cgx, cgy) {
                    self.set((x, y), Cell::Void);
                }
            }
        }
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
                Cell::Open => Some(OPEN_COST),
                Cell::Rough => Some(ROUGH_COST),
                Cell::Blocked | Cell::Void => None,
            };
            self.sail[i] = None;
        }
    }

    /// Moves to enter `pos`, or `None` if the unit cannot. (The walls of a city are not
    /// counted: see [`BattleMap::reachable_as`].)
    pub fn enter_cost(&self, pos: Pos, flying: bool) -> Option<HalfInt> {
        match (self.get(pos)?, flying) {
            (Cell::Void, _) => None,
            (_, true) => Some(OPEN_COST),
            (Cell::Blocked, false) => None,
            _ => self.costs[Self::index(pos)],
        }
    }

    /// Moves for `mover` to enter `pos`, or `None` if it cannot: fliers pay one anywhere
    /// they can go, ships the sailing cost, swimmers the lesser of that and the walking one.
    pub fn enter_cost_as(&self, pos: Pos, mover: Mover) -> Option<HalfInt> {
        let cell = self.get(pos)?;
        if cell == Cell::Void {
            return None;
        }
        if mover.flying {
            return Some(OPEN_COST);
        }
        let i = Self::index(pos);
        let (walk, sail) = (self.costs[i], self.sail[i]);
        if mover.swims {
            // The lesser of the two ways; a way that is shut does not count.
            [walk, sail].into_iter().flatten().min()
        } else if mover.sails {
            sail
        } else {
            walk
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
        if self.is_shut_tower(pos) {
            return true;
        }
        if (mover.flying || mover.through_walls)
            && self.wall_rules.as_ref().is_some_and(|r| r.passers_cross)
        {
            return false;
        }
        if !inside {
            let rules = self.wall_rules.as_ref();
            let allowed = if self.is_gate(pos) {
                rules.is_some_and(|r| r.gate_enterable)
            } else {
                rules.is_some_and(|r| r.section_enterable)
            };
            return !allowed && self.has_wall(pos);
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
                if !is_wall_site(site) || self.is_gate(site) || !self.has_wall(site) {
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

    /// Whether the step from `at` to `next` leaves a standing wall section for a square outside the
    /// city area: a unit on the wall walks along it but leaves it only by the gate.
    fn steps_off_a_wall(&self, at: Pos, next: Pos, mover: Mover) -> bool {
        let passes = (mover.flying || mover.through_walls)
            && self.wall_rules.as_ref().is_some_and(|r| r.passers_cross);
        self.is_walled()
            && !passes
            && in_city_area(at)
            && !in_city_area(next)
            && self.has_wall(at)
            && !self.is_gate(at)
    }

    /// Squares reachable from `from` with `points` moves, with the cheapest cost and the
    /// square each was reached from. `occupied` squares cannot be entered.
    pub fn reachable(
        &self,
        from: Pos,
        points: HalfInt,
        flying: bool,
        occupied: &dyn Fn(Pos) -> bool,
    ) -> HashMap<Pos, (HalfInt, Pos)> {
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
        points: HalfInt,
        mover: Mover,
        occupied: &dyn Fn(Pos) -> bool,
    ) -> HashMap<Pos, (HalfInt, Pos)> {
        let inside = in_city_area(from);
        let mut best: HashMap<Pos, (HalfInt, Pos)> = HashMap::new();
        let mut queue = BinaryHeap::new();
        best.insert(from, (HalfInt::ZERO, from));
        queue.push(Reverse((HalfInt::ZERO, from)));
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
                    } else if dx != 0 && dy != 0 {
                        // A diagonal step costs half a move more, for every kind of mover (wiki
                        // "Movement Type"). The original charges it only when it plots the
                        // path, not when it marks the squares a unit can reach (1.50 note 445):
                        // here one search does both, so they agree. Mud takes all the moves left
                        // anyway.
                        step += DIAGONAL_EXTRA;
                    }
                    if occupied(next)
                        || self.shut_by_walls(next, mover, inside)
                        || self.steps_off_a_wall(at, next, mover)
                    {
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
            // From one ship's deck a walker may jump the single square of water to another
            // deck, for two moves; any move left lets it, as with rough ground.
            if self.is_ship_deck(at) && !mover.flying && !mover.sails {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (gap, far) = ((at.0 + dx, at.1 + dy), (at.0 + 2 * dx, at.1 + 2 * dy));
                        if self.enter_cost_as(gap, mover).is_some()
                            || !self.is_ship_deck(far)
                            || occupied(far)
                        {
                            continue;
                        }
                        let total = cost + JUMP_COST;
                        if cost >= points || best.get(&far).is_some_and(|&(c, _)| c <= total) {
                            continue;
                        }
                        best.insert(far, (total, at));
                        queue.push(Reverse((total, far)));
                    }
                }
            }
        }
        best
    }
}

/// The combat terrain set for an overland terrain class: swamp and grassland are plains, and open
/// water has no ground of its own.
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
pub fn city_deployment(
    rules: &f4x_def::rules::DeploymentRules,
    attacker: bool,
    count: usize,
    fortress: bool,
) -> Vec<Pos> {
    let block = if attacker {
        &rules.city_attacker
    } else {
        &rules.city_defender
    };
    let mut out: Vec<Pos> = block
        .squares()
        .into_iter()
        .filter(|&p| attacker || !fortress || p != STRUCTURE)
        .collect();
    out.truncate(count);
    out
}

/// Squares where a side's `count` units start outside a city: the same block of three columns
/// of four as at a city (the original deploys every battle alike), then two columns further
/// back for a stack beyond twelve. The defenders keep off the square of a lair's structure.
pub fn deployment(
    rules: &f4x_def::rules::DeploymentRules,
    attacker: bool,
    count: usize,
) -> Vec<Pos> {
    let block = if attacker {
        &rules.attacker
    } else {
        &rules.defender
    };
    let mut out: Vec<Pos> = block
        .squares()
        .into_iter()
        .filter(|&p| attacker || p != STRUCTURE)
        .collect();
    out.truncate(count);
    out
}

/// The order a side's units take their squares in, front first (`Deploy_Battle_Units`): the
/// units without a ranged attack, then the shooters. With more than five shooters the strongest
/// in melee move forward, until the front has four units or four shooters are left.
pub fn deployment_order(units: &[BattleUnit]) -> Vec<usize> {
    let shoots = |u: &BattleUnit| u.own_ranged > 0 && u.ammo > 0;
    let mut melee: Vec<usize> = (0..units.len()).filter(|&i| !shoots(&units[i])).collect();
    let mut ranged: Vec<usize> = (0..units.len()).filter(|&i| shoots(&units[i])).collect();
    if ranged.len() > 5 {
        let to_move = ranged.len() - 4;
        for _ in 0..to_move {
            if melee.len() >= 4 {
                break;
            }
            // The strongest is taken from the shooters. The first of equals goes.
            let best = (0..ranged.len())
                .max_by_key(|&k| (units[ranged[k]].own_melee, std::cmp::Reverse(k)))
                .unwrap_or(0);
            melee.push(ranged.remove(best));
        }
    }
    melee.extend(ranged);
    melee
}

/// The square of a lair's structure, an outpost's or a tower's, in the rules' frame.
pub const STRUCTURE: Pos = pos_of_cg(layout::STRUCTURE);

/// Distance in squares (diagonals count as one).
pub fn distance(a: Pos, b: Pos) -> u32 {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

#[cfg(test)]
mod tests;
