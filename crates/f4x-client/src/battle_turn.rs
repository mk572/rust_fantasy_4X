//! The turn of the battle view (task 450): the field seen from another side, a quarter turn at a
//! time. The turn is only a view. The battle keeps its own squares; every picture, click and
//! animation asks this module where a square of the field is drawn, and the drawn square is the
//! field's square turned about the middle of the playable band.
//!
//! The turn works on the original's cells `(cgx, cgy)` (`f4x_battle::map::layout`; the core's
//! square is `(20 - cgx, cgy)`). One quarter turn clockwise on the screen is
//! `(cgx, cgy) -> (SUM - cgy, cgx + DIFF)`, about the middle of the field (`(8.5, 11.5)`: the field
//! has the columns `cgx` -3 to 20 and the rows 0 to 23), which stays where it is on the screen, so
//! the turned field is drawn about the same spot. Squares map to squares, so what stands on a square (units, trees, rocks, houses) is
//! drawn upright on the turned square.
//!
//! The pictures that are drawn for one point of view cannot be turned as pictures. The field's
//! rivers, roads and the edges of its ground name the neighbours they join (`N`, `E`, `S`, `W` of
//! the original's frame), so each is swapped for the picture that joins the turned neighbours; the
//! original has all of them. A wall or the city's base is the same ring of squares turned about
//! its own middle, so the picture of each turned wall square is the one the same place of the
//! ring has when it is not turned (`ring_shift`).

use f4x_battle::map::layout::{EXTRA_X, W};
use f4x_battle::map::layout::{MAX_CGX_CITY, MAX_CGY_CITY, MIN_CGX_CITY, MIN_CGY_CITY, RIVER_TILE};
use f4x_battle::map::GRID_H;

/// The field's last and first column (`cgx`) and last row, in the original's frame.
const LAST_X: i32 = W - 1;
const FIRST_X: i32 = -EXTRA_X;
const LAST_Y: i32 = GRID_H - 1;
/// A quarter turn about the field's middle maps `(x, y)` to `(SUM - y, x + DIFF)`: the field's
/// middle is `((FIRST_X + LAST_X) / 2, LAST_Y / 2)`, which is `(8.5, 11.5)`, and the sum and the
/// difference of its coordinates are whole numbers.
pub(crate) const SUM: i32 = (FIRST_X + LAST_X + LAST_Y) / 2;
pub(crate) const DIFF: i32 = (LAST_Y - FIRST_X - LAST_X) / 2;

/// How far the view is turned, in quarter turns clockwise (0 to 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Turn(u8);

impl Turn {
    /// The field as the original shows it.
    pub(crate) const NONE: Self = Self(0);

    /// One more quarter turn, or one less when `back`.
    pub(crate) fn step(self, back: bool) -> Self {
        Self((self.0 + if back { 3 } else { 1 }) % 4)
    }

    pub(crate) fn quarters(self) -> u8 {
        self.0
    }

    /// A cell (or a point between cells) of the original's frame, where the view draws it.
    pub(crate) fn cg_f(self, mut p: (f32, f32)) -> (f32, f32) {
        for _ in 0..self.0 {
            p = (SUM as f32 - p.1, p.0 + DIFF as f32);
        }
        p
    }

    /// The same for a whole cell.
    pub(crate) fn cg(self, mut p: (i32, i32)) -> (i32, i32) {
        for _ in 0..self.0 {
            p = (SUM - p.1, p.0 + DIFF);
        }
        p
    }

    /// The cell that the view draws at `p`, the other way round.
    pub(crate) fn uncg(self, mut p: (i32, i32)) -> (i32, i32) {
        for _ in 0..self.0 {
            p = (p.1 - DIFF, SUM - p.0);
        }
        p
    }

    /// Where the view draws the core's square (or a point between squares), as a cell of the
    /// original's frame.
    pub(crate) fn of_core(self, pos: (f32, f32)) -> (f32, f32) {
        self.cg_f(((W - 1) as f32 - pos.0, pos.1))
    }

    /// Which of the eight pictures of a figure (0 north, clockwise) shows it looking where the
    /// field's `facing` looks once the view is turned: a quarter turn is two of the eight.
    pub(crate) fn facing(self, facing: u8) -> u8 {
        (facing + 2 * self.0) % 8
    }

    /// The offset inside a cell (0 to 15 along each axis) of a tree or a rock, in the turned cell.
    pub(crate) fn sub(self, mut sub: (i32, i32)) -> (i32, i32) {
        for _ in 0..self.0 {
            sub = (15 - sub.1, sub.0);
        }
        sub
    }

    /// How far the turned ring of the city's squares (and its walls) lies from the ring in the
    /// original's place, as a change of cell.
    pub(crate) fn ring_shift(self) -> (i32, i32) {
        let a = self.cg((MIN_CGX_CITY, MIN_CGY_CITY));
        let b = self.cg((MAX_CGX_CITY, MAX_CGY_CITY));
        (a.0.min(b.0) - MIN_CGX_CITY, a.1.min(b.1) - MIN_CGY_CITY)
    }

    /// The cell of the ring in the original's place that has the same place in the ring as the
    /// turned cell `cell` has in the turned ring: the cell whose wall picture belongs there.
    pub(crate) fn ring_cell(self, cell: (i32, i32)) -> (i32, i32) {
        let (dx, dy) = self.ring_shift();
        (cell.0 - dx, cell.1 - dy)
    }

    /// The picture of a river (its shape 0 to 5: left-right, up-down, up-left, up-right,
    /// down-right, down-left) or the road piece of the same order, for the turned neighbours.
    pub(crate) fn piece(self, mut piece: usize) -> usize {
        for _ in 0..self.0 {
            piece = match piece {
                0 => 1,
                1 => 0,
                2 => 3,
                3 => 4,
                4 => 5,
                5 => 2,
                other => other,
            };
        }
        piece
    }

    /// The ground picture of a cell for the turned neighbours: the pictures that name where the
    /// dirt or the rough ground is (8 to 47), and the six river pictures (from `RIVER_TILE`).
    /// Every other picture is the same from every side.
    pub(crate) fn ground(self, tile: usize) -> usize {
        let river = RIVER_TILE as usize;
        match tile {
            8..=31 => {
                let (pair, variant) = ((tile - 8) / 2, (tile - 8) % 2);
                8 + 2 * self.edge(pair) + variant
            }
            32..=47 => 32 + self.rough(tile - 32),
            t if t >= river => river + (t - river) / 6 * 6 + self.piece((t - river) % 6),
            t => t,
        }
    }

    /// A pair of grass pictures edged by dirt (`pair` 0 to 11, from 8) for the turned neighbours.
    fn edge(self, mut pair: usize) -> usize {
        for _ in 0..self.0 {
            pair = EDGE_TURNED[pair];
        }
        pair
    }

    /// A rough picture (`variant` 0 to 15, from 32) for the turned neighbours.
    fn rough(self, mut variant: usize) -> usize {
        for _ in 0..self.0 {
            let (up, right, down, left) = ROUGH[variant];
            // What was to the west is north now, north is east, east is south, south is west.
            variant = rough_variant((left, up, right, down));
        }
        variant
    }
}

/// Which neighbours of a rough cell (up, right, down, left) are rough, by the picture
/// (`tile_types` in the core).
const ROUGH: [(bool, bool, bool, bool); 16] = [
    (false, false, true, false),
    (true, false, true, false),
    (false, true, false, false),
    (false, true, false, true),
    (true, true, true, true),
    (true, false, false, false),
    (false, false, false, true),
    (false, true, true, false),
    (false, true, true, true),
    (false, false, true, true),
    (true, true, true, false),
    (true, false, true, true),
    (true, true, false, false),
    (true, true, false, true),
    (true, false, false, true),
    (false, false, false, false),
];

fn rough_variant(neighbours: (bool, bool, bool, bool)) -> usize {
    ROUGH
        .iter()
        .position(|&n| n == neighbours)
        .expect("every combination of four neighbours has a picture")
}

/// The pair of pictures (`8 + 2 * pair`, `pair` 0 to 11: the base `7 + 2 * pair` of `tile_types`)
/// that shows grass edged by dirt where the turned view has it, for a quarter turn clockwise: the
/// dirt to the north goes east, to the east south, to the south west and to the west north, and the
/// same for the two sides together and for the corners.
///
/// Written out from the generator's table; the test checks it against the generator.
const EDGE_TURNED: [usize; 12] = [3, 5, 4, 6, 7, 8, 9, 10, 11, 0, 2, 1];

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_battle::map::layout::{
        self, DIRT, GRASS, H, MAX_CGX_WALL, MAX_CGY_WALL, MIN_CGX_WALL, MIN_CGY_WALL,
        ROUGH as ROUGH_GROUND,
    };
    use rand::{Rng, SeedableRng};

    #[test]
    fn a_quarter_turn_maps_cells_to_cells_and_four_of_them_are_none() {
        // Every turn maps the field's cells onto the field's cells, one to one.
        let field = |(x, y): (i32, i32)| (-EXTRA_X..W).contains(&x) && (0..GRID_H).contains(&y);
        for turn in 0..4u8 {
            let turn = Turn(turn);
            let mut seen = std::collections::HashSet::new();
            for y in 0..GRID_H {
                for x in -EXTRA_X..W {
                    let turned = turn.cg((x, y));
                    assert!(field(turned), "{turned:?}");
                    assert!(seen.insert(turned));
                    assert_eq!(turn.uncg(turned), (x, y));
                }
            }
        }
        let mut turned = Turn::NONE;
        for _ in 0..4 {
            turned = turned.step(false);
        }
        assert_eq!(turned, Turn::NONE);
        assert_eq!(Turn::NONE.step(true).step(false), Turn::NONE);
    }

    #[test]
    fn the_playable_band_keeps_its_middle_on_the_screen() {
        // The screen spot of a cell of the original's frame is ((x - y) * 16, (x + y) * 8): the
        // field's middle is the same spot after every turn.
        let middle = ((W - 1 - EXTRA_X) as f32 / 2.0, (GRID_H - 1) as f32 / 2.0);
        for turn in 0..4u8 {
            assert_eq!(Turn(turn).cg_f(middle), middle);
        }
    }

    #[test]
    fn the_turned_band_is_inside_the_field_the_core_has_and_the_screen_shows() {
        // A turned field takes the same room on the screen: the spots of its cells are the same set.
        let spots = |turn: Turn| -> std::collections::BTreeSet<(i32, i32)> {
            (0..GRID_H)
                .flat_map(|y| (-EXTRA_X..W).map(move |x| (x, y)))
                .map(|c| turn.cg(c))
                .collect()
        };
        for turn in 1..4u8 {
            assert_eq!(spots(Turn(turn)), spots(Turn::NONE));
        }
    }

    #[test]
    fn a_unit_looks_the_way_the_turned_view_shows_it() {
        // The facing from the turned squares, in the core's way of reading them, is the turned
        // facing of the facing before the turn.
        for turn in 0..4u8 {
            let turn = Turn(turn);
            for (a, b) in [
                ((5, 5), (4, 4)),
                ((5, 5), (5, 4)),
                ((5, 5), (6, 4)),
                ((5, 5), (6, 5)),
                ((5, 5), (6, 6)),
                ((5, 5), (5, 6)),
                ((5, 5), (4, 6)),
                ((5, 5), (4, 5)),
            ] {
                let core = |p: (i32, i32)| {
                    let (x, y) = turn.of_core((p.0 as f32, p.1 as f32));
                    // The turned cell as a core square, to read its facing the core's way.
                    (20 - x as i32, y as i32)
                };
                assert_eq!(
                    crate::battle_anim::facing(core(a), core(b)),
                    turn.facing(crate::battle_anim::facing(a, b)),
                    "turn {} from {a:?} to {b:?}",
                    turn.quarters()
                );
            }
        }
    }

    #[test]
    fn a_tree_stays_on_the_same_spot_of_a_cell_turned() {
        // Four quarter turns bring the offset back; a corner goes round the cell.
        for turn in 0..4u8 {
            let one = Turn(turn);
            let turned = one.sub((3, 12));
            assert!((0..16).contains(&turned.0) && (0..16).contains(&turned.1));
        }
        assert_eq!(Turn(1).sub((0, 0)), (15, 0));
        let mut sub = (2, 9);
        for _ in 0..4 {
            sub = Turn(1).sub(sub);
        }
        assert_eq!(sub, (2, 9));
    }

    #[test]
    fn the_walled_ring_turns_into_a_ring_of_the_same_size_in_its_turned_place() {
        for turn in 0..4u8 {
            let turn = Turn(turn);
            let (dx, dy) = turn.ring_shift();
            let mut cells: Vec<(i32, i32)> = Vec::new();
            for y in MIN_CGY_WALL..=MAX_CGY_WALL {
                for x in MIN_CGX_WALL..=MAX_CGX_WALL {
                    let at = turn.cg((x, y));
                    cells.push(at);
                    // The same place of the ring, in the original's place.
                    let home = turn.ring_cell(at);
                    assert!((MIN_CGX_WALL..=MAX_CGX_WALL).contains(&home.0));
                    assert!((MIN_CGY_WALL..=MAX_CGY_WALL).contains(&home.1));
                    assert_eq!((at.0 - dx, at.1 - dy), home);
                }
            }
            cells.sort();
            cells.dedup();
            assert_eq!(
                cells.len() as i32,
                (MAX_CGX_WALL - MIN_CGX_WALL + 1) * (MAX_CGY_WALL - MIN_CGY_WALL + 1)
            );
        }
    }

    /// A field of patches: the `kind` of every cell inside the square the turn keeps inside the
    /// field, grass outside it.
    fn patches(seed: u64) -> Vec<u8> {
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
        let mut group = vec![GRASS; (W * H) as usize];
        for y in 4..=17 {
            for x in 4..=17 {
                group[(y * W + x) as usize] =
                    [GRASS, ROUGH_GROUND, DIRT, layout::RIVER][rng.gen_range(0..4)];
            }
        }
        group
    }

    /// The same ground turned a quarter turn, in the cells of the turned frame (the square the
    /// patches are in stays inside the field when turned).
    fn turned(group: &[u8]) -> Vec<u8> {
        let mut out = vec![GRASS; (W * H) as usize];
        for y in 0..H {
            for x in 0..W {
                let (tx, ty) = Turn(1).cg((x, y));
                if (0..W).contains(&tx) && (0..H).contains(&ty) {
                    out[(ty * W + tx) as usize] = group[(y * W + x) as usize];
                }
            }
        }
        out
    }

    /// A picture with its random variant taken out, so that two draws of the same ground can be
    /// compared.
    fn sort_of(tile: usize) -> usize {
        match tile {
            0..=3 => 0,
            4..=7 => 4,
            8..=31 => 8 + (tile - 8) / 2 * 2,
            32..=47 => tile,
            t if t >= RIVER_TILE as usize => RIVER_TILE as usize + (t - RIVER_TILE as usize) % 6,
            t => t,
        }
    }

    #[test]
    fn the_pictures_of_the_ground_turn_into_the_pictures_the_generator_gives_the_turned_ground() {
        for seed in 0..40 {
            let group = patches(seed);
            let before = layout::tile_types(&group, &mut rand_pcg::Pcg64Mcg::seed_from_u64(1));
            let after =
                layout::tile_types(&turned(&group), &mut rand_pcg::Pcg64Mcg::seed_from_u64(2));
            // Leave a margin: the border cells look at cells the patches do not fill.
            for y in 5..=16 {
                for x in 5..=16 {
                    let (tx, ty) = Turn(1).cg((x, y));
                    let was = before[(y * W + x) as usize] as usize;
                    // A river cell that joins two neighbours has the picture of that pair; one
                    // that joins more, or fewer, gets the generator's first match, which is not
                    // the same after a turn.
                    let rivers = [(0, -1), (1, 0), (0, 1), (-1, 0)]
                        .iter()
                        .filter(|&&(dx, dy)| {
                            group[((y + dy) * W + x + dx) as usize] == layout::RIVER
                        })
                        .count();
                    if was >= RIVER_TILE as usize && rivers != 2 {
                        continue;
                    }
                    // Grass edged by dirt is chosen by the first of a row of tests, which a turn
                    // reorders when the dirt is on opposite sides, on three, or on several corners.
                    if (8..32).contains(&was) {
                        let dirt =
                            |dx: i32, dy: i32| group[((y + dy) * W + x + dx) as usize] == DIRT;
                        let (u, r, d, l) = (dirt(0, -1), dirt(1, 0), dirt(0, 1), dirt(-1, 0));
                        let sides = [u, r, d, l].iter().filter(|&&b| b).count();
                        let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
                            .iter()
                            .filter(|&&(dx, dy)| dirt(dx, dy))
                            .count();
                        if sides >= 3 || (u && d) || (l && r) || (sides == 0 && corners > 1) {
                            continue;
                        }
                    }
                    let now = after[(ty * W + tx) as usize] as usize;
                    assert_eq!(
                        sort_of(Turn(1).ground(was)),
                        sort_of(now),
                        "seed {seed}, cell ({x}, {y}) picture {was}"
                    );
                }
            }
        }
    }

    #[test]
    fn four_turns_of_a_picture_are_the_picture_and_each_turn_is_a_permutation() {
        for tile in 0..100 {
            let mut t = tile;
            for _ in 0..4 {
                t = Turn(1).ground(t);
            }
            assert_eq!(t, tile);
        }
        for turn in 0..4u8 {
            let mut seen = [false; 6];
            for piece in 0..6 {
                seen[Turn(turn).piece(piece)] = true;
            }
            assert!(seen.iter().all(|&s| s));
        }
        // The pairs of pictures keep their variant.
        assert_eq!(Turn(1).ground(8) % 2, 0);
        assert_eq!(Turn(1).ground(9) % 2, 1);
    }

    #[test]
    fn a_straight_river_turns_into_the_other_straight_and_a_bend_into_the_next_bend() {
        assert_eq!((Turn(1).piece(0), Turn(1).piece(1)), (1, 0));
        assert_eq!(Turn(2).piece(0), 0);
        assert_eq!(Turn(1).piece(2), 3);
        assert_eq!(Turn(2).piece(2), 4);
    }
}
