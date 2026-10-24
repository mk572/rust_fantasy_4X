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
//! Rivers: the original has `Carve_River_Terrain` and never uses it (`Build_Battlefield` zeroes
//! the river flags it passes down). Ours fills the flags from the overland square and its
//! neighbours, so a battle on a river square, on plains, has a river walked across it.
//!
//! Water battles (`Set::Water`) are all ocean: nobody can walk on it, ships sail it for two
//! half-moves a square, fliers cross it. A side with a Floating Island has its deck (a block of
//! squares marked `0x80` in the road layer, with `0x81` where the island's picture hangs) to stand
//! land units on.

use rand::Rng;

pub const W: i32 = 21;
pub const H: i32 = 22;

/// Half-moves for a cell nobody can enter (outside the band, or a lair's structure).
pub const INF: u8 = 255;
/// Half-moves for a ship to sail a square of ocean or river.
pub const SAIL_COST: u8 = 2;
/// The road-layer value of a floating island's deck, and of the square its picture hangs from
/// (which stays ocean).
pub const DECK: u8 = 0x80;
pub const DECK_PICTURE: u8 = 0x81;
/// The road-layer value of a ship's deck in a sea battle (task 233): ground to stand on, apart
/// from the island's only in that a walker may jump the single square of water between decks.
pub const SHIP_DECK: u8 = 0x82;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Site {
    Open,
    Outpost,
    City,
}

/// What stands on the structure square of a field with a [`Site::Outpost`] (`CS_*` of the
/// original): an outpost, or the building of a lair, tower or node; or, at a [`Site::City`], the
/// wizard's fortress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Structure {
    Outpost,
    /// The wizard's fortress city: a city with the fortress on its structure square.
    Fortress,
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
    /// What the structure square holds when `site` is [`Site::Outpost`]; [`Structure::Fortress`]
    /// at a wizard's capital ([`Site::City`]).
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
    /// Whether a river runs on the square and on its eight neighbours, in the order of `roads`.
    pub rivers: [bool; 9],
    /// A Floating Island under the attackers, and under the defenders (water battles).
    pub attacker_island: bool,
    pub defender_island: bool,
    /// The squares of the ships' decks (water battles), in the original's frame.
    pub ship_decks: Vec<(i32, i32)>,
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
            rivers: [false; 9],
            attacker_island: false,
            defender_island: false,
            ship_decks: Vec::new(),
        }
    }
}

/// A house in the city, which is only for show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct House {
    pub cell: (i32, i32),
    /// Which of the fifteen house pictures (`cmbtcity` entry `2 + variant`).
    pub variant: u8,
}

/// A tree or a rock: the cell, the offset inside it (0 to 5) and which of the five pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Prop {
    pub cell: (i32, i32),
    pub sub: (u8, u8),
    pub variant: u8,
}

/// What the battle screen draws, in the original's frame.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Half-moves to enter each cell by ship (`INF` for none).
    pub sail: Vec<u8>,
}

impl Layout {
    /// Whether nobody can enter the cell, flying or not: outside the band, or a structure's.
    pub fn is_void(&self, cgx: i32, cgy: i32) -> bool {
        !in_band(cgx, cgy)
            || ((cgx, cgy) == STRUCTURE
                && (self.look.site == Site::Outpost || self.look.structure == Structure::Fortress))
    }

    pub fn sail_at(&self, cgx: i32, cgy: i32) -> u8 {
        if inside(cgx, cgy) {
            self.sail[at(cgx, cgy)]
        } else {
            INF
        }
    }

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
const RIVER: u8 = 3;
const OCEAN: u8 = 4;

/// The ground picture of a river cell's tile is one of these (`56 + 6 * variant + shape`), over
/// grass picture `(cgx + cgy) & 3`.
pub const RIVER_TILE: u8 = 56;

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

/// `Carve_River_Terrain`: a river walked across the field from the sides the overland river
/// leaves by, only where the square has a river (`rivers[4]`). A river leaving to the east or
/// west starts at the left corner of the playable band and winds towards the right one (never
/// stepping west or south); one leaving to the north or south winds from top to bottom. The
/// original starts the first kind at (7, 20), outside the band, so most of it is never seen, and
/// the second on the field's last row, where it ends after a step; ours starts them at (6, 15)
/// and at the band's top edge (`x + y == 11`).
fn carve_river(group: &mut [u8], rivers: &[bool; 9], rng: &mut impl Rng) {
    if !rivers[4] {
        return;
    }
    let mut alternate = false;
    if rivers[5] || rivers[2] || rivers[3] || rivers[6] {
        let (mut x, mut y) = (6, 15);
        while x < W - 1 && y > 0 {
            let dir = if random(rng, 2) == 1 {
                random(rng, 4) - 1
            } else if !alternate {
                3
            } else {
                2
            };
            alternate = !alternate;
            let (nx, ny) = (
                x + STEP_X[(dir + 1) as usize],
                y + STEP_Y[(dir + 1) as usize],
            );
            // Forward is anything but west and south.
            if inside(nx, ny) && dir != 1 && dir != 0 {
                x = nx;
                y = ny;
                group[at(x, y)] = RIVER;
            }
        }
    } else if rivers[0] || rivers[1] || rivers[7] || rivers[8] {
        let x0 = 3 + random(rng, 6);
        let (mut x, mut y) = (x0, 11 - x0);
        while x < W - 1 && y < H - 1 {
            let dir = if random(rng, 2) == 1 {
                random(rng, 4) - 1
            } else if !alternate {
                3
            } else {
                0
            };
            alternate = !alternate;
            let (nx, ny) = (
                x + STEP_X[(dir + 1) as usize],
                y + STEP_Y[(dir + 1) as usize],
            );
            // Forward is anything but west and north.
            if inside(nx, ny) && dir != 1 && dir != 2 {
                x = nx;
                y = ny;
                group[at(x, y)] = RIVER;
            }
        }
    }
}

/// The squares a Floating Island's deck covers, in the original's frame, for the defenders and
/// for the attackers, and the square each island's picture hangs from. When both sides have one
/// the attackers' sits in the middle, with one more square towards the defenders.
pub fn island_decks(attacker: bool, defender: bool) -> IslandDecks {
    let mut decks = IslandDecks::default();
    let block = |x0: i32, y0: i32| -> Vec<(i32, i32)> {
        (y0..y0 + 3)
            .flat_map(|y| (x0..x0 + 3).map(move |x| (x, y)))
            .collect()
    };
    if defender {
        decks.defenders = block(MIN_CGX_CITY, 11);
        decks.pictures.push((MAX_CGX_CITY, 13));
    }
    if attacker {
        if defender {
            decks.attackers = block(9, 11);
            decks.attackers.push((8, 12));
            decks.pictures.push((12, 13));
        } else {
            decks.attackers = block(16, 10);
            decks.pictures.push((19, 12));
        }
    }
    decks
}

/// The squares of a ship's deck of `width` by `length` squares, relative to its top left corner:
/// long in `y`, pointed at both ends.
pub fn ship_shape(width: i32, length: i32) -> Vec<(i32, i32)> {
    let taper = (width / 2).min(2);
    let least = if width % 2 == 0 { 2 } else { 1 };
    let mut out = Vec::new();
    for row in 0..length {
        let d = row.min(length - 1 - row);
        let inset = 2 * (taper - d).max(0);
        let here = (width - inset).max(least).min(width);
        let x0 = (width - here) / 2;
        out.extend((x0..x0 + here).map(|x| (x, row)));
    }
    out
}

/// Where the ships' decks lie in a sea battle, in the rules' frame: for each ship of a side, in
/// the order given, the squares of its deck, or `None` if it did not fit. The first ship of each
/// side faces the other's across one square of water; the rest lie where they fit, one square of
/// water between any two decks. `attackers` and `defenders` are the `(width, length)` of each
/// ship; if the first two cannot face each other nothing is laid out (all `None`).
pub fn ship_decks(attackers: &[(i32, i32)], defenders: &[(i32, i32)]) -> ShipDecks {
    let mut out = ShipDecks {
        attackers: vec![None; attackers.len()],
        defenders: vec![None; defenders.len()],
    };
    let cells_at = |size: (i32, i32), x: i32, y: i32| -> Vec<(i32, i32)> {
        ship_shape(size.0, size.1)
            .into_iter()
            .map(|(dx, dy)| (x + dx, y + dy))
            .collect()
    };
    let inside = |cells: &[(i32, i32)]| cells.iter().all(|&(x, y)| in_band(W - 1 - x, y));
    let clear = |cells: &[(i32, i32)], placed: &[(i32, i32)]| {
        cells.iter().all(|&(x, y)| {
            placed
                .iter()
                .all(|&(px, py)| (px - x).abs().max((py - y).abs()) > 1)
        })
    };
    let mut placed: Vec<(i32, i32)> = Vec::new();
    // The first ships face each other across a gap column.
    let first = match (attackers.first(), defenders.first()) {
        (Some(&a), Some(&d)) => {
            let mut best: Option<(i32, Cells, Cells)> = None;
            for edge in 0..W {
                let (xa, xd) = (edge - a.0 + 1, edge + 2);
                if xa < 0 || xd + d.0 > W {
                    continue;
                }
                for ya in 0..=(H - a.1) {
                    let ca = cells_at(a, xa, ya);
                    if !inside(&ca) {
                        continue;
                    }
                    for yd in 0..=(H - d.1) {
                        let cd = cells_at(d, xd, yd);
                        if !inside(&cd) {
                            continue;
                        }
                        let score = (edge - 9).abs() * 2
                            + (ya * 2 + a.1 - H).abs()
                            + (yd * 2 + d.1 - H).abs();
                        if best.as_ref().is_none_or(|b| score < b.0) {
                            best = Some((score, ca.clone(), cd));
                        }
                    }
                }
            }
            best.map(|(_, ca, cd)| (ca, cd))
        }
        // One side alone: its first ship lies by the middle, on its own side.
        (Some(&one), None) | (None, Some(&one)) => {
            let attacker = defenders.is_empty();
            let mut best: Option<(i32, Cells)> = None;
            for x in 0..=(W - one.0) {
                for y in 0..=(H - one.1) {
                    let cells = cells_at(one, x, y);
                    if !inside(&cells) {
                        continue;
                    }
                    let edge = if attacker { x + one.0 - 1 } else { x };
                    let score = (edge - if attacker { 9 } else { 11 }).abs() * 2
                        + (y * 2 + one.1 - H).abs();
                    if best.as_ref().is_none_or(|b| score < b.0) {
                        best = Some((score, cells));
                    }
                }
            }
            best.map(|(_, c)| {
                if attacker {
                    (c, Vec::new())
                } else {
                    (Vec::new(), c)
                }
            })
        }
        _ => None,
    };
    let Some((ca, cd)) = first else {
        return out;
    };
    placed.extend(&ca);
    placed.extend(&cd);
    let centre = |c: &[(i32, i32)]| {
        let n = c.len() as i32;
        (
            c.iter().map(|p| p.0).sum::<i32>() / n,
            c.iter().map(|p| p.1).sum::<i32>() / n,
        )
    };
    let anchors = [
        if ca.is_empty() { (9, 11) } else { centre(&ca) },
        if cd.is_empty() { (11, 11) } else { centre(&cd) },
    ];
    if !ca.is_empty() {
        out.attackers[0] = Some(ca);
    }
    if !cd.is_empty() {
        out.defenders[0] = Some(cd);
    }
    for (side, sizes) in [(0usize, attackers), (1, defenders)] {
        for (n, &size) in sizes.iter().enumerate().skip(1) {
            let mut best: Option<(i32, Vec<(i32, i32)>)> = None;
            for x in 0..=(W - size.0) {
                for y in 0..=(H - size.1) {
                    let cells = cells_at(size, x, y);
                    if !inside(&cells) || !clear(&cells, &placed) {
                        continue;
                    }
                    let c = centre(&cells);
                    let near = (c.0 - anchors[side].0)
                        .abs()
                        .max((c.1 - anchors[side].1).abs());
                    // A side keeps to its own half of the field.
                    let wrong = if side == 0 {
                        (c.0 - 9).max(0)
                    } else {
                        (11 - c.0).max(0)
                    };
                    let score = near + wrong * 2;
                    if best.as_ref().is_none_or(|b| score < b.0) {
                        best = Some((score, cells));
                    }
                }
            }
            if let Some((_, cells)) = best {
                placed.extend(&cells);
                if side == 0 {
                    out.attackers[n] = Some(cells);
                } else {
                    out.defenders[n] = Some(cells);
                }
            }
        }
    }
    out
}

/// The squares of one deck.
type Cells = Vec<(i32, i32)>;

/// See [`ship_decks`]: the deck of each ship, in the order the sizes were given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipDecks {
    pub attackers: Vec<Option<Vec<(i32, i32)>>>,
    pub defenders: Vec<Option<Vec<(i32, i32)>>>,
}

/// See [`island_decks`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IslandDecks {
    pub defenders: Vec<(i32, i32)>,
    pub attackers: Vec<(i32, i32)>,
    /// The squares the island pictures hang from (the road layer's `0x81`).
    pub pictures: Vec<(i32, i32)>,
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
                RIVER => {
                    // Beyond the field's edge a river goes on.
                    let r = |dx: i32, dy: i32| !inside(x + dx, y + dy) || g(dx, dy) == RIVER;
                    let (u, d, l, rt) = (r(0, -1), r(0, 1), r(-1, 0), r(1, 0));
                    let variant = ((random(rng, 2) - 1) * 6) as u8;
                    // The original tests the shapes in turn and the last that fits wins.
                    let shape = if l && d {
                        5
                    } else if d && rt {
                        4
                    } else if rt && u {
                        3
                    } else if u && l {
                        2
                    } else if u && d {
                        1
                    } else {
                        0
                    };
                    RIVER_TILE + shape + variant
                }
                _ => 0,
            };
        }
    }
    tiles
}

/// `Set_Movement_Cost_Maps`: the walkers' map (grass and dirt 2, rough ground and rivers 4,
/// roads 1, ocean none) and the ships' (ocean and rivers 2, nothing on land or a road), with the
/// corners outside the band and the structure of a lair shut on both. A floating island's deck
/// walks like a road.
fn movement_costs(
    group: &[u8],
    roads: &[u8],
    site: Site,
    structure: Structure,
) -> (Vec<u8>, Vec<u8>) {
    let n = (W * H) as usize;
    let mut cost = vec![2u8; n];
    let mut sail = vec![INF; n];
    for y in 0..H {
        for x in 0..W {
            let i = at(x, y);
            match group[i] {
                ROUGH => cost[i] = 4,
                RIVER => {
                    cost[i] = 4;
                    sail[i] = SAIL_COST;
                }
                OCEAN => {
                    cost[i] = INF;
                    sail[i] = SAIL_COST;
                }
                _ => {}
            }
            if roads[i] == SHIP_DECK {
                cost[i] = 2;
                sail[i] = INF;
            } else if roads[i] != 0 && roads[i] != DECK_PICTURE {
                cost[i] = 1;
                sail[i] = INF;
            }
            if !in_band(x, y) {
                cost[i] = INF;
                sail[i] = INF;
            }
        }
    }
    if site == Site::Outpost || structure == Structure::Fortress {
        cost[at(STRUCTURE.0, STRUCTURE.1)] = INF;
        sail[at(STRUCTURE.0, STRUCTURE.1)] = INF;
    }
    (cost, sail)
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
    // The fortress stands on the structure square, which is the city area's cell (1, 1).
    let fortress = spec.structure == Structure::Fortress;
    let picture = |rng: &mut dyn rand::RngCore| (style * 5 + rng.gen_range(0..5)) as u8;
    // A city fills the area from twelve citizens; a fortress city has one cell less, so eleven do.
    if spec.population >= 12 - u32::from(fortress) {
        for j in 0..4 {
            for i in 0..4 {
                if (corner(i, j) && spec.walled) || (fortress && (i, j) == (1, 1)) {
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
        let room = 16 - if spec.walled { 4 } else { 0 } - usize::from(fortress);
        let wanted = (spec.population as usize).min(room);
        while houses.len() < wanted {
            let (i, j) = (random(rng, 4) - 1, random(rng, 4) - 1);
            let cell = (MIN_CGX_CITY + i, MIN_CGY_CITY + j);
            if (spec.walled && corner(i, j))
                || (fortress && (i, j) == (1, 1))
                || houses.iter().any(|h| h.cell == cell)
            {
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
        // All ocean; a Floating Island's deck is marked in the road layer.
        let tiles: Vec<u8> = (0..n).map(|_| (48 + rng.gen_range(0..4)) as u8).collect();
        let group = vec![OCEAN; n];
        let mut roads = vec![0u8; n];
        let decks = island_decks(spec.attacker_island, spec.defender_island);
        for &(x, y) in decks.defenders.iter().chain(&decks.attackers) {
            roads[at(x, y)] = DECK;
        }
        for &(x, y) in &decks.pictures {
            roads[at(x, y)] = DECK_PICTURE;
        }
        for &(x, y) in &spec.ship_decks {
            roads[at(x, y)] = SHIP_DECK;
        }
        let (cost, sail) = movement_costs(&group, &roads, Site::Open, Structure::Outpost);
        return Layout {
            look: Look {
                set: spec.set,
                site: spec.site,
                structure: spec.structure,
                enchanted_roads: false,
                tiles,
                roads,
                houses: Vec::new(),
                trees: Vec::new(),
                rocks: Vec::new(),
            },
            cost,
            sail,
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
    if spec.set == Set::Plains {
        carve_river(&mut group, &spec.rivers, rng);
        // A city's ground and its structure keep dry.
        if spec.site != Site::Open {
            for y in MIN_CGY_CITY..=MAX_CGY_CITY {
                for x in MIN_CGX_CITY..=MAX_CGX_CITY {
                    if group[at(x, y)] == RIVER {
                        group[at(x, y)] = GRASS;
                    }
                }
            }
        }
    }
    let roads = build_roads(spec.site, &spec.roads, rng);
    remove_rough(&mut group, &roads, spec.site);
    merge_dirt(&mut group);
    let tiles = tile_types(&group, rng);
    let (mut cost, sail) = movement_costs(&group, &roads, spec.site, spec.structure);
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
        sail,
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
    fn a_fortress_city_keeps_its_structure_cell_clear_and_shut() {
        let mut spec = Spec::open(Set::Plains);
        spec.site = Site::City;
        spec.structure = Structure::Fortress;
        spec.population = 11;
        spec.walled = true;
        let full = generate(&spec, &mut rng(1));
        assert_eq!(
            full.look.houses.len(),
            11,
            "sixteen squares less four corners and the fortress"
        );
        assert!(full.look.houses.iter().all(|h| h.cell != STRUCTURE));
        assert_eq!(full.cost_at(STRUCTURE.0, STRUCTURE.1), INF);
        // Few citizens: as many houses as citizens, never on the fortress's cell.
        for seed in 0..20 {
            let small = Spec {
                population: 7,
                walled: false,
                ..spec.clone()
            };
            let l = generate(&small, &mut rng(seed));
            assert_eq!(l.look.houses.len(), 7);
            assert!(l.look.houses.iter().all(|h| h.cell != STRUCTURE));
        }
        // A plain city leaves the square open.
        spec.structure = Structure::Outpost;
        assert_ne!(
            generate(&spec, &mut rng(1)).cost_at(STRUCTURE.0, STRUCTURE.1),
            INF
        );
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
    fn water_is_bare_ocean_for_ships_and_fliers_only() {
        let l = generate(&Spec::open(Set::Water), &mut rng(1));
        assert!(l.look.trees.is_empty() && l.look.rocks.is_empty());
        assert_eq!(l.cost_at(10, 10), INF, "nobody walks on the sea");
        assert_eq!(l.sail_at(10, 10), SAIL_COST);
        assert_eq!(l.sail_at(0, 0), INF, "the corners are shut to ships too");
        assert!(!l.is_void(10, 10) && l.is_void(0, 0));
        assert!(l.look.tiles.iter().all(|&t| (48..52).contains(&t)));
    }

    #[test]
    fn a_floating_island_has_a_deck_to_walk_on_and_a_picture_square_that_stays_sea() {
        let mut spec = Spec::open(Set::Water);
        spec.defender_island = true;
        let l = generate(&spec, &mut rng(1));
        let decks = island_decks(false, true);
        assert_eq!(decks.defenders.len(), 9);
        for &(x, y) in &decks.defenders {
            assert_eq!(l.look.roads[at(x, y)], DECK);
            assert_eq!(l.cost_at(x, y), 1, "a deck walks like a road");
            assert_eq!(l.sail_at(x, y), INF);
        }
        let (px, py) = decks.pictures[0];
        assert_eq!(l.look.roads[at(px, py)], DECK_PICTURE);
        assert_eq!(l.cost_at(px, py), INF);
        assert_eq!(l.sail_at(px, py), SAIL_COST);
        // Both sides: the attackers' island is in the middle, and nothing overlaps.
        let both = island_decks(true, true);
        assert!(both.attackers.len() == 10 && both.pictures.len() == 2);
        assert!(both
            .attackers
            .iter()
            .all(|c| !both.defenders.contains(c) && !both.pictures.contains(c)));
        // The attackers alone have theirs on the far side.
        let alone = island_decks(true, false);
        assert!(alone.attackers.iter().all(|&(x, _)| x >= 16));
    }

    fn river_spec(rivers: [bool; 9]) -> Spec {
        let mut spec = Spec::open(Set::Plains);
        spec.rivers = rivers;
        spec
    }

    #[test]
    fn a_river_square_has_a_river_across_the_field_and_others_have_none() {
        let east_west = [false, false, false, true, true, true, false, false, false];
        let north_south = [false, true, false, false, true, false, false, true, false];
        let mut across = 0;
        for seed in 0..20 {
            for rivers in [east_west, north_south] {
                let l = generate(&river_spec(rivers), &mut rng(seed));
                let wet = (0..H)
                    .flat_map(|y| (0..W).map(move |x| (x, y)))
                    .filter(|&(x, y)| in_band(x, y) && l.look.tiles[at(x, y)] >= RIVER_TILE)
                    .count();
                assert!(wet >= 8, "seed {seed}: {wet} river squares in view");
                across += 1;
                for y in 0..H {
                    for x in 0..W {
                        let i = at(x, y);
                        if l.look.tiles[i] >= RIVER_TILE && l.look.roads[i] == 0 {
                            assert_eq!(l.cost_at(x, y), if in_band(x, y) { 4 } else { INF });
                            if in_band(x, y) {
                                assert_eq!(l.sail_at(x, y), SAIL_COST);
                            }
                        }
                    }
                }
                for t in l.look.trees.iter().chain(&l.look.rocks) {
                    assert!(l.look.tiles[at(t.cell.0, t.cell.1)] < RIVER_TILE);
                }
            }
        }
        assert_eq!(across, 40);
        // No river on the square, none on the field; and none on other ground.
        for set in [Set::Plains, Set::Hills] {
            let mut spec = Spec::open(set);
            spec.rivers = [false; 9];
            let l = generate(&spec, &mut rng(3));
            assert!(l.look.tiles.iter().all(|&t| t < RIVER_TILE));
        }
        let mut hills = Spec::open(Set::Hills);
        hills.rivers = east_west;
        assert!(generate(&hills, &mut rng(3))
            .look
            .tiles
            .iter()
            .all(|&t| t < RIVER_TILE));
    }

    #[test]
    fn a_river_keeps_out_of_a_citys_area_and_leaves_the_deployment_squares_reachable() {
        let mut spec = river_spec([false, false, false, true, true, true, false, false, false]);
        spec.site = Site::City;
        spec.population = 8;
        for seed in 0..20 {
            let l = generate(&spec, &mut rng(seed));
            for y in MIN_CGY_CITY..=MAX_CGY_CITY {
                for x in MIN_CGX_CITY..=MAX_CGX_CITY {
                    assert!(l.look.tiles[at(x, y)] < RIVER_TILE);
                }
            }
            // Every cell a unit may deploy on can be entered on foot.
            for (x, y) in [(14, 12), (15, 11), (16, 13), (8, 12), (7, 11)] {
                assert_ne!(l.cost_at(x, y), INF, "seed {seed} ({x},{y})");
            }
        }
    }

    #[test]
    fn a_ship_deck_is_pointed_at_both_ends() {
        let shape = ship_shape(5, 9);
        let width = |row: i32| shape.iter().filter(|p| p.1 == row).count();
        assert_eq!(
            (0..9).map(width).collect::<Vec<_>>(),
            vec![1, 3, 5, 5, 5, 5, 5, 3, 1]
        );
        assert_eq!(ship_shape(4, 8).iter().filter(|p| p.1 == 0).count(), 2);
    }

    #[test]
    fn every_pair_of_ships_faces_across_one_square_of_water() {
        let sizes = [(3, 7), (4, 8), (5, 9)];
        for a in sizes {
            for d in sizes {
                let decks = ship_decks(&[a], &[d]);
                let (Some(da), Some(dd)) = (&decks.attackers[0], &decks.defenders[0]) else {
                    panic!("{a:?} against {d:?} did not fit");
                };
                for &(x, y) in da.iter().chain(dd) {
                    assert!(
                        in_band(W - 1 - x, y),
                        "{a:?} {d:?}: ({x}, {y}) is off the field"
                    );
                }
                let gap = da
                    .iter()
                    .flat_map(|p| {
                        dd.iter()
                            .map(move |q| (p.0 - q.0).abs().max((p.1 - q.1).abs()))
                    })
                    .min()
                    .unwrap();
                assert_eq!(gap, 2, "{a:?} against {d:?}: one square of water between");
            }
        }
    }

    #[test]
    fn more_ships_lie_where_they_fit_with_water_between() {
        let decks = ship_decks(&[(3, 7), (3, 7)], &[(3, 7), (3, 7), (3, 7)]);
        let all: Vec<_> = decks
            .attackers
            .iter()
            .chain(&decks.defenders)
            .flatten()
            .collect();
        assert!(all.len() >= 3, "{} ships laid out", all.len());
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(a.iter().all(|p| b
                    .iter()
                    .all(|q| (p.0 - q.0).abs().max((p.1 - q.1).abs()) >= 2)));
            }
        }
    }
}
