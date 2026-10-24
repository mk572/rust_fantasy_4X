//! Drawing one overland map square the way the original does: ground, river, corruption,
//! the thing standing on it (mineral, site, tower, node), roads, and the owner's node aura.

use f4x_core::feature::def::FeatureKind;
use f4x_core::game::Game;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Grid, Plane};
use f4x_core::player::PlayerId;
use macroquad::prelude::*;

use crate::art::{terrain_type as tt, Art, TILE_W};
use crate::terrain_pick as pick;

/// Which banner a player's things are drawn in.
pub fn banner_of(game: &Game, owner: PlayerId) -> u8 {
    game.wizard(owner)
        .map_or(f4x_core::player::NEUTRAL_BANNER, |w| w.banner())
}

fn draw_pic(tex: &Texture2D, x: f32, y: f32, scale: f32) {
    draw_texture_ex(
        tex,
        x,
        y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(tex.size() * scale),
            ..Default::default()
        },
    );
}

/// The soft edge of explored land (`mapback.lbx` 0 to 13) laid over a square with some of its
/// corners explored; `corners` is 1 to 14.
pub(crate) fn draw_unexplored_edge(art: &mut Art, corners: u8, sx: f32, sy: f32, k: f32) {
    if let Some(tex) = art.picture("mapback", usize::from(corners) - 1, 0) {
        draw_pic(tex, sx, sy, k);
    }
}

/// A filled five-pointed star with its top point up.
pub(crate) fn draw_star(centre: Vec2, outer: f32, colour: Color) {
    let inner = outer * 0.4;
    let point = |i: usize| {
        let radius = if i.is_multiple_of(2) { outer } else { inner };
        let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
        centre + vec2(angle.cos(), angle.sin()) * radius
    };
    for i in 0..10 {
        draw_triangle(centre, point(i), point((i + 1) % 10), colour);
    }
}

/// Whether a neighbouring square counts as water for a river that runs into it.
fn is_water(game: &Game, grid: &Grid, x: i32, y: i32) -> bool {
    pick::class_at(grid, game.content(), x, y) == Some(TerrainClass::Ocean)
}

/// The eight road directions in the order of `mapback.lbx` 46 to 53: clockwise from north.
const ROAD_DIRS: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// Draw the square at `(wx, y)` with its top-left at `(sx, sy)` and tile size `t`. `anim` is
/// the animation step shared by water, roads and node auras; the owners' auras are always drawn
/// round the magic nodes.
#[allow(clippy::too_many_arguments)]
pub fn draw_square(
    game: &Game,
    art: &mut Art,
    plane: Plane,
    wx: u32,
    y: u32,
    sx: f32,
    sy: f32,
    t: Vec2,
    anim: usize,
) {
    let Some(map) = game.map() else {
        return;
    };
    let grid = map.plane(plane);
    let content = &game.content();
    let k = t.x / TILE_W;
    let (x, yi) = (wx as i32, y as i32);
    let plane_offset = if plane == Plane::Myrror {
        tt::MYRROR_OFFSET
    } else {
        0
    };
    let here = f4x_core::map::Position { plane, x: wx, y };
    let feature = game.economy().feature_at(here);
    // The ground: a node's own piece, or the type the neighbours call for.
    let ground = if game.economy().volcano_at(here).is_some() {
        Some(pick::VOLCANO)
    } else {
        pick::ground_type(grid, content, x, yi, plane.index(), &|table, mask| {
            art.autotile_type(table, mask)
        })
    };
    let Some(ground) = ground else {
        return;
    };
    match art.terrain_frame(ground + plane_offset, anim) {
        Some(tex) => draw_pic(tex, sx, sy, k),
        None => draw_rectangle(sx, sy, t.x, t.y, Color::new(0.2, 0.4, 0.2, 1.0)),
    }
    // A river over any ground: it leaves by the sides that have river or sea.
    if grid.has_river(x, yi) {
        let side = |dx: i32, dy: i32| {
            grid.has_river(x + dx, yi + dy) || is_water(game, grid, x + dx, yi + dy)
        };
        let ty = pick::river_type(
            side(0, -1),
            side(1, 0),
            side(0, 1),
            side(-1, 0),
            pick::variation(x, yi, 1),
        );
        if let Some(tex) = art.river_overlay(ty + plane_offset) {
            draw_pic(tex, sx, sy, k);
        }
    }
    if grid.is_corrupted(x, yi) {
        if let Some(tex) = art.picture("mapback", 77, 0) {
            draw_pic(tex, sx, sy, k);
        }
    }
    // What stands on the square.
    if let Some(f) = feature {
        match f.kind {
            FeatureKind::Mineral => {
                if let Some(e) = pick::mineral_sprite(&f.id) {
                    if let Some(tex) = art.picture("mapback", e, 0) {
                        draw_pic(tex, sx, sy, k);
                    }
                }
            }
            FeatureKind::Encounter => {
                if let Some(tex) = art.picture("mapback", pick::site_sprite(&f.id), 0) {
                    draw_pic(tex, sx, sy, k);
                }
            }
            FeatureKind::Tower => match game.economy().tower_owner(here) {
                Some(owner) => {
                    let banner = banner_of(game, owner);
                    if let Some(tex) = art.banner_picture("tower_owned", banner, 0) {
                        draw_pic(tex, sx, sy, k);
                    }
                }
                None => {
                    if let Some(tex) = art.picture("mapback", 69, 0) {
                        draw_pic(tex, sx, sy, k);
                    }
                }
            },
            FeatureKind::Node => {}
        }
    }
    // Roads: a piece for each direction a road (or a city) lies in, or a lone piece.
    if grid.has_road(x, yi) {
        let magic = plane == Plane::Myrror || grid.has_enchanted_road(x, yi);
        let joins = |dx: i32, dy: i32| {
            grid.has_road(x + dx, yi + dy) || {
                let p = f4x_core::map::Position {
                    plane,
                    x: grid.wrap_x(x + dx),
                    y: (yi + dy).max(0) as u32,
                };
                game.economy().city_at(p).is_some()
            }
        };
        let mut any = false;
        for (i, &(dx, dy)) in ROAD_DIRS.iter().enumerate() {
            if joins(dx, dy) {
                any = true;
                let (entry, frame) = if magic {
                    (55 + i, anim % 6)
                } else {
                    (46 + i, 0)
                };
                if let Some(tex) = art.picture("mapback", entry, frame) {
                    draw_pic(tex, sx, sy, k);
                }
            }
        }
        if !any {
            let (entry, frame) = if magic { (54, anim % 6) } else { (45, 0) };
            if let Some(tex) = art.picture("mapback", entry, frame) {
                draw_pic(tex, sx, sy, k);
            }
        }
    }
    // The aura of a node somebody owns, in the owner's banner, shimmering.
    if let Some(node) = game.economy().node_covering(here) {
        if let Some(owner) = node.owner {
            let banner = banner_of(game, owner).min(4);
            let slot = node.aura.iter().position(|&a| a == (wx, y)).unwrap_or(0);
            let entry = 63 + usize::from(banner);
            if let Some(tex) = art.picture("mapback", entry, (anim + slot) % 6) {
                draw_pic(tex, sx, sy, k);
            }
        }
        if node.warped && node.pos == here {
            draw_warped_node(art, sx, sy, k);
        }
    }
}

/// The pixels of a node square: 20 by 18.
const NODE_W: usize = 20;
const NODE_H: usize = 18;

/// How often the warped node's picture changes: the original redraws the screen this often.
const WARP_STEPS_PER_SECOND: f64 = 18.0;

/// The warped node's square as the original draws it (`Draw_Map_Nodes`): what is on the screen
/// there is copied, half its pixels are dropped, each row and then each column is shifted by
/// up to a pixel (a different amount every frame, so it shimmers), and the result is cut to
/// the shape of `mapback.lbx` 93 and drawn over the square.
fn draw_warped_node(art: &mut Art, sx: f32, sy: f32, k: f32) {
    let mask: Vec<bool> = match art.picture("mapback", 93, 0) {
        Some(tex) => {
            let img = tex.get_texture_data();
            (0..NODE_W * NODE_H)
                .map(|i| {
                    let (x, y) = (i % NODE_W, i / NODE_W);
                    x < usize::from(img.width)
                        && y < usize::from(img.height)
                        && img.bytes[(y * usize::from(img.width) + x) * 4 + 3] > 0
                })
                .collect()
        }
        None => vec![true; NODE_W * NODE_H],
    };
    let shot = get_screen_data();
    let (width, height) = (usize::from(shot.width), usize::from(shot.height));
    let dpi = screen_dpi_scale();
    let step = (get_time() * WARP_STEPS_PER_SECOND) as u64;
    // A pixel of the square, sampled from the screen and kept half of the time.
    let mut square = [[None::<[u8; 3]>; NODE_W]; NODE_H];
    for (y, row) in square.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            if crate::cityscape::vanished(x as u32, y as u32, step, 50) {
                continue;
            }
            let px = ((sx + (x as f32 + 0.5) * k) * dpi) as usize;
            let py = ((sy + (y as f32 + 0.5) * k) * dpi) as usize;
            if px < width && py < height {
                let i = (py * width + px) * 4;
                *cell = Some([shot.bytes[i], shot.bytes[i + 1], shot.bytes[i + 2]]);
            }
        }
    }
    // The shift of each row and column: -1, 0 or 1, from the frame number.
    let shear = |i: usize| -> i32 {
        match crate::cityscape::vanished(i as u32, 99, step, 50) as u8
            + 2 * crate::cityscape::vanished(i as u32, 7, step, 75) as u8
        {
            0 => 1,
            3 => -1,
            _ => 0,
        }
    };
    let mut across = [[None::<[u8; 3]>; NODE_W]; NODE_H];
    for (y, row) in across.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            let from = x as i32 - shear(y);
            if (0..NODE_W as i32).contains(&from) {
                *cell = square[y][from as usize];
            }
        }
    }
    let mut bytes = vec![0u8; NODE_W * NODE_H * 4];
    for y in 0..NODE_H {
        for x in 0..NODE_W {
            let from = y as i32 - shear(x);
            if !(0..NODE_H as i32).contains(&from) || !mask[y * NODE_W + x] {
                continue;
            }
            if let Some(c) = across[from as usize][x] {
                let i = (y * NODE_W + x) * 4;
                bytes[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    let tex = Texture2D::from_rgba8(NODE_W as u16, NODE_H as u16, &bytes);
    tex.set_filter(FilterMode::Nearest);
    draw_pic(&tex, sx, sy, k);
}

/// Which city picture frame fits a population (in thousands): the original's `(pop - 1) / 4`,
/// at most four.
pub fn city_frame(figures: u32) -> usize {
    (figures.saturating_sub(1) / 4).min(4) as usize
}

/// Draw a city on the map: walled or open, by size, in its owner's banner. The picture is
/// 32 by 30 and reaches six pixels past the square on the top and left.
pub fn draw_city(game: &Game, art: &mut Art, city: hecs::Entity, sx: f32, sy: f32, t: Vec2) {
    let k = t.x / TILE_W;
    let owner = game
        .economy()
        .city_owner(city)
        .unwrap_or(f4x_core::player::MONSTERS);
    let banner = banner_of(game, owner);
    let figures = game.city_population(city).figures;
    let walled = game
        .city_buildings(city)
        .iter()
        .any(|&id| game.content().rules.buildings.get(id).id == "city_walls");
    let name = if walled { "city_walled" } else { "city_open" };
    if let Some(tex) = art.banner_picture(name, banner, city_frame(figures)) {
        draw_pic(tex, sx - 6.0 * k, sy - 6.0 * k, k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_city_picture_grows_every_four_thousand_citizens() {
        assert_eq!(city_frame(0), 0);
        assert_eq!(city_frame(1), 0);
        assert_eq!(city_frame(4), 0);
        assert_eq!(city_frame(5), 1);
        assert_eq!(city_frame(12), 2);
        assert_eq!(city_frame(13), 3);
        assert_eq!(city_frame(17), 4);
        assert_eq!(city_frame(25), 4);
    }
}
