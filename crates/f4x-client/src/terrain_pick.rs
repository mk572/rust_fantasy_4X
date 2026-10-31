//! Which of the original's terrain pictures a map square gets.
//!
//! The original stores a terrain *type number* per square (762 of them per plane): the base
//! kinds, several look-alike variants of grass, forest, swamp, desert and tundra, edge pieces
//! for mountains, hills, deserts, tundra and ocean shores chosen by which of the eight
//! neighbours differ, and river pieces by which of the four sides the river leaves through.
//! Here the map keeps only the class (and flags for rivers and volcanoes), so the type is
//! worked out when drawing, with the original's tables and rules (`Simtex_Autotiling` and its
//! passes in ReMoM `MAPGEN.c`). Look-alike variants are picked from the square's position so
//! they stay put.

use f4x_core::content::Content;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Grid;

pub const GRASS: [usize; 3] = [0xA2, 0xAC, 0xAD];
pub const FOREST: [usize; 3] = [0xA3, 0xB7, 0xB8];
pub const SWAMP: [usize; 3] = [0xA6, 0xB1, 0xB2];
pub const DESERT: [usize; 4] = [0xA5, 0xAE, 0xAF, 0xB0];
pub const TUNDRA: [usize; 3] = [0xA7, 0xB5, 0xB6];
pub const MOUNTAIN: usize = 0xA4;
pub const HILLS: usize = 0xAB;
pub const VOLCANO: usize = 0xB3;
pub const SORCERY_NODE: usize = 0xA8;
pub const NATURE_NODE: usize = 0xA9;
pub const CHAOS_NODE: usize = 0xAA;
/// First of the desert edge pieces (`_Desert00001000`), whose table index is 2-based.
const DESERT_FIRST: usize = 0x124;
/// Tundra edge pieces start at 602 (`_Tundra00001000`) for a table value of 2.
const TUNDRA_BASE: usize = 600;
/// The mountain edge pieces, and the hills that follow them sixteen types later.
const MOUNTAIN_EDGES: std::ops::RangeInclusive<usize> = 0x103..=0x112;

/// The shore pieces with a river reaching the sea (`_ShoreNNNNNNNN` with an `R` in place of the
/// digit of the side the river comes from), by pattern, with their terrain type numbers
/// (`TerrType.h`). The pattern's digits are the neighbours that are land, in the order of the
/// mask bits, NW N NE E SE S SW W.
const RIVER_MOUTHS: [(&str, usize); 176] = [
    ("1R00000R", 201),
    ("1R10000R", 202),
    ("1R00001R", 203),
    ("1R10001R", 204),
    ("000R1R00", 205),
    ("000R1R10", 206),
    ("001R1R00", 207),
    ("001R1R10", 208),
    ("0R1R0000", 209),
    ("0R1R1000", 210),
    ("1R1R0000", 211),
    ("1R1R1000", 212),
    ("00000R1R", 213),
    ("00001R1R", 214),
    ("10000R1R", 215),
    ("10001R1R", 216),
    ("00001R10", 217),
    ("00001R00", 218),
    ("00000R10", 219),
    ("00000R00", 220),
    ("1000001R", 221),
    ("0000001R", 222),
    ("1000000R", 223),
    ("0000000R", 224),
    ("1R100000", 225),
    ("1R000000", 226),
    ("0R100000", 227),
    ("0R000000", 228),
    ("001R1000", 229),
    ("001R0000", 230),
    ("000R1000", 231),
    ("000R0000", 232),
    ("00011R11", 452),
    ("1100011R", 453),
    ("1R110001", 454),
    ("011R1100", 455),
    ("10011R11", 456),
    ("1110011R", 457),
    ("1R111001", 458),
    ("011R1110", 459),
    ("00111R11", 460),
    ("1100111R", 461),
    ("1R110011", 462),
    ("111R1100", 463),
    ("10111R11", 464),
    ("1110111R", 465),
    ("1R111011", 466),
    ("111R1110", 467),
    ("1100000R", 473),
    ("1110000R", 474),
    ("1100001R", 475),
    ("1110001R", 476),
    ("00011R00", 477),
    ("00111R00", 478),
    ("00011R10", 479),
    ("00111R10", 480),
    ("0R110000", 481),
    ("0R111000", 482),
    ("1R110000", 483),
    ("1R111000", 484),
    ("00000R11", 485),
    ("00001R11", 486),
    ("10000R11", 487),
    ("10001R11", 488),
    ("1R000001", 489),
    ("1R100001", 490),
    ("1R000011", 491),
    ("1R100011", 492),
    ("000R1100", 493),
    ("001R1100", 494),
    ("000R1110", 495),
    ("001R1110", 496),
    ("011R0000", 497),
    ("011R1000", 498),
    ("111R0000", 499),
    ("111R1000", 500),
    ("0000011R", 501),
    ("0000111R", 502),
    ("1000011R", 503),
    ("1000111R", 504),
    ("0001111R", 505),
    ("1R000111", 506),
    ("111R0001", 507),
    ("0R111100", 508),
    ("1001111R", 509),
    ("1R100111", 510),
    ("111R1001", 511),
    ("0R111110", 512),
    ("0011111R", 513),
    ("1R001111", 514),
    ("111R0011", 515),
    ("1R111100", 516),
    ("1011111R", 517),
    ("1R101111", 518),
    ("111R1011", 519),
    ("1R111110", 520),
    ("000R1111", 521),
    ("11000R11", 522),
    ("1111000R", 523),
    ("01111R00", 524),
    ("100R1111", 525),
    ("11100R11", 526),
    ("1111100R", 527),
    ("01111R10", 528),
    ("001R1111", 529),
    ("11001R11", 530),
    ("1111001R", 531),
    ("11111R00", 532),
    ("101R1111", 533),
    ("11101R11", 534),
    ("1111101R", 535),
    ("11111R10", 536),
    ("1R101110", 537),
    ("1R100110", 538),
    ("1R101100", 539),
    ("1R100100", 540),
    ("1R001110", 541),
    ("1R000110", 542),
    ("1R001100", 543),
    ("1R000100", 544),
    ("0R101110", 545),
    ("0R100110", 546),
    ("0R101100", 547),
    ("0R100100", 548),
    ("0R001110", 549),
    ("0R000110", 550),
    ("0R001100", 551),
    ("0R000100", 552),
    ("11101R10", 553),
    ("11100R10", 554),
    ("11101R00", 555),
    ("11100R00", 556),
    ("11001R10", 557),
    ("11000R10", 558),
    ("11001R00", 559),
    ("11000R00", 560),
    ("01101R10", 561),
    ("01100R10", 562),
    ("01101R00", 563),
    ("01100R00", 564),
    ("01001R10", 565),
    ("01000R10", 566),
    ("01001R00", 567),
    ("01000R00", 568),
    ("1001001R", 569),
    ("1001101R", 570),
    ("1011001R", 571),
    ("1011101R", 572),
    ("1001000R", 573),
    ("1001100R", 574),
    ("1011000R", 575),
    ("1011100R", 576),
    ("0001001R", 577),
    ("0001101R", 578),
    ("0011001R", 579),
    ("0011101R", 580),
    ("0001000R", 581),
    ("0001100R", 582),
    ("0011000R", 583),
    ("0011100R", 584),
    ("100R0011", 585),
    ("100R1011", 586),
    ("101R0011", 587),
    ("101R1011", 588),
    ("100R0001", 589),
    ("100R1001", 590),
    ("101R0001", 591),
    ("101R1001", 592),
    ("000R0011", 593),
    ("000R1011", 594),
    ("001R0011", 595),
    ("001R1011", 596),
    ("000R0001", 597),
    ("000R1001", 598),
    ("001R0001", 599),
    ("001R1001", 600),
];

/// The eight neighbours in the order of the mask bits: NW 1, N 2, NE 4, E 8, SE 16, S 32,
/// SW 64, W 128.
pub const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
];

pub fn class_at(grid: &Grid, content: &Content, x: i32, y: i32) -> Option<TerrainClass> {
    grid.get(x, y)
        .and_then(|t| content.terrains.get(t))
        .map(|d| d.class)
}

/// A number from the square's place, for choosing among look-alikes.
pub fn variation(x: i32, y: i32, plane: usize) -> usize {
    let h = (x as u32).wrapping_mul(73_856_093)
        ^ (y as u32).wrapping_mul(19_349_663)
        ^ (plane as u32).wrapping_mul(83_492_791);
    (h ^ (h >> 13)).wrapping_mul(2_654_435_761) as usize >> 4
}

/// The mask of neighbours for which `test(class)` holds; squares off the map never count.
fn mask_of(
    grid: &Grid,
    content: &Content,
    x: i32,
    y: i32,
    test: impl Fn(TerrainClass) -> bool,
) -> usize {
    AROUND
        .iter()
        .enumerate()
        .filter(|&(_, &(dx, dy))| class_at(grid, content, x + dx, y + dy).is_some_and(&test))
        .fold(0, |m, (bit, _)| m | (1 << bit))
}

/// The shore piece of a sea square whose land neighbours are `land` (the mask of table 0) and
/// that a river runs into from the sides in `rivers` (bits N 1, E 2, S 4, W 8, each a side with
/// land). The original's `River_Autotile` swaps the `R` piece in for the plain one; when it has
/// no piece for the combination, the river-mouth pieces for fewer sides are tried, and last the
/// plain shore.
pub fn shore_with_rivers(land: usize, rivers: usize, plain: usize) -> usize {
    // Pattern position of each cardinal side: N 1, E 3, S 5, W 7.
    const SIDES: [(usize, usize); 4] = [(1, 1), (2, 3), (4, 5), (8, 7)];
    // Only a side with land can have a river.
    let cardinal_land = (land >> 1 & 1) | (land >> 2 & 2) | (land >> 3 & 4) | (land >> 4 & 8);
    let mut sides = rivers & cardinal_land;
    loop {
        if sides == 0 {
            return plain;
        }
        let mut name: Vec<u8> = (0..8)
            .map(|i| if land >> i & 1 == 1 { b'1' } else { b'0' })
            .collect();
        for (bit, at) in SIDES {
            if sides & bit != 0 {
                name[at] = b'R';
            }
        }
        if let Some(&(_, t)) = RIVER_MOUTHS
            .iter()
            .find(|(n, _)| n.as_bytes() == name.as_slice())
        {
            return t;
        }
        // Drop the highest side and try again.
        sides &= !(1 << (usize::BITS - 1 - sides.leading_zeros()));
    }
}

/// The lake (a sea square with land all round) that a river runs into, for the sides in
/// `rivers` (bits N 1, E 2, S 4, W 8): `_1LakeRiv_N/E/S/W`. Where more than one river runs in the
/// original keeps one and turns the others to grassland (`River_Autotile`, block 1); ours keeps
/// them all, so the same side is drawn, the north one first, then west, south, east.
pub fn lake_with_river(rivers: usize) -> usize {
    match rivers {
        r if r & 1 != 0 => LAKE_RIVER_N,
        r if r & 8 != 0 => LAKE_RIVER_W,
        r if r & 4 != 0 => LAKE_RIVER_S,
        _ => LAKE_RIVER_E,
    }
}

const LAKE_RIVER_W: usize = 0xC5;
const LAKE_RIVER_N: usize = 0xC6;
const LAKE_RIVER_E: usize = 0xC7;
const LAKE_RIVER_S: usize = 0xC8;

/// The terrain type of a square on Arcanus (add 762 for Myrror). `table(t, mask)` is one of
/// the original's autotile tables.
pub fn ground_type(
    grid: &Grid,
    content: &Content,
    x: i32,
    y: i32,
    plane: usize,
    table: &dyn Fn(usize, usize) -> usize,
) -> Option<usize> {
    let class = class_at(grid, content, x, y)?;
    let v = variation(x, y, plane);
    Some(match class {
        // Water: the shore piece for the land around it (a lake is water with land all round).
        TerrainClass::Ocean => {
            let land = mask_of(grid, content, x, y, |c| c != TerrainClass::Ocean);
            let plain = table(0, land);
            // A river reaching the sea gives the shore piece its mouth.
            let rivers = [(0, -1, 1), (1, 0, 2), (0, 1, 4), (-1, 0, 8)]
                .iter()
                .filter(|&&(dx, dy, _)| {
                    grid.has_river(x + dx, y + dy)
                        && class_at(grid, content, x + dx, y + dy)
                            .is_some_and(|c| c != TerrainClass::Ocean)
                })
                .fold(0, |m, &(_, _, bit)| m | bit);
            if land == 255 && rivers != 0 {
                lake_with_river(rivers)
            } else {
                shore_with_rivers(land, rivers, plain)
            }
        }
        TerrainClass::SorceryNode => SORCERY_NODE,
        TerrainClass::NatureNode => NATURE_NODE,
        TerrainClass::ChaosNode => CHAOS_NODE,
        TerrainClass::Grassland => GRASS[v % 3],
        TerrainClass::Forest => FOREST[v % 3],
        TerrainClass::Swamp => SWAMP[v % 3],
        TerrainClass::Desert => {
            let m = mask_of(grid, content, x, y, |c| c != TerrainClass::Desert);
            if m > 0 {
                DESERT_FIRST + table(0, m).saturating_sub(2)
            } else {
                DESERT[v % 4]
            }
        }
        TerrainClass::Tundra => {
            let m = mask_of(grid, content, x, y, |c| c != TerrainClass::Tundra);
            if m > 0 {
                TUNDRA_BASE + table(0, m)
            } else {
                TUNDRA[v % 3]
            }
        }
        TerrainClass::Mountains => {
            let m = mask_of(grid, content, x, y, |c| c == TerrainClass::Mountains);
            let t = table(1, m);
            if m > 0 && MOUNTAIN_EDGES.contains(&t) {
                t
            } else {
                MOUNTAIN
            }
        }
        TerrainClass::Hills => {
            let m = mask_of(grid, content, x, y, |c| c == TerrainClass::Hills);
            let t = table(1, m);
            // The hill pieces sit sixteen types after the mountain pieces.
            if m > 0 && MOUNTAIN_EDGES.contains(&t) {
                t + 16
            } else {
                HILLS
            }
        }
    })
}

/// The river piece for the sides a river leaves a square by (north, east, south, west), picked
/// among its look-alikes by `v`.
pub fn river_type(n: bool, e: bool, s: bool, w: bool, v: usize) -> usize {
    let pick = |list: &[usize]| list[v % list.len()];
    match (n, e, s, w) {
        (true, false, false, false) => 0xBB,
        (false, true, false, false) => 0xBC,
        (false, false, true, false) => 0xB9,
        (false, false, false, true) => 0xBA,
        (true, true, false, false) => pick(&[0xBD, 0xC1, 0xE9]),
        (false, false, true, true) => pick(&[0xBE, 0xC2, 0xEA]),
        (false, true, true, false) => pick(&[0xBF, 0xC3, 0xEB]),
        (true, false, false, true) => pick(&[0xC0, 0xC4, 0xEC]),
        (true, false, true, false) => pick(&[0xED, 0xEE, 0xEF]),
        (false, true, false, true) => pick(&[0xF0, 0xF1, 0xF2]),
        (true, true, false, true) => pick(&[0xF3, 0xF4, 0xF5, 0xF6]),
        (false, true, true, true) => pick(&[0xF7, 0xF8, 0xF9, 0xFA]),
        (true, true, true, false) => pick(&[0xFB, 0xFC, 0xFD, 0xFE]),
        (true, false, true, true) => pick(&[0xFF, 0x100, 0x101, 0x102]),
        (true, true, true, true) => pick(&[0x1D4, 0x1D5, 0x1D6, 0x1D7, 0x1D8]),
        (false, false, false, false) => 0xB9,
    }
}

/// Which `mapback.lbx` picture shows a mineral or a living thing, by the feature's id.
pub fn mineral_sprite(id: &str) -> Option<usize> {
    const TABLE: [(&str, usize); 11] = [
        ("coal", 78),
        ("iron", 79),
        ("silver", 80),
        ("gold", 81),
        ("gem", 82),
        ("mithril", 83),
        ("adamantium", 84),
        ("quork", 85),
        ("crysx", 86),
        ("nightshade", 91),
        ("wild_game", 92),
    ];
    TABLE.iter().find(|(k, _)| id.contains(k)).map(|&(_, e)| e)
}

/// Which `mapback.lbx` picture shows an encounter site, by the feature's id.
pub fn site_sprite(id: &str) -> usize {
    if id.contains("fallen") {
        75
    } else if id.contains("temple") {
        72
    } else if id.contains("keep") {
        73
    } else if id.contains("ruin") || id.contains("dungeon") {
        74
    } else {
        71
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::terrain::{TerrainClass as C, Terrains};

    /// A content with the eight terrain classes in a known order, and a grid built from rows
    /// of letters: o ocean, g grass, f forest, h hills, m mountains, s swamp, d desert, t tundra.
    fn world(rows: &[&str]) -> (Grid, Content) {
        let content = crate::test_app::shipped::content().clone();
        let terrains: &Terrains = &content.terrains;
        let id = |c: C| {
            terrains
                .first_of_class(c, f4x_core::map::Plane::Arcanus)
                .expect("the shipped terrains have every class")
        };
        let mut grid = Grid::new(rows[0].len() as u32, rows.len() as u32, id(C::Grassland));
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let class = match ch {
                    'o' => C::Ocean,
                    'g' => C::Grassland,
                    'f' => C::Forest,
                    'h' => C::Hills,
                    'm' => C::Mountains,
                    's' => C::Swamp,
                    'd' => C::Desert,
                    _ => C::Tundra,
                };
                grid.set(x as u32, y as u32, id(class));
            }
        }
        (grid, content)
    }

    /// Tables that return the mask itself plus a marker, so the tests see what was asked.
    fn table(t: usize, mask: usize) -> usize {
        match t {
            1 => 0x103 + (mask % 16),
            _ => 2 + mask,
        }
    }

    fn ground(g: &Grid, c: &Content, x: i32, y: i32) -> usize {
        ground_type(g, c, x, y, 0, &table).expect("the square has a ground type")
    }

    #[test]
    fn variants_are_stable_and_vary_across_squares() {
        assert_eq!(variation(3, 4, 0), variation(3, 4, 0));
        let picks: std::collections::HashSet<usize> =
            (0..40).map(|x| variation(x, 7, 0) % 3).collect();
        assert_eq!(picks.len(), 3, "all three look-alikes occur");
        assert_ne!(variation(3, 4, 0), variation(3, 4, 1));
    }

    #[test]
    fn plain_land_gets_one_of_its_variants() {
        let (g, c) = world(&["ggg", "ggg", "ggg"]);
        let t = ground(&g, &c, 1, 1);
        assert!(GRASS.contains(&t));
        let (g, c) = world(&["fff", "fff", "fff"]);
        assert!(FOREST.contains(&ground(&g, &c, 1, 1)));
        let (g, c) = world(&["sss", "sss", "sss"]);
        assert!(SWAMP.contains(&ground(&g, &c, 1, 1)));
    }

    #[test]
    fn a_lone_mountain_or_hill_has_no_edges_and_a_group_does() {
        let (g, c) = world(&["ggg", "gmg", "ggg"]);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(MOUNTAIN));
        let (g, c) = world(&["ggg", "mmg", "ggg"]);
        let t = ground(&g, &c, 0, 1);
        assert!((0x103..=0x112).contains(&t), "an edge piece: {t:#x}");
        let (g, c) = world(&["ggg", "hhg", "ggg"]);
        let t = ground(&g, &c, 0, 1);
        assert!((0x113..=0x122).contains(&t), "a hill piece: {t:#x}");
        let (g, c) = world(&["ggg", "ghg", "ggg"]);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(HILLS));
    }

    #[test]
    fn deserts_and_tundra_edge_against_other_ground() {
        let (g, c) = world(&["ddd", "ddd", "ddd"]);
        assert!(DESERT.contains(&ground(&g, &c, 1, 1)));
        let (g, c) = world(&["ggg", "gdd", "ggg"]);
        let t = ground(&g, &c, 1, 1);
        assert!(t >= 0x124, "an edge piece: {t:#x}");
        let (g, c) = world(&["ttt", "ttt", "ttt"]);
        assert!(TUNDRA.contains(&ground(&g, &c, 1, 1)));
        let (g, c) = world(&["ggg", "ttt", "ttt"]);
        assert!(ground(&g, &c, 1, 1) >= 602);
    }

    #[test]
    fn water_asks_the_shore_table_with_its_land_neighbours() {
        let (g, c) = world(&["ooo", "ooo", "ooo"]);
        assert_eq!(
            ground_type(&g, &c, 1, 1, 0, &table),
            Some(2),
            "open sea: mask 0"
        );
        let (g, c) = world(&["ooo", "ooo", "ogo"]);
        assert_eq!(
            ground_type(&g, &c, 1, 1, 0, &table),
            Some(2 + 32),
            "land to the south is bit 32"
        );
        let (g, c) = world(&["ggg", "gog", "ggg"]);
        assert_eq!(
            ground_type(&g, &c, 1, 1, 0, &table),
            Some(2 + 255),
            "a lake: land all round"
        );
    }

    #[test]
    fn a_river_into_a_lake_gives_the_lake_piece_of_its_side() {
        assert_eq!(lake_with_river(1), 0xC6);
        assert_eq!(lake_with_river(2), 0xC7);
        assert_eq!(lake_with_river(4), 0xC8);
        assert_eq!(lake_with_river(8), 0xC5);
        assert_eq!(lake_with_river(2 | 4), 0xC8, "south wins over east");
        assert_eq!(lake_with_river(2 | 8), 0xC5, "west wins over east");
        assert_eq!(lake_with_river(1 | 4 | 8), 0xC6, "north wins");
        let (mut g, c) = world(&["ggg", "gog", "ggg"]);
        g.set_river(1, 0, true);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(0xC6));
        g.set_river(1, 0, false);
        g.set_river(2, 1, true);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(0xC7));
    }

    #[test]
    fn a_river_into_the_sea_gives_the_shore_its_mouth() {
        // Land to the south and south-east (mask 48, plain piece 3); a river on the south
        // square is `_Shore00001R00`.
        assert_eq!(shore_with_rivers(48, 4, 3), 0xDA);
        // Land north-west, north and west: a river from the north is `_Shore1R000001`.
        let n_mouth = RIVER_MOUTHS
            .iter()
            .find(|(n, _)| *n == "1R000001")
            .expect("the piece for a river from the north")
            .1;
        assert_eq!(shore_with_rivers(1 | 2 | 128, 1, 99), n_mouth);
        // No river, or a combination without a piece, keeps the plain piece.
        assert_eq!(shore_with_rivers(48, 0, 3), 3);
        assert_eq!(shore_with_rivers(48, 1, 77), 77, "no piece for it");
        let (mut g, c) = world(&["ooo", "ooo", "ogg"]);
        g.set_river(1, 2, true);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(0xDA));
    }

    #[test]
    fn river_pieces_match_the_sides_they_leave_by() {
        assert_eq!(river_type(true, false, false, false, 0), 0xBB);
        assert_eq!(river_type(false, false, true, false, 0), 0xB9);
        assert!([0xED, 0xEE, 0xEF].contains(&river_type(true, false, true, false, 5)));
        assert!((0x1D4..=0x1D8).contains(&river_type(true, true, true, true, 3)));
        assert!((0xF3..=0xF6).contains(&river_type(true, true, false, true, 2)));
        // Every possible combination has a piece.
        for bits in 0..16 {
            let t = river_type(bits & 1 > 0, bits & 2 > 0, bits & 4 > 0, bits & 8 > 0, 7);
            assert!((0xB9..=0x102).contains(&t) || (0x1D4..=0x1D8).contains(&t));
        }
    }

    #[test]
    fn minerals_and_sites_find_their_pictures() {
        assert_eq!(mineral_sprite("gold_ore"), Some(81));
        assert_eq!(mineral_sprite("gem"), Some(82));
        assert_eq!(mineral_sprite("adamantium_ore"), Some(84));
        assert_eq!(mineral_sprite("wild_game"), Some(92));
        assert_eq!(mineral_sprite("tower"), None);
        assert_eq!(site_sprite("monster_lair"), 71);
        assert_eq!(site_sprite("ancient_temple"), 72);
        assert_eq!(site_sprite("fallen_temple"), 75);
        assert_eq!(site_sprite("abandoned_keep"), 73);
        assert_eq!(site_sprite("dungeon"), 74);
    }
}
