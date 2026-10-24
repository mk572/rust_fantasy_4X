//! The picture of a city on the city screen: a port of the original's procedural layout
//! (`Cityscape_Make_Buildings_Array` and `Cityscape_Draw_Buildings` in ReMoM `CITYSCAP.c`).
//!
//! The city is a 15 by 23 grid of cells drawn sheared, with roads between fixed rows and
//! columns, buildings placed by footprint (2x2, 2x3, 3x2, 3x3) around the middle, houses
//! (eight to a thousand citizens), trees and rocks scattered by terrain, the fortress and the
//! magic buildings in fixed places. The layout is seeded by the city's position, so a city
//! looks the same every time it is opened.

// The loops follow the original's row and column indexes, which reads best as written.
#![allow(clippy::needless_range_loop)]

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

pub const ROWS: usize = 15;
pub const COLS: usize = 23;

/// Cell contents: a building index, or one of these.
pub const OPEN: i16 = 0;
pub const HOUSE: i16 = 2;
pub const UNUSABLE: i16 = -1;
pub const ROAD: i16 = -2;
pub const TREE: i16 = -3;
pub const ROCK: i16 = -4;

/// Building indexes beyond the 100 ordinary ones: the magic buildings.
pub const SUMMONING_CIRCLE: i16 = 100;
pub const EARTH_GATE: i16 = 101;
pub const STREAM_OF_LIFE: i16 = 102;
pub const ASTRAL_GATE: i16 = 103;
pub const FORTRESS: i16 = 104;
pub const DARK_RITUALS: i16 = 105;
pub const ALTAR_OF_BATTLE: i16 = 106;

const SHIP_WRIGHTS: i16 = 12;
const MARITIME: i16 = 14;
const FIRST_BUILDING: i16 = 3;
const LAST_BUILDING: i16 = 34;

/// Footprint in cells (rows, columns), by the building table's shape number.
fn footprint(shape: u8) -> (usize, usize) {
    match shape {
        0 => (2, 2),
        1 => (2, 3),
        2 => (3, 2),
        _ => (3, 3),
    }
}

/// What the layout needs to know about a city.
#[derive(Debug, Clone, Default)]
pub struct CityInfo {
    /// Citizens in thousands.
    pub population: u32,
    /// Ordinary buildings present, as `(original building index, shape)`.
    pub buildings: Vec<(i16, u8)>,
    /// Magic buildings present (indexes 100 to 106 other than the fortress).
    pub magic: Vec<i16>,
    pub fortress: bool,
    /// Tree houses (the race's house type is 1) or forest ground: many trees.
    pub many_trees: bool,
    /// Mountain or hills under the city: many rocks.
    pub many_rocks: bool,
    /// Seed: the original uses `0x2895 + x * y`.
    pub seed: u64,
}

pub type Grid = [[i16; COLS]; ROWS];

/// `Random(n)` in the original: one to n.
fn random(rng: &mut Pcg64Mcg, n: i32) -> i32 {
    rng.gen_range(1..=n.max(1))
}

/// Where footprints of one shape went, in the order placed.
struct Spots {
    rows: Vec<usize>,
    cols: Vec<usize>,
}

/// Place `count` footprints of `rows` by `cols` cells around the middle of the grid.
fn place(
    grid: &mut Grid,
    rng: &mut Pcg64Mcg,
    rows: usize,
    cols: usize,
    mut count: usize,
    always_widen: bool,
) -> Spots {
    let mut spots = Spots {
        rows: Vec::new(),
        cols: Vec::new(),
    };
    let mut rnd_loc = 3i32;
    let mut tries = 0;
    while spots.rows.len() < count {
        let col = 10 + random(rng, rnd_loc + 1) - rnd_loc / 2;
        let row = 6 + random(rng, rnd_loc + 1) - rnd_loc / 2;
        if col >= 0
            && (col as usize) < COLS - (cols - 1)
            && row >= 0
            && (row as usize) < ROWS - (rows - 1)
        {
            let (r, c) = (row as usize, col as usize);
            let free = (0..rows).all(|dr| (0..cols).all(|dc| grid[r + dr][c + dc] == OPEN));
            if free {
                tries = 0;
                spots.rows.push(r);
                spots.cols.push(c);
                for dr in 0..rows {
                    for dc in 0..cols {
                        grid[r + dr][c + dc] = 1;
                    }
                }
            }
            tries += 1;
            if tries > 500 {
                tries = 0;
                count -= 1;
            }
        }
        if rnd_loc < 30 && (always_widen || random(rng, 3) == 1) {
            rnd_loc += 1;
        }
    }
    spots
}

/// The grid of cells for a city.
pub fn make_layout(info: &CityInfo) -> Grid {
    let mut rng = Pcg64Mcg::seed_from_u64(info.seed);
    let mut g: Grid = [[OPEN; COLS]; ROWS];

    // Corners nobody can build in.
    for r in 4..ROWS {
        g[r][0] = UNUSABLE;
    }
    for r in 7..ROWS {
        g[r][1] = UNUSABLE;
    }
    for r in 10..ROWS {
        g[r][2] = UNUSABLE;
    }
    for r in 12..ROWS {
        g[r][3] = UNUSABLE;
    }
    for c in 19..COLS {
        for r in 0..5 {
            g[r][c] = UNUSABLE;
        }
    }
    g[6][21] = UNUSABLE;
    g[6][22] = UNUSABLE;
    g[7][22] = UNUSABLE;
    g[0][18] = UNUSABLE;
    if info.fortress {
        for c in 10..13 {
            for r in 6..9 {
                g[r][c] = 1;
            }
        }
        for r in 1..=2 {
            g[r][7] = UNUSABLE;
            g[r][8] = UNUSABLE;
        }
        for r in 3..=4 {
            g[r][10] = UNUSABLE;
        }
    }
    g[7][22] = UNUSABLE;
    g[8][22] = UNUSABLE;
    g[7][21] = UNUSABLE;
    g[8][21] = UNUSABLE;
    // Roads: four across, four down.
    for c in 0..18 {
        g[0][c] = ROAD;
    }
    for c in 1..23 {
        g[5][c] = ROAD;
    }
    for c in 3..23 {
        g[9][c] = ROAD;
    }
    for c in 4..23 {
        g[14][c] = ROAD;
    }
    for r in 0..15 {
        g[r][4] = ROAD;
        g[r][9] = ROAD;
        g[r][13] = ROAD;
    }
    for r in 1..15 {
        g[r][18] = ROAD;
    }

    // How many footprints of each shape are needed (ships and the walls are placed by hand).
    let mut counts = [0usize; 4];
    for &(idx, shape) in &info.buildings {
        if (FIRST_BUILDING..=LAST_BUILDING).contains(&idx)
            && !(SHIP_WRIGHTS..=MARITIME).contains(&idx)
        {
            counts[usize::from(shape.min(3))] += 1;
        }
    }
    let has = |m: i16| info.magic.contains(&m);
    // Two by three: summoning circle, dark rituals, altar of battle; two by two: the gates and
    // the stream of life.
    for m in [SUMMONING_CIRCLE, DARK_RITUALS, ALTAR_OF_BATTLE] {
        if has(m) {
            counts[1] += 1;
        }
    }
    for m in [EARTH_GATE, STREAM_OF_LIFE, ASTRAL_GATE] {
        if has(m) {
            counts[0] += 1;
        }
    }
    // The port takes a fixed place.
    let port = info
        .buildings
        .iter()
        .map(|&(i, _)| i)
        .filter(|i| (SHIP_WRIGHTS..=MARITIME).contains(i))
        .max();
    if port.is_some() {
        for r in 6..9 {
            for c in 2..4 {
                g[r][c] = 1;
            }
        }
    }

    let s33 = place(&mut g, &mut rng, 3, 3, counts[3], true);
    let s23 = place(&mut g, &mut rng, 2, 3, counts[1], false);
    let s32 = place(&mut g, &mut rng, 3, 2, counts[2], false);
    let s22 = place(&mut g, &mut rng, 2, 2, counts[0], false);

    // Houses: eight to a thousand citizens, near the middle.
    let houses = (info.population * 8).max(1);
    let mut rnd_loc = 3i32;
    let mut h = 0;
    while h < houses {
        let col = 10 + random(&mut rng, rnd_loc + 1) - rnd_loc / 2;
        let row = 6 + random(&mut rng, rnd_loc + 1) - rnd_loc / 2;
        if (0..COLS as i32).contains(&col)
            && (0..ROWS as i32).contains(&row)
            && g[row as usize][col as usize] == OPEN
        {
            g[row as usize][col as usize] = HOUSE;
            h += 1;
        } else {
            if rnd_loc < 30 && random(&mut rng, 4) == 1 {
                rnd_loc += 1;
            }
            if random(&mut rng, 4) == 1 {
                h += 1; // gives up on this one now and then, as the original does
            }
        }
    }

    // Trees and rocks.
    let trees = if info.many_trees {
        80 + random(&mut rng, 20)
    } else {
        5 + random(&mut rng, 8) + random(&mut rng, 8)
    };
    let rocks = if info.many_rocks {
        20 + random(&mut rng, 10)
    } else {
        random(&mut rng, 8) + random(&mut rng, 8)
    };
    for (count, kind) in [(trees, TREE), (rocks, ROCK)] {
        let mut rnd_loc = 6i32;
        let mut i = 0;
        let mut guard = 0;
        while i < count && guard < 4000 {
            guard += 1;
            let col = 10 + random(&mut rng, rnd_loc + 1) - rnd_loc / 2;
            let row = 6 + random(&mut rng, rnd_loc + 1) - rnd_loc / 2;
            if (0..COLS as i32).contains(&col)
                && (0..ROWS as i32).contains(&row)
                && g[row as usize][col as usize] == OPEN
            {
                g[row as usize][col as usize] = kind;
                i += 1;
            } else {
                if rnd_loc < 30 && random(&mut rng, 4) == 1 {
                    rnd_loc += 1;
                }
                if random(&mut rng, 2) != 1 {
                    // keep trying this one
                } else {
                    i += 1;
                }
            }
        }
    }

    // Now the buildings go on their footprints: the anchor is the bottom-left cell.
    let (mut i22, mut i23, mut i32_, mut i33) = (0, 0, 0, 0);
    let put = |g: &mut Grid, spots: &Spots, i: &mut usize, rows: usize, id: i16| {
        if *i < spots.rows.len() {
            g[spots.rows[*i] + rows - 1][spots.cols[*i]] = id;
        }
        *i += 1;
    };
    if has(SUMMONING_CIRCLE) {
        put(&mut g, &s23, &mut i23, 2, SUMMONING_CIRCLE);
    }
    for m in [EARTH_GATE, STREAM_OF_LIFE, ASTRAL_GATE] {
        if has(m) {
            put(&mut g, &s22, &mut i22, 2, m);
        }
    }
    if has(ALTAR_OF_BATTLE) {
        put(&mut g, &s23, &mut i23, 2, ALTAR_OF_BATTLE);
    }
    if has(DARK_RITUALS) {
        put(&mut g, &s23, &mut i23, 2, DARK_RITUALS);
    }
    let mut ordered = info.buildings.clone();
    ordered.sort_by_key(|&(i, _)| i);
    for (idx, shape) in ordered {
        if !(FIRST_BUILDING..=LAST_BUILDING).contains(&idx)
            || (SHIP_WRIGHTS..=MARITIME).contains(&idx)
        {
            continue;
        }
        match shape.min(3) {
            0 => put(&mut g, &s22, &mut i22, 2, idx),
            1 => put(&mut g, &s23, &mut i23, 2, idx),
            2 => put(&mut g, &s32, &mut i32_, 3, idx),
            _ => put(&mut g, &s33, &mut i33, 3, idx),
        }
    }
    if info.fortress {
        g[8][10] = FORTRESS;
    }
    if let Some(p) = port {
        g[8][2] = p;
    }
    let _ = footprint;
    g
}

/// Which `cityscap.lbx` entry draws a building: `(entry, y offset above the cell)`.
pub fn sprite_of(idx: i16) -> Option<(usize, f32)> {
    match idx {
        3..=30 => Some((42 + idx as usize, 32.0)),
        31 => Some((78, 32.0)),
        32..=34 => Some((41 + idx as usize, 32.0)),
        SUMMONING_CIRCLE => Some((6, 32.0)),
        EARTH_GATE => Some((83, 32.0)),
        STREAM_OF_LIFE => Some((84, 32.0)),
        ASTRAL_GATE => Some((85, 32.0)),
        FORTRESS => Some((40, 47.0)),
        DARK_RITUALS => Some((81, 32.0)),
        ALTAR_OF_BATTLE => Some((12, 32.0)),
        _ => None,
    }
}

/// How often the city screen draws in the original, which counts its animations in draws.
pub const DRAWS_PER_SECOND: f64 = 18.0;

/// The animation counter after `draws` draws (`cityscape_bldg_anim_ctr` and the others): the
/// counter is the draw count over six, and when the count reaches `wrap` it goes back to six.
/// Buildings and walls wrap at 60 (counter 1 to 9), the water at 84 (1 to 13).
pub fn anim_counter(draws: u64, wrap: u64) -> usize {
    let itr = if draws < wrap {
        draws
    } else {
        6 + (draws - 6) % (wrap - 6)
    };
    (itr / 6) as usize
}

/// The frame a counter shows of a picture with `frames` frames: past the last it loops back to
/// the first (`Set_Animation_Frame`).
pub fn looped_frame(counter: usize, frames: usize) -> usize {
    if frames == 0 {
        0
    } else {
        counter % frames
    }
}

/// How many times the water is drawn over the ground: the layers add up from the first frame
/// of the river or ocean picture, each draw taking the next frame (`FLIC_Draw` moves on).
pub fn water_layers(water_counter: usize) -> usize {
    water_counter / 2 + 1
}

/// Where the ward and enchantment pictures stand along the bottom, left to right: the
/// realm of a Spell Ward and its picture, then the spells.
pub const REALM_WARDS: [(&str, usize, f32); 5] = [
    ("death", 94, 8.0),
    ("chaos", 95, 29.0),
    ("nature", 98, 50.0),
    ("life", 96, 71.0),
    ("sorcery", 97, 92.0),
];
pub const CITY_BLESSINGS: [(&str, usize, f32); 4] = [
    ("natures_eye", 99, 113.0),
    ("inspirations", 100, 134.0),
    ("prosperity", 101, 155.0),
    ("consecration", 102, 176.0),
];
/// The height of the ward row below the top of the picture.
pub const WARD_ROW: f32 = 83.0;

/// The cityscape indexes Evil Presence marks (`bt_Shrine` to `bt_Parthenon`: the shrine, the
/// temple and the parthenon, not the cathedral) and the picture drawn over them.
pub const EVIL_PRESENCE_BUILDINGS: std::ops::RangeInclusive<i16> = 22..=24;
pub const EVIL_PRESENCE_PICTURE: usize = 82;

/// Rubble of a sold building, by the building's shape (`cityscap.lbx` 105 to 108).
pub fn rubble_picture(shape: u8) -> usize {
    105 + usize::from(shape.min(3))
}

/// Whether the build-in effect hides the pixel `(x, y)` of the new building at `percent`
/// (`Vanish_Bitmap`): each pixel has a threshold from one to a hundred that changes with every
/// draw, and it shows once the percent reaches it.
pub fn vanished(x: u32, y: u32, draw: u64, percent: u32) -> bool {
    let mut h = u64::from(x) << 32 ^ u64::from(y) ^ draw.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h % 100) as u32 + 1 > percent
}

/// Screen position of a cell relative to the cityscape's top-left, as in
/// `Cityscape_Draw_Buildings`: rows slant to the left, and each road takes two pixels.
pub fn cell_position(row: usize, col: usize) -> (f32, f32) {
    let r = row as i32;
    let c = col as i32;
    let mut y = (r - 1) * 5;
    for edge in [4, 8, 13] {
        if r > edge {
            y -= 2;
        }
    }
    y += 27;
    let mut x = c * 10;
    for edge in [3, 8, 13, 18] {
        if c > edge {
            x -= 2;
        }
    }
    x += 27 - (y - 27);
    (x as f32, y as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(population: u32, buildings: &[(i16, u8)]) -> CityInfo {
        CityInfo {
            population,
            buildings: buildings.to_vec(),
            seed: 0x2895 + 10 * 12,
            ..Default::default()
        }
    }

    fn count(g: &Grid, v: i16) -> usize {
        g.iter().flatten().filter(|&&c| c == v).count()
    }

    #[test]
    fn the_layout_is_the_same_every_time_and_follows_the_seed() {
        let i = info(6, &[(3, 2), (16, 1)]);
        assert_eq!(make_layout(&i), make_layout(&i));
        let other = CityInfo {
            seed: i.seed + 1,
            ..i.clone()
        };
        assert_ne!(make_layout(&i), make_layout(&other));
    }

    #[test]
    fn roads_and_corners_are_reserved() {
        let g = make_layout(&info(3, &[]));
        assert_eq!(g[0][3], ROAD);
        assert_eq!(g[5][8], ROAD);
        assert_eq!(g[14][10], ROAD);
        assert_eq!(g[4][4], ROAD);
        assert_eq!(g[14][0], UNUSABLE);
        assert_eq!(g[2][20], UNUSABLE);
        assert_eq!(g[0][18], UNUSABLE);
    }

    #[test]
    fn every_building_gets_a_cell_with_its_own_footprint() {
        // Barracks 3x2 (idx 3, shape 2), library 2x3 (16, 1), stables 3x3 (9, 3), a 2x2 (11, 0).
        let built = [(3, 2), (16, 1), (9, 3), (11, 0)];
        let g = make_layout(&info(8, &built));
        for (idx, _) in built {
            assert_eq!(count(&g, idx), 1, "building {idx}");
        }
    }

    #[test]
    fn a_typical_city_fits_everything_and_a_huge_one_fits_most() {
        // Ten ordinary buildings of mixed shapes always find room.
        let some: Vec<(i16, u8)> = [3, 5, 9, 16, 20, 22, 26, 29, 31, 32]
            .iter()
            .map(|&i| (i, (i % 4) as u8))
            .collect();
        // Every building at once is more than the grid holds, as in the original: the
        // placer gives up on what does not fit.
        let all: Vec<(i16, u8)> = (3..=34)
            .filter(|i| !(12..=14).contains(i))
            .map(|i| (i, (i % 4) as u8))
            .collect();
        for seed in 0..40 {
            let typical = make_layout(&CityInfo {
                population: 12,
                buildings: some.clone(),
                seed,
                ..Default::default()
            });
            for (idx, _) in &some {
                assert_eq!(
                    count(&typical, *idx),
                    1,
                    "seed {seed}: building {idx} was lost"
                );
            }
            let huge = make_layout(&CityInfo {
                population: 20,
                buildings: all.clone(),
                seed,
                ..Default::default()
            });
            assert_eq!(huge[0][4], ROAD);
            assert_eq!(huge[14][0], UNUSABLE);
            let placed = all
                .iter()
                .filter(|(idx, _)| count(&huge, *idx) == 1)
                .count();
            assert!(
                placed >= 10,
                "seed {seed}: only {placed} of {} placed",
                all.len()
            );
        }
    }

    #[test]
    fn houses_follow_the_population_and_trees_the_terrain() {
        let small = make_layout(&info(1, &[]));
        let big = make_layout(&info(20, &[]));
        assert!(count(&big, HOUSE) > count(&small, HOUSE));
        assert!(count(&small, HOUSE) >= 1);
        let forest = CityInfo {
            many_trees: true,
            ..info(3, &[])
        };
        let (plain, wood) = (make_layout(&info(3, &[])), make_layout(&forest));
        assert!(count(&wood, TREE) > count(&plain, TREE) + 20);
        let rocky = CityInfo {
            many_rocks: true,
            ..info(3, &[])
        };
        assert!(count(&make_layout(&rocky), ROCK) >= 15);
    }

    #[test]
    fn the_fortress_and_the_port_take_fixed_places() {
        let g = make_layout(&CityInfo {
            fortress: true,
            ..info(3, &[(13, 2)])
        });
        assert_eq!(g[8][10], FORTRESS);
        assert_eq!(g[8][2], 13, "the shipyard sits at the water's edge");
        let g = make_layout(&CityInfo {
            magic: vec![SUMMONING_CIRCLE, EARTH_GATE],
            ..info(3, &[])
        });
        assert_eq!(count(&g, SUMMONING_CIRCLE), 1);
        assert_eq!(count(&g, EARTH_GATE), 1);
    }

    #[test]
    fn cells_are_drawn_sheared_with_gaps_at_the_roads() {
        assert_eq!(cell_position(1, 0), (27.0, 27.0));
        // One row down moves five pixels down and five left.
        assert_eq!(cell_position(2, 0), (22.0, 32.0));
        // Crossing a road column costs two pixels.
        assert_eq!(cell_position(1, 3), (57.0, 27.0));
        assert_eq!(cell_position(1, 4), (65.0, 27.0));
        assert_eq!(sprite_of(3), Some((45, 32.0)));
        assert_eq!(sprite_of(31), Some((78, 32.0)));
        assert_eq!(sprite_of(34), Some((75, 32.0)));
        assert_eq!(sprite_of(FORTRESS), Some((40, 47.0)));
        assert_eq!(sprite_of(35), None);
    }
    #[test]
    fn counters_loop_like_the_originals() {
        // The first draw shows counter 0; after that the counter is the draw count over six.
        assert_eq!(anim_counter(0, 60), 0);
        assert_eq!(anim_counter(5, 60), 0);
        assert_eq!(anim_counter(6, 60), 1);
        assert_eq!(anim_counter(59, 60), 9);
        // At the wrap the count goes back to six, so counter 1 again, never 0.
        assert_eq!(anim_counter(60, 60), 1);
        assert_eq!(anim_counter(113, 60), 9);
        assert_eq!(anim_counter(114, 60), 1);
        assert_eq!(anim_counter(83, 84), 13);
        assert_eq!(anim_counter(84, 84), 1);
        assert_eq!(looped_frame(9, 9), 0);
        assert_eq!(looped_frame(4, 9), 4);
        assert_eq!(water_layers(0), 1);
        assert_eq!(water_layers(13), 7);
    }

    #[test]
    fn the_new_building_fades_in() {
        let shown = |p| {
            (0..40u32)
                .flat_map(|x| (0..40u32).map(move |y| (x, y)))
                .filter(|&(x, y)| !vanished(x, y, 3, p))
                .count()
        };
        assert!(shown(0) < 40, "{}", shown(0));
        assert_eq!(shown(100), 1600);
        assert!(shown(30) < shown(60) && shown(60) < shown(90));
    }
}
