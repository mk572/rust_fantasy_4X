//! The ground and the scenery of the battle screen, drawn from the field the core built
//! (`f4x_core::battle::map::layout`) with the original's combat pictures: the terrain files
//! (`cmbgrass` and the like: ground 0 to 47, trees 48 to 52, rocks 53 to 57), `cmbtcity` (roads,
//! houses, the city base, structures) and `citywall`.
//!
//! The field is in the original's frame; the core's squares are mirrored (`x = 20 - cgx`), so
//! every cell is looked up by its mirror. Positions follow `Combat_Screen_Map_Draw`,
//! `Spawn_Tree_Entities` and the other spawners of `Combat.c`: a picture is placed by a point
//! (the cell's top-left plus (14, 0), plus the offset inside the cell for trees and rocks) and
//! the shift of its anchor from the picture's top-left.

use f4x_core::battle::map::layout::{self, Set, Site, Structure, H, W};
use f4x_core::battle::map::{Pos, Wall, GRID_W};
use f4x_core::battle::Battle;
use f4x_core::map::Plane;
use macroquad::prelude::*;

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
/// The river pictures (twelve in the original, six of them in the files), in `cmbtcity`.
const RIVER: usize = 97;
const MUD: usize = 118;

const FORTRESS_CLOUD: usize = 113;

/// What the depth sort puts in order: scenery, walls and units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Piece {
    Tree(usize),
    Rock(usize),
    House(usize),
    Structure,
    Wall(Pos),
    /// A section of a Wall of Fire (`0`) or Wall of Darkness (`1`): which of the fourteen.
    MagicWall(u8, usize),
    Unit(usize),
}

/// The fourteen pictures of a magic wall (`citywall` 36 for fire, 50 for darkness) and where each
/// stands: the cell, the point's offset, the anchor (`None` for half the picture's width), and
/// whether it is drawn in front of the units (`Spawn_Fire_Wall_Entities` and the dark one).
type MagicSection = ((i32, i32), (f32, f32), (Option<f32>, f32), bool);

fn magic_section(itr: usize) -> MagicSection {
    use layout::{MAX_CGX_CITY as X1, MAX_CGY_CITY as Y1, MIN_CGX_CITY as X0, MIN_CGY_CITY as Y0};
    let i = itr as i32;
    match itr {
        0 => ((X0, Y0), (1.0, 1.0), (None, 19.0), false),
        1..=3 => ((X0, Y0 + i), (1.0, 1.0), (Some(17.0), 18.0), false),
        4..=6 => (((2 + i), Y0), (1.0, 0.0), (Some(17.0), 17.0), false),
        7..=9 => (((i - 1), Y1), (0.0, 14.0), (Some(16.0), 29.0), true),
        10 | 11 => ((X1, i + 1), (0.0, 14.0), (Some(16.0), 29.0), true),
        12 => ((X0, Y1), (0.0, 14.0), (Some(16.0), 30.0), true),
        _ => ((X1, Y0), (0.0, 14.0), (Some(16.0), 30.0), true),
    }
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

/// The core's square for a cell of the original's frame.
fn core_pos(cgx: i32, cgy: i32) -> Pos {
    (GRID_W - 1 - cgx, cgy)
}

/// Top-left of a cell of the original's frame, relative to the grid's origin.
fn origin(cgx: i32, cgy: i32) -> (f32, f32) {
    ((cgx - cgy) as f32 * 16.0, (cgx + cgy) as f32 * 8.0)
}

/// The point the original's `Combat_Grid_Screen_Coordinates` gives, offset inside the cell by
/// `sub`.
fn point(cgx: i32, cgy: i32, sub: (i32, i32)) -> (f32, f32) {
    let (x, y) = origin(cgx, cgy);
    (
        x + 14.0 + ((sub.0 - sub.1) * 2) as f32,
        y + (sub.0 + sub.1) as f32,
    )
}

/// Which of the twelve wall pictures stands on a wall site (in the original's frame) and how it
/// is placed: the point's offset, and the anchor's shift from the picture's top-left (`None`
/// for half the picture's width).
type WallSprite = (usize, (f32, f32), (Option<f32>, f32));

fn wall_sprite(cgx: i32, cgy: i32) -> Option<WallSprite> {
    use layout::{MAX_CGX_CITY as X1, MAX_CGY_CITY as Y1, MIN_CGX_CITY as X0, MIN_CGY_CITY as Y0};
    Some(match (cgx, cgy) {
        (X0, Y0) => (0, (1.0, 1.0), (None, 18.0)),
        (X0, y) if (Y0 + 1..=Y1).contains(&y) => {
            ((y - Y0) as usize, (1.0, 1.0), (Some(17.0), 17.0))
        }
        (x, Y0) if (X0 + 1..=X1).contains(&x) => ((x - 2) as usize, (1.0, 0.0), (Some(17.0), 17.0)),
        (x, Y1) if (X0 + 1..=X1).contains(&x) => {
            ((x + 1) as usize, (0.0, 14.0), (Some(16.0), 30.0))
        }
        (X1, y) if (Y0 + 1..Y1).contains(&y) => ((y - 1) as usize, (0.0, 14.0), (Some(16.0), 30.0)),
        _ => return None,
    })
}

/// The depth a piece sorts by: far cells first, the nearer ones over them. Scaled by a hundred so
/// that the offsets inside a cell can be told apart.
pub(crate) fn depth_of(cgx: i32, cgy: i32, extra: i32) -> i32 {
    (cgx + cgy) * 100 + extra
}

impl App {
    /// The ground of every playable square, the roads over it, and what stands flat on it (the
    /// city's base, an outpost's dirt). `view` is the grid's `(origin x, origin y, scale)`.
    pub(crate) fn draw_ground(&mut self, battle: &Battle, plane: Plane, view: (f32, f32, f32)) {
        let (ox, oy, scale) = view;
        let anim = (get_time() * 6.0) as usize % 5;
        let at = |cgx: i32, cgy: i32| {
            let (x, y) = origin(cgx, cgy);
            (ox + x * scale, oy + y * scale)
        };
        let look = battle.map.look();
        let file = look.map_or("cmbgrass", |l| ground_file(l.set, plane));
        for cgy in 0..H {
            for cgx in 0..W {
                if !battle.map.is_playable(core_pos(cgx, cgy)) {
                    continue;
                }
                let (sx, sy) = at(cgx, cgy);
                let size = Some(vec2(CELL_W * scale, CELL_H * scale));
                let tile = look.map_or(0, |l| l.tiles[(cgy * W + cgx) as usize]) as usize;
                let water = look.is_some_and(|l| l.set == Set::Water);
                let river = !water && tile >= layout::RIVER_TILE as usize;
                // A floating island's deck: its picture in the files (`cmbtcity` 117) is blank,
                // so the deck is shown as grass.
                let deck = look.is_some_and(|l| l.roads[(cgy * W + cgx) as usize] == layout::DECK);
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
                    // The shipped files hold only the second variant of each shape (`cmbtcity`
                    // 103 to 108; entries 97 to 102 and all of Myrror's `chriver` 0 to 11 are
                    // empty, which is why the original never shows a river). Myrror uses
                    // the same pictures with the water repainted.
                    let shape = (tile - layout::RIVER_TILE as usize) % 6;
                    let entry = RIVER + 6 + shape;
                    match plane {
                        Plane::Arcanus => {
                            self.draw_native("cmbtcity", entry, anim, (sx, sy), scale)
                        }
                        Plane::Myrror => {
                            if let Some(tex) = self.art.myrror_river(entry, anim).cloned() {
                                draw_texture_ex(
                                    &tex,
                                    sx,
                                    sy,
                                    WHITE,
                                    DrawTextureParams {
                                        dest_size: Some(tex.size() * scale),
                                        ..Default::default()
                                    },
                                );
                            }
                        }
                    }
                }
                // Mud (Earth to Mud) covers whatever the ground was.
                if battle.map.is_mud(core_pos(cgx, cgy)) {
                    let frame = (cgx + cgy) as usize + anim;
                    self.draw_native("cmbtcity", MUD, frame % 5, (sx, sy), scale);
                }
            }
        }
        let Some(look) = look else {
            return;
        };
        // Roads: the pieces a cell's flags call for, the worn variant when bit 0 is clear.
        let base = if look.enchanted_roads { 14 } else { 0 };
        for cgy in 0..H {
            for cgx in 0..W {
                let flags = look.roads[(cgy * W + cgx) as usize];
                if flags == 0 || !battle.map.is_playable(core_pos(cgx, cgy)) {
                    continue;
                }
                let worn = if flags & 1 != 0 { 0 } else { 7 };
                for k in 0..6 {
                    if flags & (2 << k) != 0 {
                        let (sx, sy) = at(cgx, cgy);
                        self.draw_native(
                            "cmbtcity",
                            ROAD_GRID + k + base + worn,
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
                let (sx, sy) = at(layout::MAX_CGX_CITY, layout::MAX_CGY_CITY);
                let entry = if battle.map.is_flying_fortress() {
                    FORTRESS_CLOUD
                } else {
                    CITY_BASE
                };
                self.draw_native(
                    "cmbtcity",
                    entry,
                    anim,
                    (sx - 48.0 * scale, sy - 48.0 * scale),
                    scale,
                );
            }
            Site::Open => {}
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
                    dest_size: Some(tex.size() * scale),
                    ..Default::default()
                },
            );
        }
    }

    /// The scenery and walls of the field with the depth each sorts by, to be put in order with
    /// the units: `(depth, rank, square, piece)`, the rank putting scenery before walls before
    /// units on one depth.
    pub(crate) fn scenery_pieces(&self, battle: &Battle) -> Vec<(i32, u8, Pos, Piece)> {
        let mut out = Vec::new();
        if let Some(look) = battle.map.look() {
            for (i, t) in look.trees.iter().enumerate() {
                let d = depth_of(t.cell.0, t.cell.1, (t.sub.0 + t.sub.1) as i32 * 4);
                out.push((d, 0, core_pos(t.cell.0, t.cell.1), Piece::Tree(i)));
            }
            for (i, r) in look.rocks.iter().enumerate() {
                let d = depth_of(r.cell.0, r.cell.1, (r.sub.0 + r.sub.1) as i32 * 4);
                out.push((d, 0, core_pos(r.cell.0, r.cell.1), Piece::Rock(i)));
            }
            for (i, h) in look.houses.iter().enumerate() {
                out.push((
                    depth_of(h.cell.0, h.cell.1, 20),
                    0,
                    core_pos(h.cell.0, h.cell.1),
                    Piece::House(i),
                ));
            }
            let has_building = matches!(look.site, Site::Outpost)
                || (look.site == Site::City && look.structure == Structure::Fortress);
            if has_building && structure_entry(look.structure).is_some() {
                let (x, y) = layout::STRUCTURE;
                out.push((depth_of(x, y, 0), 0, core_pos(x, y), Piece::Structure));
            }
        }
        // Walls, back to front: the stone wall's far side, the darkness, the fire, then (over the
        // units standing there) the fire, the darkness and the stone wall's near side. Equal
        // depths keep the order they were pushed in.
        let stone = |near: bool, out: &mut Vec<(i32, u8, Pos, Piece)>| {
            for cgy in 0..H {
                for cgx in 0..W {
                    let pos = core_pos(cgx, cgy);
                    if battle.map.wall(pos) == Wall::None {
                        continue;
                    }
                    if let Some((_, (_, dy), _)) = wall_sprite(cgx, cgy) {
                        if (dy > 1.0) == near {
                            let extra = if near { 50 } else { 10 };
                            out.push((depth_of(cgx, cgy, extra), 1, pos, Piece::Wall(pos)));
                        }
                    }
                }
            }
        };
        let magic = |kind: u8, near: bool, out: &mut Vec<(i32, u8, Pos, Piece)>| {
            let on = if kind == 0 {
                battle.wall_of_fire
            } else {
                battle.wall_of_darkness
            };
            if !on {
                return;
            }
            for itr in 0..14 {
                let ((cgx, cgy), _, _, front) = magic_section(itr);
                if front == near {
                    let extra = if near { 50 } else { 10 };
                    let pos = core_pos(cgx, cgy);
                    out.push((
                        depth_of(cgx, cgy, extra),
                        1,
                        pos,
                        Piece::MagicWall(kind, itr),
                    ));
                }
            }
        };
        stone(false, &mut out);
        magic(1, false, &mut out);
        magic(0, false, &mut out);
        magic(0, true, &mut out);
        magic(1, true, &mut out);
        stone(true, &mut out);
        out
    }

    /// Whether the wall's picture is not there (the art is not extracted), so the caller draws
    /// a plain block instead.
    pub(crate) fn wall_picture_missing(&mut self, battle: &Battle, plane: Plane, pos: Pos) -> bool {
        let (cgx, cgy) = (GRID_W - 1 - pos.0, pos.1);
        let Some((itr, _, _)) = wall_sprite(cgx, cgy) else {
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
    ) {
        let Some(look) = battle.map.look() else {
            return;
        };
        let file = ground_file(look.set, plane);
        // (file, entry, frame, top-left in native pixels)
        let (lbx, entry, frame, (x, y)): (&str, usize, usize, (f32, f32)) = match piece {
            Piece::Tree(i) => {
                let t = look.trees[i];
                let (px, py) = point(t.cell.0, t.cell.1, (i32::from(t.sub.0), i32::from(t.sub.1)));
                (
                    file,
                    48 + usize::from(t.variant),
                    0,
                    (px - 5.0 - 8.0, py - 13.0),
                )
            }
            Piece::Rock(i) => {
                let r = look.rocks[i];
                let (px, py) = point(r.cell.0, r.cell.1, (i32::from(r.sub.0), i32::from(r.sub.1)));
                (
                    file,
                    53 + usize::from(r.variant),
                    0,
                    (px - 5.0 - 6.0, py - 12.0),
                )
            }
            Piece::House(i) => {
                let h = look.houses[i];
                let (px, py) = point(h.cell.0, h.cell.1, (0, 0));
                let entry = HOUSES + usize::from(h.variant);
                let Some(size) = self.art.picture("cmbtcity", entry, 0).map(|t| t.size()) else {
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
                let (px, py) = point(layout::STRUCTURE.0, layout::STRUCTURE.1, (0, 0));
                let structure = battle
                    .map
                    .look()
                    .map_or(Structure::Outpost, |l| l.structure);
                let Some(entry) = structure_entry(structure) else {
                    return;
                };
                let Some(size) = self.art.picture("cmbtcity", entry, 0).map(|t| t.size()) else {
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
            Piece::Wall(pos) => {
                let (cgx, cgy) = (GRID_W - 1 - pos.0, pos.1);
                let Some((itr, (dx, dy), (shift_x, shift_y))) = wall_sprite(cgx, cgy) else {
                    return;
                };
                let bank = usize::from(plane == Plane::Myrror);
                let frame = usize::from(battle.map.wall(pos) == Wall::Broken);
                let entry = bank * 12 + itr;
                let Some(size) = self.art.picture("citywall", entry, frame).map(|t| t.size())
                else {
                    return;
                };
                let (px, py) = point(cgx, cgy, (0, 0));
                (
                    "citywall",
                    entry,
                    frame,
                    (px + dx - shift_x.unwrap_or(size.x / 2.0), py + dy - shift_y),
                )
            }
            Piece::MagicWall(kind, itr) => {
                let ((cgx, cgy), (dx, dy), (hang_x, hang_y), _) = magic_section(itr);
                let entry = if kind == 0 { FIRE_WALL } else { DARK_WALL } + itr;
                let frame = (get_time() * 6.0) as usize % 4;
                let Some(size) = self.art.picture("citywall", entry, 0).map(|t| t.size()) else {
                    return;
                };
                let (px, py) = point(cgx, cgy, (0, 0));
                (
                    "citywall",
                    entry,
                    frame,
                    (px + dx - hang_x.unwrap_or(size.x / 2.0), py + dy - hang_y),
                )
            }
            Piece::Unit(_) => return,
        };
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

    #[test]
    fn the_twelve_wall_sites_have_twelve_different_pictures() {
        let mut seen = Vec::new();
        for y in layout::MIN_CGY_CITY..=layout::MAX_CGY_CITY {
            for x in layout::MIN_CGX_CITY..=layout::MAX_CGX_CITY {
                if let Some((itr, _, _)) = wall_sprite(x, y) {
                    seen.push(itr);
                }
            }
        }
        seen.sort();
        assert_eq!(seen, (0..12).collect::<Vec<_>>());
        // The middle four squares have none.
        assert!(wall_sprite(6, 11).is_none() && wall_sprite(7, 12).is_none());
    }

    #[test]
    fn the_wall_sites_match_the_cores_wall_sites() {
        use f4x_core::battle::map::is_wall_site;
        for y in layout::MIN_CGY_CITY..=layout::MAX_CGY_CITY {
            for x in layout::MIN_CGX_CITY..=layout::MAX_CGX_CITY {
                assert_eq!(
                    wall_sprite(x, y).is_some(),
                    is_wall_site(core_pos(x, y)),
                    "({x},{y})"
                );
            }
        }
    }

    #[test]
    fn a_point_is_fourteen_across_the_cells_corner() {
        assert_eq!(point(5, 5, (0, 0)), (14.0, 80.0));
        assert_eq!(point(6, 5, (0, 0)), (30.0, 88.0));
        assert_eq!(point(5, 5, (3, 1)), (18.0, 84.0));
    }
}
