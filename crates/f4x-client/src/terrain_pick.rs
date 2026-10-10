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

use f4x_core::content::{Content, TerrainClass};
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
        TerrainClass::Ocean => table(
            0,
            mask_of(grid, content, x, y, |c| c != TerrainClass::Ocean),
        ),
        _ if grid.volcano(x, y).is_some() => VOLCANO,
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

/// The node's own ground piece.
pub fn node_type(id: &str) -> usize {
    match id {
        "sorcery_node" => SORCERY_NODE,
        "nature_node" => NATURE_NODE,
        _ => CHAOS_NODE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::content::{TerrainClass as C, Terrains};

    /// A content with the eight terrain classes in a known order, and a grid built from rows
    /// of letters: o ocean, g grass, f forest, h hills, m mountains, s swamp, d desert, t tundra.
    fn world(rows: &[&str]) -> (Grid, Content) {
        let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
        let terrains: &Terrains = &content.terrains;
        let id = |c: C| {
            terrains
                .first_of_class(c, f4x_core::map::Plane::Arcanus)
                .unwrap()
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
        let t = ground_type(&g, &c, 1, 1, 0, &table).unwrap();
        assert!(GRASS.contains(&t));
        let (g, c) = world(&["fff", "fff", "fff"]);
        assert!(FOREST.contains(&ground_type(&g, &c, 1, 1, 0, &table).unwrap()));
        let (g, c) = world(&["sss", "sss", "sss"]);
        assert!(SWAMP.contains(&ground_type(&g, &c, 1, 1, 0, &table).unwrap()));
    }

    #[test]
    fn a_lone_mountain_or_hill_has_no_edges_and_a_group_does() {
        let (g, c) = world(&["ggg", "gmg", "ggg"]);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(MOUNTAIN));
        let (g, c) = world(&["ggg", "mmg", "ggg"]);
        let t = ground_type(&g, &c, 0, 1, 0, &table).unwrap();
        assert!((0x103..=0x112).contains(&t), "an edge piece: {t:#x}");
        let (g, c) = world(&["ggg", "hhg", "ggg"]);
        let t = ground_type(&g, &c, 0, 1, 0, &table).unwrap();
        assert!((0x113..=0x122).contains(&t), "a hill piece: {t:#x}");
        let (g, c) = world(&["ggg", "ghg", "ggg"]);
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(HILLS));
    }

    #[test]
    fn deserts_and_tundra_edge_against_other_ground() {
        let (g, c) = world(&["ddd", "ddd", "ddd"]);
        assert!(DESERT.contains(&ground_type(&g, &c, 1, 1, 0, &table).unwrap()));
        let (g, c) = world(&["ggg", "gdd", "ggg"]);
        let t = ground_type(&g, &c, 1, 1, 0, &table).unwrap();
        assert!(t >= 0x124, "an edge piece: {t:#x}");
        let (g, c) = world(&["ttt", "ttt", "ttt"]);
        assert!(TUNDRA.contains(&ground_type(&g, &c, 1, 1, 0, &table).unwrap()));
        let (g, c) = world(&["ggg", "ttt", "ttt"]);
        assert!(ground_type(&g, &c, 1, 1, 0, &table).unwrap() >= 602);
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
    fn volcanoes_have_their_own_piece() {
        let (mut g, c) = world(&["ggg", "gmg", "ggg"]);
        g.set_volcano(1, 1, Some(0));
        assert_eq!(ground_type(&g, &c, 1, 1, 0, &table), Some(VOLCANO));
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
        assert_eq!(node_type("chaos_node"), CHAOS_NODE);
    }
}
