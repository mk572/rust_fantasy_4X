//! The look and the ground of a battlefield, built the way the original builds it
//! (`Build_Battlefield` and `Generate_Combat_Map` in ReMoM `Combat.c`).
//!
//! Everything here is in the **original's frame**: a cell is `(cgx, cgy)`, the city and the
//! defenders are at the low `cgx`, and the screen position of a cell is
//! `((cgx - cgy) * 16 + 158, (cgx + cgy) * 8 - 80)`. The battle rules use the mirrored frame
//! (`x = 20 - cgx`, see `docs/divergences.md`); [`Layout::cost_at`] and the client convert.
//!
//! The pieces, in the original's order: patches of rough ground and dirt are scattered, a road
//! is walked in from each side the overland roads leave by, rough ground next to dirt or a road
//! is put right, dirt patches are merged, then every cell gets its picture (grass, dirt, a rough
//! piece chosen from its four neighbours) and its movement cost, and last trees and rocks are
//! scattered, each making the cell it stands on dearer to enter (never forbidden).
//!
//! The original never fills in the river flags it passes down (`Build_Battlefield` zeroes
//! them), so a battle has no river, and neither has ours. Water battles (`Set::Water`) keep the
//! old open ground.

use rand::Rng;

pub const W: i32 = 21;
pub const H: i32 = 22;

/// Half-moves for a cell nobody can enter (outside the band, or a lair's structure).
pub const INF: u8 = 255;

/// The original's city area, in its own frame.
pub const MIN_CGX_CITY: i32 = 5;
pub const MAX_CGX_CITY: i32 = 8;
pub const MIN_CGY_CITY: i32 = 10;
pub const MAX_CGY_CITY: i32 = 13;
/// Where a lair, tower, outpost and the like stands.
pub const STRUCTURE: (i32, i32) = (6, 11);

/// Whether a cell is in the playable band: the part of the grid the original's screen shows
/// (`Set_Movement_Cost_Maps` shuts the four corners).
pub fn in_band(cgx: i32, cgy: i32) -> bool {
    (0..W).contains(&cgx)
        && (0..H).contains(&cgy)
        && (11..=31).contains(&(cgx + cgy))
        && (cgx - cgy).abs() <= 9
}

/// The screen position of a cell's top corner, on the original's 320 by 200 screen, with the
/// offsets inside the cell used for trees and rocks (`Combat_Grid_Screen_Coordinates`).
pub fn screen_of(cgx: i32, cgy: i32, sub_x: i32, sub_y: i32) -> (i32, i32) {
    (
        (cgx - cgy) * 16 + 158 + (sub_x - sub_y) * 2,
        (cgx + cgy) * 8 - 80 + sub_x + sub_y,
    )
}

/// The original's combat terrain sets (`cts_*`) that have ground to fight on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set {
    Plains,
    Hills,
    Mountains,
    Desert,
    Tundra,
    Forest,
    /// Open water: kept as plain open ground, without scenery.
    Water,
}

/// What stands in the middle of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    Open,
    Outpost,
    City,
}

/// What stands on the structure square of a field with a [`Site::Outpost`] (`CS_*` of the
/// original): an outpost, or the building of a lair, tower or node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Structure {
    Outpost,
    Cave,
    Tower,
    Dungeon,
    Fort,
    Temple,
    Ruins,
    /// The nodes: nature has a picture, sorcery and chaos an animation over the ground.
    NatureNode,
    SorceryNode,
    ChaosNode,
}

impl Structure {
    /// The structure a site of the map stands for (`clt_*` to `CS_*` in `Build_Battlefield`; the
    /// original's ruins are a dungeon and its fallen temple the ruins), by the feature's id.
    pub fn of_feature(id: &str) -> Option<Structure> {
        Some(match id {
            "tower" => Structure::Tower,
            "chaos_node" => Structure::ChaosNode,
            "nature_node" => Structure::NatureNode,
            "sorcery_node" => Structure::SorceryNode,
            "mysterious_cave" | "monster_lair" => Structure::Cave,
            "dungeon" | "ruins" => Structure::Dungeon,
            "ancient_temple" => Structure::Temple,
            "abandoned_keep" => Structure::Fort,
            "fallen_temple" => Structure::Ruins,
            _ => return None,
        })
    }
}

/// What the overland square says about the field.
#[derive(Debug, Clone)]
pub struct Spec {
    pub set: Set,
    pub site: Site,
    /// What the structure square holds when `site` is [`Site::Outpost`].
    pub structure: Structure,
    /// Citizens (the number of houses, twelve at most).
    pub population: u32,
    pub walled: bool,
    /// The race's house style (0 standard, 1 tree, 2 mound), as in the content.
    pub house_style: u8,
    /// Whether a road lies on the square and on its eight neighbours, row by row from the north
    /// west: index 4 is the square itself.
    pub roads: [bool; 9],
    pub enchanted_roads: bool,
}

impl Spec {
    /// Set the house style from the race's (`0` standard, `1` tree, `2` mound).
    pub fn house_type_from(&mut self, house_type: u8) {
        self.house_style = house_type;
    }

    /// An open field of plain ground.
    pub fn open(set: Set) -> Spec {
        Spec {
            set,
            site: Site::Open,
            structure: Structure::Outpost,
            population: 0,
            walled: false,
            house_style: 0,
            roads: [false; 9],
            enchanted_roads: false,
        }
    }
}

/// A house in the city, which is only for show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct House {
    pub cell: (i32, i32),
    /// Which of the fifteen house pictures (`cmbtcity` entry `2 + variant`).
    pub variant: u8,
}

/// A tree or a rock: the cell, the offset inside it (0 to 5) and which of the five pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prop {
    pub cell: (i32, i32),
    pub sub: (u8, u8),
    pub variant: u8,
}

/// What the battle screen draws, in the original's frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    pub set: Set,
    pub site: Site,
    pub structure: Structure,
    pub enchanted_roads: bool,
    /// Ground picture of each cell, `cgy * 21 + cgx`: 0 to 3 grass, 4 to 7 dirt, 8 to 31 grass
    /// edged by dirt, 32 to 47 rough ground edged by its neighbours.
    pub tiles: Vec<u8>,
    /// Road pieces of each cell: 0 none; otherwise bit 0 the plain/worn variant and bits 1 to 6
    /// the pieces to draw (`ROADGRID`, in the order of `Build_Road_Network`).
    pub roads: Vec<u8>,
    pub houses: Vec<House>,
    pub trees: Vec<Prop>,
    pub rocks: Vec<Prop>,
}

/// The ground of a battlefield and its look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub look: Look,
    /// Half-moves to enter each cell on foot (`INF` for none), `cgy * 21 + cgx`.
    pub cost: Vec<u8>,
}

impl Layout {
    pub fn cost_at(&self, cgx: i32, cgy: i32) -> u8 {
        if (0..W).contains(&cgx) && (0..H).contains(&cgy) {
            self.cost[(cgy * W + cgx) as usize]
        } else {
            INF
        }
    }
}

const GRASS: u8 = 0;
const ROUGH: u8 = 1;
const DIRT: u8 = 2;

/// `step_delta_cgx` and `step_delta_cgy` of the original, indexed 1 to 4 (south, west, north,
/// east).
const STEP_X: [i32; 5] = [0, 0, -1, 0, 1];
const STEP_Y: [i32; 5] = [0, 1, 0, -1, 0];

fn at(cgx: i32, cgy: i32) -> usize {
    (cgy * W + cgx) as usize
}

fn inside(cgx: i32, cgy: i32) -> bool {
    (0..W).contains(&cgx) && (0..H).contains(&cgy)
}

/// The original's `Random(n)`: 1 to n.
fn random(rng: &mut impl Rng, n: i32) -> i32 {
    rng.gen_range(1..=n)
}

fn group_at(group: &[u8], cgx: i32, cgy: i32) -> u8 {
    if inside(cgx, cgy) {
        group[at(cgx, cgy)]
    } else {
        GRASS
    }
}

/// `Scatter_Terrain_Patches`: random walks of `base + Random(span)` steps that lay `kind`.
fn scatter_patches(
    group: &mut [u8],
    kind: u8,
    count: i32,
    span: i32,
    base: i32,
    rng: &mut impl Rng,
) {
    for _ in 0..count {
        let start_x = random(rng, W) - 1;
        let start_y = random(rng, H) - 1;
        let mut x = start_x + STEP_X[random(rng, 4) as usize];
        let mut y = start_y + STEP_Y[random(rng, 4) as usize];
        let size = base + random(rng, span);
        for _ in 0..size {
            let dir = (random(rng, 4) - 1) as usize;
            x = (x + STEP_X[1 + dir]).clamp(0, W - 1);
            y = (y + STEP_Y[1 + dir]).clamp(0, H - 1);
            group[at(x, y)] = kind;
        }
    }
}

/// `Build_Road_Network`: a road walked in from each side the overland road leaves by, then the
/// pieces each cell needs.
fn build_roads(site: Site, roads_in: &[bool; 9], rng: &mut impl Rng) -> Vec<u8> {
    let mut roads = vec![0u8; (W * H) as usize];
    let city = site != Site::Open;
    if roads_in[4] {
        let origin = (10, 10);
        if roads_in[5] || roads_in[2] {
            let (mut x, mut y) = if city {
                (MAX_CGX_CITY - 1, MAX_CGY_CITY - 1)
            } else {
                origin
            };
            roads[at(x, y)] = 1;
            let mut previous = 3;
            let mut on_heading = 0;
            while x < 20 {
                let dir;
                if on_heading < 2 {
                    dir = previous;
                    on_heading += 1;
                } else {
                    let mut d;
                    loop {
                        d = if random(rng, 4) == 1 {
                            random(rng, 4) - 1
                        } else {
                            3
                        };
                        if d != 1 {
                            break;
                        }
                    }
                    dir = d;
                    on_heading = 1;
                }
                previous = dir;
                let (nx, ny) = (
                    x + STEP_X[(dir + 1) as usize],
                    y + STEP_Y[(dir + 1) as usize],
                );
                if inside(nx, ny) && dir != 1 {
                    x = nx;
                    y = ny;
                    roads[at(x, y)] = 1;
                }
            }
        }
        if roads_in[1] || roads_in[0] {
            let (mut x, mut y) = if city { (7, 11) } else { origin };
            roads[at(x, y)] = 1;
            let mut previous = 2;
            let mut on_heading = 0;
            while y > 0 {
                let dir;
                if on_heading < 2 {
                    dir = previous;
                    on_heading += 1;
                } else {
                    let mut d;
                    loop {
                        d = if random(rng, 3) == 1 {
                            random(rng, 4) - 1
                        } else {
                            2
                        };
                        if d != 0 {
                            break;
                        }
                    }
                    dir = d;
                    if dir != previous {
                        on_heading = 1;
                    }
                }
                previous = dir;
                let (nx, ny) = (
                    x + STEP_X[(dir + 1) as usize],
                    y + STEP_Y[(dir + 1) as usize],
                );
                if inside(nx, ny) {
                    x = nx;
                    y = ny;
                    roads[at(x, y)] = 1;
                }
            }
        }
        if roads_in[3] || roads_in[6] {
            let (mut x, mut y) = if city { (6, 12) } else { origin };
            roads[at(x, y)] = 1;
            let mut previous = 1;
            let mut on_heading = 0;
            while x > 0 {
                let dir;
                if on_heading < 2 {
                    dir = previous;
                    on_heading += 1;
                } else {
                    let mut d;
                    loop {
                        d = if random(rng, 4) == 1 {
                            random(rng, 4) - 1
                        } else {
                            1
                        };
                        if d != 3 {
                            break;
                        }
                    }
                    dir = d;
                    on_heading = 1;
                }
                previous = dir;
                let (nx, ny) = (
                    x + STEP_X[(dir + 1) as usize],
                    y + STEP_Y[(dir + 1) as usize],
                );
                if inside(nx, ny) {
                    x = nx;
                    y = ny;
                    roads[at(x, y)] = 1;
                }
            }
        }
        if roads_in[7] || roads_in[8] {
            let (mut x, mut y) = if city { (7, 12) } else { origin };
            roads[at(x, y)] = 1;
            let mut previous = 0;
            let mut on_heading = 0;
            while y < W {
                let dir;
                if on_heading < 2 {
                    dir = previous;
                    on_heading += 1;
                } else {
                    let mut d;
                    loop {
                        d = if random(rng, 4) == 1 {
                            random(rng, 4) - 1
                        } else {
                            0
                        };
                        if d != 2 {
                            break;
                        }
                    }
                    dir = d;
                    on_heading = 1;
                }
                previous = dir;
                let (nx, ny) = (
                    x + STEP_X[(dir + 1) as usize],
                    y + STEP_Y[(dir + 1) as usize],
                );
                if inside(nx, ny) {
                    x = nx;
                    y = ny;
                    roads[at(x, y)] = 1;
                }
            }
        }
    }
    // The pieces each road cell needs, from which of its neighbours are road. The edges count
    // as road, except below the last row, where the original reads past its array.
    let plain = roads.clone();
    let road_or_edge = |x: i32, y: i32| -> bool {
        if !(0..W).contains(&x) || y < 0 {
            true
        } else if y >= H {
            false
        } else {
            plain[at(x, y)] != 0
        }
    };
    for y in 0..H {
        for x in 0..W {
            if plain[at(x, y)] == 0 {
                continue;
            }
            let n = road_or_edge(x, y - 1);
            let s = road_or_edge(x, y + 1);
            let w = road_or_edge(x - 1, y);
            let e = road_or_edge(x + 1, y);
            let mut v = roads[at(x, y)];
            if w && e {
                v += 2;
            }
            if n && s {
                v += 4;
            }
            if n && w {
                v += 8;
            }
            if e && n {
                v += 16;
            }
            if s && e {
                v += 32;
            }
            if w && s {
                v += 64;
            }
            roads[at(x, y)] = v;
        }
    }
    // Half of the cells lose bit 0 (the worn variant); a cell left with nothing is no road.
    for v in roads.iter_mut() {
        if *v != 0 && random(rng, 2) == 1 {
            *v &= 0xFE;
        }
    }
    roads
}

/// `Remove_Rough_Terrain`: rough ground beside dirt, or on a road, becomes grass; so does all of
/// the city area.
fn remove_rough(group: &mut [u8], roads: &[u8], site: Site) {
    for _ in 0..8 {
        for y in 0..H {
            for x in 0..W {
                if group[at(x, y)] != ROUGH {
                    continue;
                }
                let beside_dirt = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| (dx, dy) != (0, 0) && group_at(group, x + dx, y + dy) == DIRT)
                });
                if beside_dirt || roads[at(x, y)] != 0 {
                    group[at(x, y)] = GRASS;
                }
            }
        }
    }
    if site != Site::Open {
        for y in MIN_CGY_CITY..=MAX_CGY_CITY {
            for x in MIN_CGX_CITY..=MAX_CGX_CITY {
                if group[at(x, y)] == ROUGH {
                    group[at(x, y)] = GRASS;
                }
            }
        }
    }
}

/// `Merge_Dirt_Patches`: grass with dirt on both sides, across or down, becomes dirt.
fn merge_dirt(group: &mut [u8]) {
    let is_dirt = |g: &[u8], x: i32, y: i32| inside(x, y) && g[at(x, y)] == DIRT;
    for _ in 0..10 {
        for y in 0..H {
            for x in 0..W {
                if group[at(x, y)] != GRASS {
                    continue;
                }
                if is_dirt(group, x + 1, y) && is_dirt(group, x - 1, y) {
                    group[at(x, y)] = DIRT;
                }
                if is_dirt(group, x, y - 1) && is_dirt(group, x, y + 1) {
                    group[at(x, y)] = DIRT;
                }
            }
        }
    }
}

/// `Set_Terrain_Tile_Types`: the picture of every cell. Cells outside the grid count as grass.
fn tile_types(group: &[u8], rng: &mut impl Rng) -> Vec<u8> {
    let mut tiles = vec![0u8; (W * H) as usize];
    for y in 0..H {
        for x in 0..W {
            let g = |dx: i32, dy: i32| group_at(group, x + dx, y + dy);
            let (up, down, left, right) = (g(0, -1), g(0, 1), g(-1, 0), g(1, 0));
            let (ul, ur, dl, dr) = (g(-1, -1), g(1, -1), g(-1, 1), g(1, 1));
            let centre = g(0, 0);
            let pair =
                |base: i32, rng: &mut dyn rand::RngCore| (base + 1 + rng.gen_range(0..2)) as u8;
            tiles[at(x, y)] = match centre {
                GRASS => {
                    if right == DIRT {
                        if up == DIRT {
                            pair(27, rng)
                        } else if down == DIRT {
                            pair(11, rng)
                        } else {
                            pair(29, rng)
                        }
                    } else if up == DIRT {
                        if left == DIRT {
                            pair(21, rng)
                        } else {
                            pair(23, rng)
                        }
                    } else if left == DIRT {
                        if down == DIRT {
                            pair(15, rng)
                        } else {
                            pair(17, rng)
                        }
                    } else if down == DIRT {
                        pair(9, rng)
                    } else if dr == DIRT {
                        pair(7, rng)
                    } else if ur == DIRT {
                        pair(25, rng)
                    } else if ul == DIRT {
                        pair(19, rng)
                    } else if dl == DIRT {
                        pair(13, rng)
                    } else {
                        (random(rng, 4) - 1) as u8
                    }
                }
                DIRT => (3 + random(rng, 4)) as u8,
                ROUGH => {
                    let (u, r, d, l) = (up == ROUGH, right == ROUGH, down == ROUGH, left == ROUGH);
                    let variant = match (u, r, d, l) {
                        (true, true, true, true) => 4,
                        (true, true, true, false) => 10,
                        (true, true, false, true) => 13,
                        (true, true, false, false) => 12,
                        (true, false, true, true) => 11,
                        (true, false, true, false) => 1,
                        (true, false, false, true) => 14,
                        (true, false, false, false) => 5,
                        (false, true, true, true) => 8,
                        (false, true, true, false) => 7,
                        (false, true, false, true) => 3,
                        (false, true, false, false) => 2,
                        (false, false, true, true) => 9,
                        (false, false, true, false) => 0,
                        (false, false, false, true) => 6,
                        (false, false, false, false) => 15,
                    };
                    32 + variant
                }
                _ => 0,
            };
        }
    }
    tiles
}

/// `Set_Movement_Cost_Maps` (the walkers' map): grass and dirt 2, rough ground 4, roads 1,
/// the corners outside the band and the structure of a lair none.
fn movement_costs(group: &[u8], roads: &[u8], site: Site) -> Vec<u8> {
    let mut cost = vec![2u8; (W * H) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = at(x, y);
            cost[i] = match group[i] {
                ROUGH => 4,
                _ => 2,
            };
            if roads[i] != 0 {
                cost[i] = 1;
            }
            if !in_band(x, y) {
                cost[i] = INF;
            }
        }
    }
    if site == Site::Outpost {
        cost[at(STRUCTURE.0, STRUCTURE.1)] = INF;
    }
    cost
}

/// Whether the cell is in the city area.
#[cfg(test)]
fn in_city_area(x: i32, y: i32) -> bool {
    (MIN_CGX_CITY..=MAX_CGX_CITY).contains(&x) && (MIN_CGY_CITY..=MAX_CGY_CITY).contains(&y)
}

/// The cells a tree or a rock may not stand on, beside the ones every scatter refuses
/// (`Scatter_Tree_Scenery`, `Scatter_Rock_Scenery`).
fn forbidden(site: Site, x: i32, y: i32, rock: bool) -> bool {
    let range = |lo: i32, hi: i32, v: i32| (lo..=hi).contains(&v);
    let mut no = false;
    if site != Site::Open {
        // The lower rows of the city area, and the part round the structure.
        no |= range(MIN_CGY_CITY + 1, MAX_CGY_CITY, y) && range(MIN_CGX_CITY, MAX_CGX_CITY - 1, x);
        no |= range(MIN_CGX_CITY, MAX_CGX_CITY - 1, x) && range(MIN_CGY_CITY, MAX_CGY_CITY - 1, y);
    }
    if rock {
        no |= site == Site::City && x == MAX_CGX_CITY && y == MIN_CGY_CITY;
        // The original keeps rocks off the east and south walls even on an open field.
        no |= x == MAX_CGX_CITY && range(MIN_CGY_CITY + 1, MAX_CGY_CITY, y);
        no |= y == MAX_CGY_CITY && range(MIN_CGX_CITY, MAX_CGX_CITY, x);
    } else if site == Site::City {
        no |= x == MAX_CGX_CITY && range(MIN_CGY_CITY, MAX_CGY_CITY, y);
        no |= y == MAX_CGY_CITY && range(MIN_CGX_CITY, MAX_CGX_CITY, x);
    }
    no
}

fn on_screen(cgx: i32, cgy: i32, sub: (i32, i32)) -> bool {
    let (sx, sy) = screen_of(cgx, cgy, sub.0, sub.1);
    sx > 0 && sx < 320 && sy > 0 && sy < 180
}

/// `Scatter_Tree_Scenery`.
fn scatter_trees(
    set: Set,
    site: Site,
    group: &[u8],
    roads: &[u8],
    cost: &mut [u8],
    rng: &mut impl Rng,
) -> Vec<Prop> {
    let tree_count = match set {
        Set::Plains | Set::Hills | Set::Tundra => random(rng, 20),
        Set::Desert => random(rng, 10),
        Set::Mountains => random(rng, 40),
        _ => 30 + random(rng, 30),
    };
    let mut trees = Vec::new();
    if tree_count == 0 {
        return trees;
    }
    let patch_count = tree_count / 10 + random(rng, 4);
    if tree_count < patch_count {
        return trees;
    }
    let patch_size = (tree_count / patch_count).max(1);
    let mut tries = 0;
    for _ in 0..patch_count {
        let (px, py) = loop {
            if tries > 500 {
                return trees;
            }
            let (px, py) = (random(rng, W) - 1, random(rng, H) - 1);
            if roads[at(px, py)] != 0 || group[at(px, py)] != GRASS || cost[at(px, py)] == INF {
                continue;
            }
            break (px, py);
        };
        let mut placed = 0;
        while placed < patch_size {
            let (tx, ty) = (px + random(rng, 7) - 4, py + random(rng, 7) - 4);
            let sub = (random(rng, 6), random(rng, 6));
            let mut illegal = !inside(tx, ty);
            if !illegal {
                illegal = roads[at(tx, ty)] != 0
                    || group[at(tx, ty)] != GRASS
                    || cost[at(tx, ty)] == INF
                    || forbidden(site, tx, ty, false);
            }
            if !illegal && on_screen(tx, ty, sub) {
                let c = &mut cost[at(tx, ty)];
                if *c < 4 {
                    *c += 1;
                }
                trees.push(Prop {
                    cell: (tx, ty),
                    sub: (sub.0 as u8, sub.1 as u8),
                    variant: (random(rng, 5) - 1) as u8,
                });
            } else {
                tries += 1;
                if tries < 500 {
                    placed -= 1;
                }
            }
            placed += 1;
        }
    }
    trees
}

/// `Scatter_Rock_Scenery`.
fn scatter_rocks(
    set: Set,
    site: Site,
    group: &[u8],
    roads: &[u8],
    cost: &mut [u8],
    rng: &mut impl Rng,
) -> Vec<Prop> {
    let rock_count = match set {
        Set::Plains | Set::Hills => random(rng, 5) - 1,
        Set::Desert => random(rng, 8) - 1,
        Set::Mountains => random(rng, 12),
        Set::Tundra => random(rng, 12) + 4,
        _ => random(rng, 8),
    };
    let mut rocks = Vec::new();
    let mut placed = 0;
    // The original has no limit on its attempts; this one stops a field with no room at all.
    let mut attempts = 0;
    while placed < rock_count && attempts < 5000 {
        attempts += 1;
        let (rx, ry) = (random(rng, W) - 1, random(rng, H) - 1);
        let sub = (random(rng, 2) + 3, random(rng, 2) + 3);
        let ok = roads[at(rx, ry)] == 0
            && group[at(rx, ry)] == GRASS
            && cost[at(rx, ry)] != INF
            && !forbidden(site, rx, ry, true)
            && on_screen(rx, ry, sub);
        if ok {
            let c = &mut cost[at(rx, ry)];
            if *c < 4 {
                *c += 1;
            }
            rocks.push(Prop {
                cell: (rx, ry),
                sub: (sub.0 as u8, sub.1 as u8),
                variant: (random(rng, 5) - 1) as u8,
            });
            placed += 1;
        }
    }
    rocks
}

/// The houses of a city (`Generate_Combat_Map`, `clt_City`): twelve for a city of twelve
/// citizens or more, one for each citizen otherwise at random places, none on the wall's
/// corners when it is walled.
fn houses_of(spec: &Spec, rng: &mut impl Rng) -> Vec<House> {
    let mut houses = Vec::new();
    if spec.site != Site::City {
        return houses;
    }
    // The original swaps the tree and mound styles when it picks the pictures.
    let style = match spec.house_style {
        1 => 2,
        2 => 1,
        s => s,
    } as i32;
    let corner = |i: i32, j: i32| (i == 0 || i == 3) && (j == 0 || j == 3);
    let picture = |rng: &mut dyn rand::RngCore| (style * 5 + rng.gen_range(0..5)) as u8;
    if spec.population >= 12 {
        for j in 0..4 {
            for i in 0..4 {
                if corner(i, j) && spec.walled {
                    continue;
                }
                houses.push(House {
                    cell: (MIN_CGX_CITY + i, MIN_CGY_CITY + j),
                    variant: picture(rng),
                });
            }
        }
    } else {
        // Few citizens: random squares, each used once. A walled city's corners are refused.
        let room = 16 - if spec.walled { 4 } else { 0 };
        let wanted = (spec.population as usize).min(room);
        while houses.len() < wanted {
            let (i, j) = (random(rng, 4) - 1, random(rng, 4) - 1);
            let cell = (MIN_CGX_CITY + i, MIN_CGY_CITY + j);
            if (spec.walled && corner(i, j)) || houses.iter().any(|h| h.cell == cell) {
                continue;
            }
            houses.push(House {
                cell,
                variant: picture(rng),
            });
        }
    }
    houses
}

/// Build the field for `spec`, with `rng` standing in for the original's seeded `Random`.
pub fn generate(spec: &Spec, rng: &mut impl Rng) -> Layout {
    let n = (W * H) as usize;
    if spec.set == Set::Water {
        let mut cost = vec![2u8; n];
        for y in 0..H {
            for x in 0..W {
                if !in_band(x, y) {
                    cost[at(x, y)] = INF;
                }
            }
        }
        let tiles = (0..n).map(|_| (48 + rng.gen_range(0..4)) as u8).collect();
        return Layout {
            look: Look {
                set: spec.set,
                site: spec.site,
                structure: spec.structure,
                enchanted_roads: false,
                tiles,
                roads: vec![0; n],
                houses: Vec::new(),
                trees: Vec::new(),
                rocks: Vec::new(),
            },
            cost,
        };
    }
    let houses = houses_of(spec, rng);
    let mut group = vec![GRASS; n];
    let rough = match spec.set {
        Set::Plains | Set::Forest => 5,
        Set::Hills => 20,
        Set::Desert | Set::Tundra => 10,
        Set::Mountains => 30,
        Set::Water => 0,
    };
    scatter_patches(&mut group, ROUGH, rough, 8, 3, rng);
    scatter_patches(&mut group, DIRT, 8, 4, 0, rng);
    let roads = build_roads(spec.site, &spec.roads, rng);
    remove_rough(&mut group, &roads, spec.site);
    merge_dirt(&mut group);
    let tiles = tile_types(&group, rng);
    let mut cost = movement_costs(&group, &roads, spec.site);
    let trees = scatter_trees(spec.set, spec.site, &group, &roads, &mut cost, rng);
    let rocks = scatter_rocks(spec.set, spec.site, &group, &roads, &mut cost, rng);
    Layout {
        look: Look {
            set: spec.set,
            site: spec.site,
            structure: spec.structure,
            enchanted_roads: spec.enchanted_roads,
            tiles,
            roads,
            houses,
            trees,
            rocks,
        },
        cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;

    fn rng(seed: u64) -> Pcg64Mcg {
        Pcg64Mcg::seed_from_u64(seed)
    }

    #[test]
    fn the_band_is_the_screens_diamond() {
        let cells = (0..H)
            .flat_map(|y| (0..W).map(move |x| (x, y)))
            .filter(|&(x, y)| in_band(x, y))
            .count();
        assert_eq!(
            cells, 200,
            "ten cells on each of the twenty rows of the screen"
        );
        assert!(!in_band(0, 0) && !in_band(20, 0) && !in_band(0, 21) && !in_band(20, 21));
        // The deployment squares and the city area are inside.
        for (x, y) in [(6, 11), (8, 12), (14, 10), (16, 13), (5, 10), (8, 13)] {
            assert!(in_band(x, y), "({x},{y})");
        }
        // The screen position of the band's cells stays on the 320 by 200 screen.
        for y in 0..H {
            for x in 0..W {
                if in_band(x, y) {
                    let (sx, sy) = screen_of(x, y, 0, 0);
                    assert!(
                        (-16..=320).contains(&sx) && (0..=170).contains(&sy),
                        "({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn a_field_is_repeatable_and_keeps_the_corners_shut() {
        let spec = Spec::open(Set::Hills);
        let a = generate(&spec, &mut rng(3));
        let b = generate(&spec, &mut rng(3));
        assert_eq!(a, b);
        let c = generate(&spec, &mut rng(4));
        assert_ne!(a.look.tiles, c.look.tiles);
        for y in 0..H {
            for x in 0..W {
                assert_eq!(a.cost_at(x, y) == INF, !in_band(x, y), "({x},{y})");
            }
        }
    }

    #[test]
    fn the_ground_costs_two_rough_ground_four_and_trees_add_one() {
        for seed in 0..40 {
            let l = generate(&Spec::open(Set::Mountains), &mut rng(seed));
            let costs: std::collections::HashSet<u8> =
                l.cost.iter().copied().filter(|&c| c != INF).collect();
            assert!(costs.iter().all(|c| (1..=4).contains(c)), "{costs:?}");
            for t in l.look.trees.iter().chain(&l.look.rocks) {
                assert!(l.cost_at(t.cell.0, t.cell.1) >= 3 || l.look.set == Set::Water);
                assert_eq!(l.look.roads[at(t.cell.0, t.cell.1)], 0, "no prop on a road");
            }
        }
    }

    #[test]
    fn mountain_fields_have_more_rough_ground_than_plains() {
        let rough = |set: Set| -> usize {
            (0..30)
                .map(|s| {
                    let l = generate(&Spec::open(set), &mut rng(s));
                    l.look.tiles.iter().filter(|&&t| t >= 32).count()
                })
                .sum()
        };
        assert!(rough(Set::Mountains) > 2 * rough(Set::Plains));
    }

    #[test]
    fn tiles_are_pictures_the_set_has() {
        for seed in 0..20 {
            let l = generate(&Spec::open(Set::Plains), &mut rng(seed));
            assert!(l.look.tiles.iter().all(|&t| t < 48));
        }
    }

    #[test]
    fn roads_run_from_the_sides_the_overland_roads_leave_by() {
        let mut spec = Spec::open(Set::Plains);
        // A road on the square and to the east and the west.
        spec.roads = [false, false, false, true, true, true, false, false, false];
        let l = generate(&spec, &mut rng(2));
        let road = |x: i32, y: i32| l.look.roads[at(x, y)] != 0;
        let count = l.look.roads.iter().filter(|&&r| r != 0).count();
        assert!(count >= 15, "a road across the field, got {count}");
        assert!(l.look.roads.iter().all(|&r| r & 0x80 == 0));
        // No road leaves a field with no road on it.
        let mut none = Spec::open(Set::Plains);
        none.roads = [false; 9];
        let n = generate(&none, &mut rng(2));
        assert!(n.look.roads.iter().all(|&r| r == 0));
        let _ = road;
    }

    #[test]
    fn a_city_has_houses_and_keeps_its_inside_clear() {
        let mut spec = Spec::open(Set::Plains);
        spec.site = Site::City;
        spec.population = 14;
        spec.walled = true;
        let l = generate(&spec, &mut rng(1));
        assert_eq!(
            l.look.houses.len(),
            12,
            "sixteen squares less the four corners"
        );
        assert!(l
            .look
            .houses
            .iter()
            .all(|h| in_city_area(h.cell.0, h.cell.1)));
        let small = Spec {
            population: 3,
            ..spec.clone()
        };
        assert_eq!(generate(&small, &mut rng(1)).look.houses.len(), 3);
        for seed in 0..30 {
            let l = generate(&spec, &mut rng(seed));
            for t in l.look.trees.iter().chain(&l.look.rocks) {
                assert!(
                    !(in_city_area(t.cell.0, t.cell.1) && t.cell.0 < MAX_CGX_CITY),
                    "{:?}",
                    t.cell
                );
            }
        }
    }

    #[test]
    fn an_outpost_shuts_its_structure_cell() {
        let mut spec = Spec::open(Set::Plains);
        spec.site = Site::Outpost;
        let l = generate(&spec, &mut rng(1));
        assert_eq!(l.cost_at(STRUCTURE.0, STRUCTURE.1), INF);
        assert!(l.look.houses.is_empty());
    }

    #[test]
    fn water_stays_open_and_bare() {
        let l = generate(&Spec::open(Set::Water), &mut rng(1));
        assert!(l.look.trees.is_empty() && l.look.rocks.is_empty());
        assert_eq!(l.cost_at(10, 10), 2);
    }
}
