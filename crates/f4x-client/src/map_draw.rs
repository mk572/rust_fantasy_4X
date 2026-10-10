//! Drawing one overland map square the way the original does: ground, river, corruption,
//! the thing standing on it (mineral, site, tower, node), roads, and the owner's node aura.

use f4x_core::city::PlayerId;
use f4x_core::content::{FeatureKind, TerrainClass};
use f4x_core::map::{Grid, Plane};
use f4x_core::Game;
use macroquad::prelude::*;

use crate::art::{terrain_type as tt, Art, TILE_W};
use crate::terrain_pick as pick;

/// Which banner a player's things are drawn in.
pub fn banner_of(game: &Game, owner: u8) -> u8 {
    game.player(PlayerId(owner))
        .map_or(f4x_core::player::NEUTRAL_BANNER, |p| p.banner)
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

/// Whether a neighbouring square counts as water for a river that runs into it.
fn is_water(game: &Game, grid: &Grid, x: i32, y: i32) -> bool {
    pick::class_at(grid, &game.content, x, y) == Some(TerrainClass::Ocean)
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
/// the animation step shared by water, roads and node auras.
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
    let Some(map) = game.map.as_ref() else {
        return;
    };
    let grid = map.plane(plane);
    let content = &game.content;
    let k = t.x / TILE_W;
    let (x, yi) = (wx as i32, y as i32);
    let plane_offset = if plane == Plane::Myrror {
        tt::MYRROR_OFFSET
    } else {
        0
    };
    let feature = grid.feature(x, yi).and_then(|f| content.features.get(f));
    // The ground: a node's own piece, or the type the neighbours call for.
    let ground = feature
        .filter(|f| f.kind == FeatureKind::Node)
        .map(|f| pick::node_type(&f.id))
        .or_else(|| {
            pick::ground_type(grid, content, x, yi, plane.index(), &|table, mask| {
                art.autotile_type(table, mask)
            })
        });
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
            FeatureKind::Tower => match map.tower_owner(wx, y) {
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
                let p = f4x_core::city::Position {
                    plane,
                    x: grid.wrap_x(x + dx),
                    y: (yi + dy).max(0) as u32,
                };
                game.city_at(p).is_some()
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
    if let Some(i) = map.node_covering(plane, wx, y) {
        let node = &map.nodes()[i];
        if let Some(owner) = node.owner {
            let banner = banner_of(game, owner).min(4);
            let slot = node.aura.iter().position(|&a| a == (wx, y)).unwrap_or(0);
            let entry = 63 + usize::from(banner);
            if let Some(tex) = art.picture("mapback", entry, (anim + slot) % 6) {
                draw_pic(tex, sx, sy, k);
            }
        }
        if node.warped && node.x == wx && node.y == y {
            draw_rectangle(sx, sy, t.x, t.y, Color::new(0.45, 0.0, 0.5, 0.35));
        }
    }
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
    let owner = game.city_owner(city).map_or(u8::MAX, |o| o.0);
    let banner = banner_of(game, owner);
    let figures = game
        .world
        .get::<&f4x_core::economy::Population>(city)
        .map_or(0, |p| p.figures);
    let walled = game
        .world
        .get::<&f4x_core::economy::Buildings>(city)
        .is_ok_and(|b| {
            b.0.iter()
                .any(|&id| game.content.rules.buildings.get(id).id == "city_walls")
        });
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
