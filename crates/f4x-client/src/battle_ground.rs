//! The ground and the scenery of the battle screen, drawn from the field the core built
//! (`f4x_battle::map::layout`) with the original's combat pictures: the terrain files
//! (`cmbgrass` and the like: ground 0 to 47, trees 48 to 52, rocks 53 to 57), `cmbtcity` (roads,
//! houses, the city base, structures) and `citywall`.
//!
//! The field is in the original's frame; the core's squares are mirrored (`x = 20 - cgx`), so
//! every cell is looked up by its mirror. Positions follow `Combat_Screen_Map_Draw`,
//! `Spawn_Tree_Entities` and the other spawners of `Combat.c`: a picture is placed by a point
//! (the cell's top-left plus (14, 0), plus the offset inside the cell for trees and rocks) and
//! the shift of its anchor from the picture's top-left.

use f4x_battle::battle::Battle;
use f4x_battle::map::layout::{self, Set, Site, Structure, W};
use f4x_battle::map::{cg_of, pos_of_cg, Pos, Wall, GRID_H, GRID_W};
use f4x_core::map::Plane;
use macroquad::prelude::*;

use crate::battle_turn::Turn;
use crate::ship_art;
use crate::view::App;

/// Native size of a cell.
const CELL_W: f32 = 32.0;
const CELL_H: f32 = 16.0;

/// `cmbtcity.lbx`: the city base, the dirt tile of an outpost, houses from 2, the outpost, the
/// water, the road grid and the cloud of a flying fortress.
const CITY_BASE: usize = 0;
const DIRT_TILE: usize = 1;
const HOUSES: usize = 2;
const FORTRESS: usize = 17;
const OUTPOST: usize = 18;
/// The buildings of lairs, towers and nodes in the same file, and the sorcery node's animation
/// (eight frames); the chaos node is a volcano of `chriver` 24 to 31.
const CAVE: usize = 19;
const TOWER: usize = 20;
const DUNGEON: usize = 21;
const FORT: usize = 22;
const TEMPLE: usize = 23;
const RUINS: usize = 121;
const NATURE_NODE: usize = 65;
const SORCERY_NODE: usize = 66;
const VOLCANO: usize = 24;

/// The picture of a structure's building, if it has one (the sorcery and chaos nodes are
/// animations on the ground instead).
fn structure_entry(structure: Structure) -> Option<usize> {
    match structure {
        Structure::Outpost => Some(OUTPOST),
        Structure::Fortress => Some(FORTRESS),
        Structure::Cave => Some(CAVE),
        Structure::Tower => Some(TOWER),
        Structure::Dungeon => Some(DUNGEON),
        Structure::Fort => Some(FORT),
        Structure::Temple => Some(TEMPLE),
        Structure::Ruins => Some(RUINS),
        Structure::NatureNode => Some(NATURE_NODE),
        Structure::SorceryNode | Structure::ChaosNode => None,
    }
}
const ROAD_GRID: usize = 69;
const WATER: usize = 109;
/// Myrror's ocean is in `chriver`.
const MYRROR_WATER: usize = 12;
const MUD: usize = 118;

const FORTRESS_CLOUD: usize = 113;

/// The `rising` kind of a Wall of Stone (0 is fire, 1 darkness).
pub(crate) const STONE: u8 = 2;

/// What the draw order puts in order: scenery, walls and the figures of the units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Piece {
    Tree(usize),
    Rock(usize),
    House(usize),
    Structure,
    Wall(Pos),
    /// A stone wall section that a Wall of Stone is raising: its square and the frame (of
    /// [`crate::battle_anim::STONE_RISE_FRAMES`]); the picture shows from the ground up.
    RisingStone(Pos, usize),
    /// A section of a Wall of Fire (`0`) or Wall of Darkness (`1`): which of the fourteen.
    MagicWall(u8, usize),
    /// A section of a magic wall that is rising: its kind, which of the fourteen and the frame.
    RisingWall(u8, usize, usize),
    /// One figure of a unit: the unit, which figure, and whether it is the fading twin of a
    /// teleporter.
    Figure(usize, usize, bool),
}

/// The sections of a magic wall (`citywall` 36 for fire, 50 for darkness): one for each square of the
/// ring of the city's walls, and a second at two corners, each with one of the fourteen pictures. The
/// original's four by four ring had twelve squares and these fourteen; the wider ring repeats the
/// straight pieces along its sides (`Spawn_Fire_Wall_Entities` and the dark one for the placing).
const MAGIC_SECTIONS: usize = 22;

/// A section: the cell, the point's offset, the anchor (`None` for half the picture's width) and
/// which of the fourteen pictures it shows.
type MagicSection = ((i32, i32), (f32, f32), (Option<f32>, f32), usize);

fn magic_section(itr: usize) -> MagicSection {
    use layout::{MAX_CGX_WALL as X1, MAX_CGY_WALL as Y1, MIN_CGX_WALL as X0, MIN_CGY_WALL as Y0};
    // The squares of a side between its corners, and the two pictures that alternate along it.
    let inner = |n: usize, pair: [usize; 2]| pair[n % 2];
    let along = |n: usize| n as i32;
    let (cell, piece) = match itr {
        0 => ((X0, Y0), 0),
        1..=4 => ((X0, Y0 + along(itr)), inner(itr - 1, [1, 2])),
        5 => ((X0, Y1), 3),
        6 => ((X0, Y1), 12),
        7..=10 => ((X0 + along(itr - 6), Y0), inner(itr - 7, [4, 5])),
        11 => ((X1, Y0), 6),
        12 => ((X1, Y0), 13),
        13..=16 => ((X0 + along(itr - 12), Y1), inner(itr - 13, [7, 8])),
        17 => ((X1, Y1), 9),
        _ => ((X1, Y0 + along(itr - 17)), inner(itr - 18, [10, 11])),
    };
    let (offset, anchor) = match piece {
        0 => ((1.0, 1.0), (None, 19.0)),
        1..=3 => ((1.0, 1.0), (Some(17.0), 18.0)),
        4..=6 => ((1.0, 0.0), (Some(17.0), 17.0)),
        7..=11 => ((0.0, 14.0), (Some(16.0), 29.0)),
        _ => ((0.0, 14.0), (Some(16.0), 30.0)),
    };
    (cell, offset, anchor, piece)
}
const FIRE_WALL: usize = 36;
const DARK_WALL: usize = 50;

/// The picture file of the ground for a terrain set on a plane (the `c` files are Myrror's).
pub(crate) fn ground_file(set: Set, plane: Plane) -> &'static str {
    let (arcanus, myrror) = match set {
        Set::Desert => ("cmbdesrt", "cmbdesrc"),
        Set::Mountains => ("cmbmount", "cmbmounc"),
        Set::Tundra => ("cmbtundr", "cmbtundc"),
        _ => ("cmbgrass", "cmbgrasc"),
    };
    match plane {
        Plane::Arcanus => arcanus,
        Plane::Myrror => myrror,
    }
}

/// The grass picture file of a plane.
fn file_grass(plane: Plane) -> &'static str {
    ground_file(Set::Plains, plane)
}

fn core_pos(cgx: i32, cgy: i32) -> Pos {
    pos_of_cg((cgx, cgy))
}

/// The top-left of a cell of the original's frame, where the view draws it (`turn`).
fn origin(turn: Turn, cgx: i32, cgy: i32) -> (f32, f32) {
    let (x, y) = turn.cg((cgx, cgy));
    turned_origin(x, y)
}

/// The top-left of a cell that is already the turned one.
fn turned_origin(x: i32, y: i32) -> (f32, f32) {
    ((x - y) as f32 * 16.0, (x + y) as f32 * 8.0)
}

/// The point the original's `Combat_Grid_Screen_Coordinates` gives, offset inside the cell by
/// `sub` (a tree's or a rock's offset is turned with the cell: `Turn::sub`).
fn point(turn: Turn, cgx: i32, cgy: i32, sub: (i32, i32)) -> (f32, f32) {
    let (x, y) = origin(turn, cgx, cgy);
    point_in(x, y, sub)
}

fn point_in(x: f32, y: f32, sub: (i32, i32)) -> (f32, f32) {
    (
        x + 14.0 + ((sub.0 - sub.1) * 2) as f32,
        y + (sub.0 + sub.1) as f32,
    )
}

/// The cell of the ring of the city's wall sites (in the original's place) that the turned cell
/// `at` stands for: the picture of a wall square is the one of the same place of the ring.
fn ring_site(turn: Turn, at: (i32, i32)) -> (i32, i32) {
    turn.ring_cell(at)
}

/// The point of a wall square of the ring (a cell of the original's frame).
fn wall_point(turn: Turn, cgx: i32, cgy: i32) -> (f32, f32) {
    point(turn, cgx, cgy, (0, 0))
}

/// The point of a section of a magic wall: its cell is one of the ring of the city, in the ring's
/// turned place (the pictures of the ring are the same from every side).
fn magic_point(turn: Turn, cgx: i32, cgy: i32) -> (f32, f32) {
    let (dx, dy) = turn.ring_shift();
    let (x, y) = turned_origin(cgx + dx, cgy + dy);
    point_in(x, y, (0, 0))
}

/// Which of the twelve wall pictures stands on a wall site (in the original's frame) and how it
/// is placed: the point's offset, and the anchor's shift from the picture's top-left (`None`
/// for half the picture's width).
type WallSprite = (usize, (f32, f32), (Option<f32>, f32));

/// The wall picture of the cell `(cgx, cgy)` of the original's frame in the view `turn`: the
/// picture of the same place of the ring that the turned cell has in the turned ring.
fn turned_wall_sprite(turn: Turn, cgx: i32, cgy: i32) -> Option<WallSprite> {
    let (x, y) = ring_site(turn, turn.cg((cgx, cgy)));
    wall_sprite(x, y)
}

fn wall_sprite(cgx: i32, cgy: i32) -> Option<WallSprite> {
    use layout::{MAX_CGX_WALL as X1, MAX_CGY_WALL as Y1, MIN_CGX_WALL as X0, MIN_CGY_WALL as Y0};
    // The twelve pictures: a corner each, and along each side (between its corners) two that alternate.
    let alternate = |n: i32, pair: [usize; 2]| pair[(n - 1) as usize % 2];
    let side_west = ((1.0, 1.0), (Some(17.0), 17.0));
    let side_north = ((1.0, 0.0), (Some(17.0), 17.0));
    let side_south = ((0.0, 14.0), (Some(16.0), 30.0));
    let (piece, (offset, anchor)) = match (cgx, cgy) {
        (X0, Y0) => (0, ((1.0, 1.0), (None, 18.0))),
        (X0, Y1) => (3, side_west),
        (X1, Y0) => (6, side_north),
        (X1, Y1) => (9, side_south),
        (X0, y) if (Y0 + 1..Y1).contains(&y) => (alternate(y - Y0, [1, 2]), side_west),
        (x, Y0) if (X0 + 1..X1).contains(&x) => (alternate(x - X0, [4, 5]), side_north),
        (x, Y1) if (X0 + 1..X1).contains(&x) => (alternate(x - X0, [7, 8]), side_south),
        (X1, y) if (Y0 + 1..Y1).contains(&y) => (alternate(y - Y0, [10, 11]), side_south),
        _ => return None,
    };
    Some((piece, offset, anchor))
}

/// The value a picture sorts by (`Combat_Grid_Entity_Create`, `Set_Entity_Draw_Order`): its
/// anchor point `(x, y)` in the original's screen frame, less the grid's origin `(158, -80)`, read
/// back as a cell and the offset inside it (`Screen_To_Combat_Grid_Cell_X_And_Offset` and the Y
/// one). The original draws row by row (`cgy`), then column by column (`cgx`), then by the
/// offsets inside the cell; it does not draw along the diagonals.
pub(crate) fn draw_order(x: i32, y: i32) -> i32 {
    let across = x / 2 + y;
    let down = y - x / 2;
    (down / 16) * 8000 + (across / 16) * 320 + (down % 16) * 16 + across % 16
}

/// [`draw_order`] of an anchor given as a [`point`] (which is fourteen across the cell's corner).
pub(crate) fn order_of((px, py): (f32, f32)) -> i32 {
    draw_order(px.round() as i32 - 14, py.round() as i32)
}

impl App {
    /// The ground of every playable square, the roads over it, and what stands flat on it (the
    /// city's base, an outpost's dirt). `view` is the grid's `(origin x, origin y, scale)`.
    pub(crate) fn draw_ground(
        &mut self,
        battle: &Battle,
        plane: Plane,
        view: (f32, f32, f32),
        turn: Turn,
    ) {
        let (ox, oy, scale) = view;
        let anim = (get_time() * 6.0) as usize % 5;
        let at = |cgx: i32, cgy: i32| {
            let (x, y) = origin(turn, cgx, cgy);
            (ox + x * scale, oy + y * scale)
        };
        // A cell that is already the turned one.
        let at_turned = |x: i32, y: i32| {
            let (x, y) = turned_origin(x, y);
            (ox + x * scale, oy + y * scale)
        };
        let look = battle.map.look();
        let file = look.map_or("cmbgrass", |l| ground_file(l.set, plane));
        for cgy in 0..GRID_H {
            for cgx in -layout::EXTRA_X..W {
                let (sx, sy) = at(cgx, cgy);
                let size = Some(vec2(CELL_W * scale, CELL_H * scale));
                let tile = look.map_or(0, |l| l.tiles[layout::cell(cgx, cgy)]) as usize;
                let water = look.is_some_and(|l| l.set == Set::Water);
                // The pictures that name where the dirt, the rough ground or the river goes.
                let tile = if water { tile } else { turn.ground(tile) };
                let river = !water && tile >= layout::RIVER_TILE as usize;
                // A floating island's deck: its picture in the files (`cmbtcity` 117) is blank,
                // so the deck is shown as grass.
                let deck = look.is_some_and(|l| l.roads[layout::cell(cgx, cgy)] == layout::DECK);
                // The ocean's pictures (Myrror's are in `chriver`); a river's ground is the
                // base square `(cgx + cgy) & 3`, with the river drawn over it.
                let tex = if deck {
                    self.art
                        .picture(file_grass(plane), ((cgx + cgy) & 3) as usize, 0)
                } else if water {
                    let n = tile.saturating_sub(48);
                    match plane {
                        Plane::Arcanus => self.art.picture("cmbtcity", WATER + n, anim),
                        Plane::Myrror => self.art.picture("chriver", MYRROR_WATER + n, anim),
                    }
                } else if river {
                    self.art.picture(file, ((cgx + cgy) & 3) as usize, 0)
                } else {
                    self.art.picture(file, tile, 0)
                };
                match tex {
                    Some(tex) => draw_texture_ex(
                        tex,
                        sx,
                        sy,
                        WHITE,
                        DrawTextureParams {
                            dest_size: size,
                            ..Default::default()
                        },
                    ),
                    None => draw_rectangle(
                        sx,
                        sy,
                        CELL_W * scale,
                        CELL_H * scale,
                        Color::new(0.2, 0.4, 0.2, 1.0),
                    ),
                }
                if river {
                    // Six pictures serve both planes (`river_art.rs`); Myrror's are repainted.
                    let shape = (tile - layout::RIVER_TILE as usize) % 6;
                    let myrror = plane == Plane::Myrror;
                    if let Some(tex) = self.art.river(shape, anim, myrror).cloned() {
                        draw_texture_ex(
                            &tex,
                            sx,
                            sy,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(crate::resolution::logical_size(&tex) * scale),
                                ..Default::default()
                            },
                        );
                    }
                }
                // Mud (Earth to Mud) covers whatever the ground was.
                if battle.map.is_mud(core_pos(cgx, cgy)) {
                    let frame = (cgx + cgy) as usize + anim;
                    self.draw_native("cmbtcity", MUD, frame % 5, (sx, sy), scale);
                }
            }
        }
        // The ships, over all of the water: a hull hangs over the squares round its deck.
        self.draw_ships(battle, view, turn);
        let Some(look) = look else {
            return;
        };
        // Roads: the pieces a cell's flags call for, the worn variant when bit 0 is clear.
        let base = if look.enchanted_roads { 14 } else { 0 };
        for cgy in 0..GRID_H {
            for cgx in -layout::EXTRA_X..W {
                let flags = look.roads[layout::cell(cgx, cgy)];
                // A ship's deck is marked in the road layer and is no road.
                if flags == 0
                    || flags == layout::SHIP_DECK
                    || !battle.map.is_playable(core_pos(cgx, cgy))
                {
                    continue;
                }
                let worn = if flags & 1 != 0 { 0 } else { 7 };
                for k in 0..6 {
                    if flags & (2 << k) != 0 {
                        let (sx, sy) = at(cgx, cgy);
                        self.draw_native(
                            "cmbtcity",
                            ROAD_GRID + turn.piece(k) + base + worn,
                            anim,
                            (sx, sy),
                            scale,
                        );
                    }
                }
            }
        }
        match look.site {
            Site::Outpost => {
                let (sx, sy) = at(layout::STRUCTURE.0, layout::STRUCTURE.1);
                let frame = (get_time() * 8.0) as usize % 8;
                match look.structure {
                    Structure::Outpost => {
                        self.draw_native("cmbtcity", DIRT_TILE, 0, (sx, sy), scale);
                    }
                    Structure::SorceryNode => self.draw_native(
                        "cmbtcity",
                        SORCERY_NODE,
                        frame,
                        (sx - 46.0 * scale, sy - 15.0 * scale),
                        scale,
                    ),
                    Structure::ChaosNode => self.draw_native(
                        "chriver",
                        VOLCANO + frame,
                        0,
                        (sx - 10.0 * scale, sy - 2.0 * scale),
                        scale,
                    ),
                    _ => {}
                }
            }
            Site::City => {
                // The city's base (or the cloud under a flying fortress), anchored to the cell
                // at its far corner.
                let (shift_x, shift_y) = turn.ring_shift();
                let (sx, sy) = at_turned(
                    layout::MAX_CGX_CITY + shift_x,
                    layout::MAX_CGY_CITY + shift_y,
                );
                let entry = if battle.map.is_flying_fortress() {
                    FORTRESS_CLOUD
                } else {
                    CITY_BASE
                };
                let at = (sx - 48.0 * scale, sy - 48.0 * scale);
                // The base turns with the field: its 16 squares are swapped for the pieces that
                // have the turned roads (`battle_city_base.rs`).
                let turned = (entry == CITY_BASE && turn != Turn::NONE)
                    .then(|| self.art.turned_city_base(turn.quarters()).cloned())
                    .flatten();
                match turned {
                    Some(tex) => draw_texture_ex(
                        &tex,
                        at.0,
                        at.1,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(crate::resolution::logical_size(&tex) * scale),
                            ..Default::default()
                        },
                    ),
                    None => self.draw_native("cmbtcity", entry, anim, at, scale),
                }
            }
            Site::Open => {}
        }
    }

    /// The deck of each ship of a sea battle: its picture (`ship_art.rs`), the farther ships
    /// first, or plain planks (a darker one every other square) for a size that has none.
    fn draw_ships(&mut self, battle: &Battle, (ox, oy, scale): (f32, f32, f32), turn: Turn) {
        let squares = (0..GRID_H)
            .flat_map(|y| (0..GRID_W).map(move |x| (x, y)))
            .filter(|&p| battle.map.is_ship_deck(p))
            .collect();
        let mut ships: Vec<(i32, ship_art::Deck)> = ship_art::decks(&squares)
            .into_iter()
            .map(|deck| {
                let depth: i32 = deck
                    .squares
                    .iter()
                    .map(|&(x, y)| {
                        let (cgx, cgy) = turn.cg((W - 1 - x, y));
                        cgx + cgy
                    })
                    .sum();
                (depth / deck.squares.len().max(1) as i32, deck)
            })
            .collect();
        ships.sort_by_key(|(depth, _)| *depth);
        for (_, deck) in ships {
            let drawn = ship_art::picture(&deck, turn).is_some_and(|picture| {
                let (x, y) = turned_origin(picture.cell.0, picture.cell.1);
                let at = (
                    ox + (x + picture.offset.0) * scale,
                    oy + (y + picture.offset.1) * scale,
                );
                let Some(tex) = self.art.embedded(picture.key, picture.png) else {
                    return false;
                };
                draw_texture_ex(
                    tex,
                    at.0,
                    at.1,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(crate::resolution::logical_size(tex) * scale),
                        ..Default::default()
                    },
                );
                true
            });
            if drawn {
                continue;
            }
            for &(x, y) in &deck.squares {
                let (cgx, cgy) = (W - 1 - x, y);
                let (px, py) = origin(turn, cgx, cgy);
                let (sx, sy) = (ox + px * scale, oy + py * scale);
                let plank = if (cgx + cgy) & 1 == 0 {
                    Color::new(0.47, 0.31, 0.16, 1.0)
                } else {
                    Color::new(0.41, 0.27, 0.13, 1.0)
                };
                crate::battle_screen::diamond(sx, sy, scale, plank, true);
                let seam = Color::new(0.25, 0.15, 0.07, 1.0);
                crate::battle_screen::diamond(sx, sy, scale, seam, false);
            }
        }
    }

    /// A picture at its native size times `scale`, its top-left at `at`.
    fn draw_native(&mut self, lbx: &str, entry: usize, frame: usize, at: (f32, f32), scale: f32) {
        let tex = match self.art.picture(lbx, entry, frame) {
            Some(t) => Some(t.clone()),
            None => self.art.picture(lbx, entry, 0).cloned(),
        };
        if let Some(tex) = tex {
            draw_texture_ex(
                &tex,
                at.0,
                at.1,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(crate::resolution::logical_size(&tex) * scale),
                    ..Default::default()
                },
            );
        }
    }

    /// Like [`Self::draw_native`], but only the lower `shown` (0 to 1) of the picture, which
    /// grows up out of the ground.
    fn draw_native_rising(
        &mut self,
        lbx: &str,
        entry: usize,
        frame: usize,
        at: (f32, f32),
        scale: f32,
        shown: f32,
    ) {
        let Some(tex) = self.art.picture(lbx, entry, frame).cloned() else {
            return;
        };
        let size = crate::resolution::logical_size(&tex) * scale;
        let shown = shown.clamp(0.0, 1.0);
        let (w, h) = (tex.width(), tex.height());
        draw_texture_ex(
            &tex,
            at.0,
            at.1 + size.y * (1.0 - shown),
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(size.x, size.y * shown)),
                source: Some(Rect::new(0.0, h * (1.0 - shown), w, h * shown)),
                ..Default::default()
            },
        );
    }

    /// The scenery and walls of the field with the value each sorts by ([`draw_order`], from the
    /// anchor the original gives the picture), to be put in order with the figures of the units:
    /// `(order, piece)`.
    pub(crate) fn scenery_pieces(
        &self,
        battle: &Battle,
        rising: Option<(u8, usize)>,
        turn: Turn,
    ) -> Vec<(i32, Piece)> {
        let mut out = Vec::new();
        if let Some(look) = battle.map.look() {
            for (i, t) in look.trees.iter().enumerate() {
                let sub = turn.sub((i32::from(t.sub.0), i32::from(t.sub.1)));
                let (px, py) = point(turn, t.cell.0, t.cell.1, sub);
                out.push((order_of((px - 5.0, py)), Piece::Tree(i)));
            }
            for (i, r) in look.rocks.iter().enumerate() {
                let sub = turn.sub((i32::from(r.sub.0), i32::from(r.sub.1)));
                let (px, py) = point(turn, r.cell.0, r.cell.1, sub);
                out.push((order_of((px - 5.0, py)), Piece::Rock(i)));
            }
            for (i, h) in look.houses.iter().enumerate() {
                let (px, py) = point(turn, h.cell.0, h.cell.1, (0, 0));
                out.push((order_of((px, py + 2.0)), Piece::House(i)));
            }
            let has_building = matches!(look.site, Site::Outpost)
                || (look.site == Site::City && look.structure == Structure::Fortress);
            if has_building && structure_entry(look.structure).is_some() {
                let (x, y) = layout::STRUCTURE;
                out.push((order_of(point(turn, x, y, (0, 0))), Piece::Structure));
            }
        }
        for cgy in 0..GRID_H {
            for cgx in -layout::EXTRA_X..W {
                let pos = core_pos(cgx, cgy);
                if battle.map.wall(pos) == Wall::None {
                    continue;
                }
                if let Some((_, (dx, dy), _)) = turned_wall_sprite(turn, cgx, cgy) {
                    let piece = match rising.filter(|&(k, _)| k == STONE) {
                        Some((_, frame)) => Piece::RisingStone(pos, frame),
                        None => Piece::Wall(pos),
                    };
                    let (px, py) = wall_point(turn, cgx, cgy);
                    out.push((order_of((px + dx, py + dy)), piece));
                }
            }
        }
        // The magic walls, and the one being raised by the spell that is playing: its kind (0
        // fire, 1 darkness) and frame.
        for kind in [1, 0] {
            let on = if kind == 0 {
                battle.wall_of_fire
            } else {
                battle.wall_of_darkness
            };
            let rise = rising.filter(|&(k, _)| k == kind).map(|(_, frame)| frame);
            if !on && rise.is_none() {
                continue;
            }
            for itr in 0..MAGIC_SECTIONS {
                let ((cgx, cgy), (dx, dy), _, _) = magic_section(itr);
                let piece = match rise {
                    Some(frame) => Piece::RisingWall(kind, itr, frame),
                    None => Piece::MagicWall(kind, itr),
                };
                let (px, py) = magic_point(turn, cgx, cgy);
                out.push((order_of((px + dx, py + dy)), piece));
            }
        }
        out
    }

    /// Whether the wall's picture is not there (the art is not extracted), so the caller draws
    /// a plain block instead.
    pub(crate) fn wall_picture_missing(
        &mut self,
        battle: &Battle,
        plane: Plane,
        pos: Pos,
        turn: Turn,
    ) -> bool {
        let (cgx, cgy) = cg_of(pos);
        let Some((itr, _, _)) = turned_wall_sprite(turn, cgx, cgy) else {
            return true;
        };
        let entry = usize::from(plane == Plane::Myrror) * 12 + itr;
        let frame = usize::from(battle.map.wall(pos) == Wall::Broken);
        self.art.picture("citywall", entry, frame).is_none()
    }

    /// Draw one piece of scenery or wall (units are drawn by the caller).
    pub(crate) fn draw_piece(
        &mut self,
        battle: &Battle,
        plane: Plane,
        piece: Piece,
        (ox, oy, scale): (f32, f32, f32),
        turn: Turn,
    ) {
        let Some(look) = battle.map.look() else {
            return;
        };
        let file = ground_file(look.set, plane);
        // (file, entry, frame, top-left in native pixels)
        let (lbx, entry, frame, (x, y)): (&str, usize, usize, (f32, f32)) = match piece {
            Piece::Tree(i) => {
                let t = look.trees[i];
                let sub = turn.sub((i32::from(t.sub.0), i32::from(t.sub.1)));
                let (px, py) = point(turn, t.cell.0, t.cell.1, sub);
                (
                    file,
                    48 + usize::from(t.variant),
                    0,
                    (px - 5.0 - 8.0, py - 13.0),
                )
            }
            Piece::Rock(i) => {
                let r = look.rocks[i];
                let sub = turn.sub((i32::from(r.sub.0), i32::from(r.sub.1)));
                let (px, py) = point(turn, r.cell.0, r.cell.1, sub);
                (
                    file,
                    53 + usize::from(r.variant),
                    0,
                    (px - 5.0 - 6.0, py - 12.0),
                )
            }
            Piece::House(i) => {
                let h = look.houses[i];
                let (px, py) = point(turn, h.cell.0, h.cell.1, (0, 0));
                let entry = HOUSES + usize::from(h.variant);
                let Some(size) = self
                    .art
                    .picture("cmbtcity", entry, 0)
                    .map(crate::resolution::logical_size)
                else {
                    return;
                };
                (
                    "cmbtcity",
                    entry,
                    0,
                    (px - size.x / 2.0, py + 2.0 - (size.y - 14.0)),
                )
            }
            Piece::Structure => {
                let (px, py) = point(turn, layout::STRUCTURE.0, layout::STRUCTURE.1, (0, 0));
                let structure = battle
                    .map
                    .look()
                    .map_or(Structure::Outpost, |l| l.structure);
                let Some(entry) = structure_entry(structure) else {
                    return;
                };
                let Some(size) = self
                    .art
                    .picture("cmbtcity", entry, 0)
                    .map(crate::resolution::logical_size)
                else {
                    return;
                };
                // The nature node's picture hangs from a different spot.
                let (hang_x, hang_y) = if structure == Structure::NatureNode {
                    (16.0, 15.0)
                } else {
                    (15.0, size.y - 21.0)
                };
                ("cmbtcity", entry, 0, (px - hang_x, py - hang_y))
            }
            Piece::Wall(pos) | Piece::RisingStone(pos, _) => {
                let (cgx, cgy) = cg_of(pos);
                let Some((itr, (dx, dy), (shift_x, shift_y))) = turned_wall_sprite(turn, cgx, cgy)
                else {
                    return;
                };
                let bank = usize::from(plane == Plane::Myrror);
                let frame = usize::from(battle.map.wall(pos) == Wall::Broken);
                let entry = bank * 12 + itr;
                let Some(size) = self
                    .art
                    .picture("citywall", entry, frame)
                    .map(crate::resolution::logical_size)
                else {
                    return;
                };
                let (px, py) = wall_point(turn, cgx, cgy);
                (
                    "citywall",
                    entry,
                    frame,
                    (px + dx - shift_x.unwrap_or(size.x / 2.0), py + dy - shift_y),
                )
            }
            Piece::MagicWall(kind, itr) => {
                let ((cgx, cgy), (dx, dy), (hang_x, hang_y), piece) = magic_section(itr);
                let entry = if kind == 0 { FIRE_WALL } else { DARK_WALL } + piece;
                let frame = (get_time() * 6.0) as usize % 4;
                let Some(size) = self
                    .art
                    .picture("citywall", entry, 0)
                    .map(crate::resolution::logical_size)
                else {
                    return;
                };
                let (px, py) = magic_point(turn, cgx, cgy);
                (
                    "citywall",
                    entry,
                    frame,
                    (px + dx - hang_x.unwrap_or(size.x / 2.0), py + dy - hang_y),
                )
            }
            Piece::RisingWall(kind, itr, frame) => {
                let ((cgx, cgy), (dx, dy), (hang_x, hang_y), piece) = magic_section(itr);
                let entry = if kind == 0 { FIRE_WALL } else { DARK_WALL } + piece;
                let frames = self.art.frames("wallrise", entry).max(1);
                let Some(size) = self
                    .art
                    .picture("wallrise", entry, 0)
                    .map(crate::resolution::logical_size)
                else {
                    return;
                };
                let (px, py) = magic_point(turn, cgx, cgy);
                (
                    "wallrise",
                    entry,
                    frame.min(frames - 1),
                    (px + dx - hang_x.unwrap_or(size.x / 2.0), py + dy - hang_y),
                )
            }
            Piece::Figure(..) => return,
        };
        if let Piece::RisingStone(_, step) = piece {
            let shown = (step + 1) as f32 / crate::battle_anim::STONE_RISE_FRAMES as f32;
            self.draw_native_rising(
                lbx,
                entry,
                frame,
                (ox + x * scale, oy + y * scale),
                scale,
                shown,
            );
            return;
        }
        self.draw_native(lbx, entry, frame, (ox + x * scale, oy + y * scale), scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_files_follow_the_set_and_the_plane() {
        assert_eq!(ground_file(Set::Desert, Plane::Arcanus), "cmbdesrt");
        assert_eq!(ground_file(Set::Desert, Plane::Myrror), "cmbdesrc");
        assert_eq!(ground_file(Set::Forest, Plane::Arcanus), "cmbgrass");
        assert_eq!(ground_file(Set::Mountains, Plane::Myrror), "cmbmounc");
        assert_eq!(ground_file(Set::Tundra, Plane::Arcanus), "cmbtundr");
    }

    /// The cells of the ring of the walls (the area with its walls, in the original's frame).
    fn wall_area() -> Vec<(i32, i32)> {
        (layout::MIN_CGY_WALL..=layout::MAX_CGY_WALL)
            .flat_map(|y| (layout::MIN_CGX_WALL..=layout::MAX_CGX_WALL).map(move |x| (x, y)))
            .collect()
    }

    #[test]
    fn the_wall_sites_show_all_twelve_pictures_and_the_inside_has_none() {
        let mut seen: Vec<usize> = wall_area()
            .into_iter()
            .filter_map(|(x, y)| wall_sprite(x, y).map(|s| s.0))
            .collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen, (0..12).collect::<Vec<_>>());
        let middle = (layout::MIN_CGX_WALL + 1, layout::MIN_CGY_WALL + 1);
        assert!(wall_sprite(middle.0, middle.1).is_none());
    }

    #[test]
    fn the_wall_sites_match_the_cores_wall_sites() {
        use f4x_battle::map::is_wall_site;
        for (x, y) in wall_area() {
            assert_eq!(
                wall_sprite(x, y).is_some(),
                is_wall_site(core_pos(x, y)),
                "({x},{y})"
            );
        }
    }

    #[test]
    fn in_every_turn_every_wall_site_shows_a_picture_of_the_ring_in_its_own_place() {
        let sites: Vec<_> = wall_area()
            .into_iter()
            .filter(|&(x, y)| wall_sprite(x, y).is_some())
            .collect();
        for quarters in 0..4 {
            let turn = (0..quarters).fold(Turn::NONE, |t, _| t.step(false));
            let mut places = Vec::new();
            for &(cgx, cgy) in &sites {
                assert!(
                    turned_wall_sprite(turn, cgx, cgy).is_some(),
                    "every wall site has a picture in every turn"
                );
                places.push(turn.cg((cgx, cgy)));
            }
            places.sort();
            places.dedup();
            assert_eq!(places.len(), sites.len(), "turn {quarters}");
        }
    }

    #[test]
    fn a_magic_wall_has_a_section_on_every_wall_site_and_uses_the_fourteen_pictures() {
        let sites: Vec<_> = wall_area()
            .into_iter()
            .filter(|&(x, y)| wall_sprite(x, y).is_some())
            .collect();
        let mut cells: Vec<_> = (0..MAGIC_SECTIONS).map(|i| magic_section(i).0).collect();
        cells.sort();
        cells.dedup();
        assert_eq!(cells, {
            let mut s = sites;
            s.sort();
            s
        });
        let mut pieces: Vec<_> = (0..MAGIC_SECTIONS).map(|i| magic_section(i).3).collect();
        pieces.sort();
        pieces.dedup();
        assert_eq!(pieces, (0..14).collect::<Vec<_>>());
    }

    #[test]
    fn the_draw_order_runs_row_by_row_then_column_by_column() {
        // `cgy * 8000 + cgx * 320` for the corner of a cell (`Combat_Grid_Entity_Create`).
        for (cgx, cgy) in [(0, 0), (6, 5), (13, 2), (3, 17)] {
            assert_eq!(
                order_of(point(Turn::NONE, cgx, cgy, (0, 0))),
                cgy * 8000 + cgx * 320
            );
        }
        // A cell of a later row is drawn over every cell of an earlier one, even one lower on the
        // screen.
        assert!(
            order_of(point(Turn::NONE, 0, 6, (0, 0))) > order_of(point(Turn::NONE, 15, 5, (0, 0)))
        );
        // Inside a cell, the offsets order the pictures: a figure further along `x` or `y`
        // is drawn later.
        assert!(
            order_of(point(Turn::NONE, 6, 5, (4, 0))) > order_of(point(Turn::NONE, 6, 5, (0, 0)))
        );
        assert!(
            order_of(point(Turn::NONE, 6, 5, (0, 4))) > order_of(point(Turn::NONE, 6, 5, (0, 0)))
        );
    }

    #[test]
    fn a_point_is_fourteen_across_the_cells_corner() {
        assert_eq!(point(Turn::NONE, 5, 5, (0, 0)), (14.0, 80.0));
        assert_eq!(point(Turn::NONE, 6, 5, (0, 0)), (30.0, 88.0));
        assert_eq!(point(Turn::NONE, 5, 5, (3, 1)), (18.0, 84.0));
    }
}
