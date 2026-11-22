//! The roads a new world starts with (`Generate_Roads` of the original, wiki "Roads", section
//! "Worldgen Roads"): neutral cities on one landmass, at most ten squares apart, get a road
//! between them, as a straight line plotted by `Path_Wrap`, unless it crosses water. Every city
//! stands on a road. On Myrror the roads between cities are enchanted. Where it differs from the
//! original: the range between two cities counts the short way round the map (the original's
//! `Range` has no wrap, ReMoM marks it an OGBUG), and the squares under the cities are plain
//! roads on both planes (the original's attempt to enchant them on Myrror hits the wrong
//! squares).
//!
//! Called once for each plane after the neutral cities are made, when the setting
//! `rules.roads_at_start` is on (`f4x_newgame::setup`).
//!
//! Tests: below.

use crate::game::Game;
use crate::map::terrain::TerrainClass;
use crate::map::{Plane, Position};

/// Cities at a range of this much or more get no road between them.
const MAX_ROAD_RANGE: u32 = 10;

/// The squares of a straight line from `(x1, y1)` to `(x2, y2)` on a map `width` wide, the start
/// left out and the end in (`Path_Wrap`, a Bresenham-style line that takes the short way round
/// the wrap).
pub fn path_wrap(x1: i32, y1: i32, mut x2: i32, y2: i32, width: i32) -> Vec<(i32, i32)> {
    if (width - x2 + x1).abs() < (x2 - x1).abs() {
        x2 -= width;
    }
    if (width - x1 + x2).abs() < (x2 - x1).abs() {
        x2 += width;
    }
    let (dx, dy) = (x2 - x1, y2 - y1);
    let (sign_x, sign_y) = (if dx < 0 { -1 } else { 1 }, if dy < 0 { -1 } else { 1 });
    let (dx, dy) = (dx.abs(), dy.abs());
    let (mut x, mut y) = (x1, y1);
    let mut out = Vec::new();
    if dx < dy {
        let add = (dx << 16) / dy;
        let mut rest: i32 = 0x8000;
        for _ in 0..dy {
            y += sign_y;
            rest += add;
            if rest >= 0x10000 {
                x += sign_x;
            }
            rest &= 0xFFFF;
            out.push((x, y));
        }
    } else if dy < dx {
        let add = (dy << 16) / dx;
        let mut rest: i32 = 0x8000;
        for _ in 0..dx {
            x += sign_x;
            rest += add;
            if rest >= 0x10000 {
                y += sign_y;
            }
            rest &= 0xFFFF;
            out.push((x, y));
        }
    } else {
        for _ in 0..dy {
            x += sign_x;
            y += sign_y;
            out.push((x, y));
        }
    }
    for p in &mut out {
        if p.0 < 0 {
            p.0 += width;
        }
        if p.0 >= width {
            p.0 -= width;
        }
    }
    out
}

impl Game {
    /// Lay the roads a new world starts with on `plane` (see the module).
    pub fn lay_start_roads(&mut self, plane: Plane) {
        let Some(width) = self.map.as_ref().map(|m| m.plane(plane).width()) else {
            return;
        };
        let land = self.landmasses(plane);
        let cities: Vec<(Position, bool)> = self
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, _, p)| p.plane == plane)
            .map(|(_, info, p)| (p, self.is_neutral_player(info.owner)))
            .collect();
        let mass = |p: Position| land.get((p.y * width + p.x) as usize).copied().unwrap_or(0);
        let mut roads: Vec<(u32, u32)> = Vec::new();
        for (i, &(from, from_neutral)) in cities.iter().enumerate() {
            for &(to, to_neutral) in &cities[i + 1..] {
                if !from_neutral || !to_neutral || mass(from) != mass(to) {
                    continue;
                }
                let dx = from.x.abs_diff(to.x);
                let (dx, dy) = (dx.min(width - dx), from.y.abs_diff(to.y));
                if dx.max(dy) + dx.min(dy) / 2 > MAX_ROAD_RANGE {
                    continue;
                }
                let line = path_wrap(
                    from.x as i32,
                    from.y as i32,
                    to.x as i32,
                    to.y as i32,
                    width as i32,
                );
                // The last square is the city at the end: neither tested nor laid here.
                let steps = &line[..line.len().saturating_sub(1)];
                let wet = steps.iter().any(|&(x, y)| {
                    self.economy().terrain_class_at(Position {
                        plane,
                        x: x as u32,
                        y: y as u32,
                    }) == Some(TerrainClass::Ocean)
                });
                if !wet {
                    roads.extend(steps.iter().map(|&(x, y)| (x as u32, y as u32)));
                }
            }
        }
        let Some(map) = self.map.as_mut() else {
            return;
        };
        let grid = map.plane_mut(plane);
        for (x, y) in roads {
            grid.set_road(x, y, true);
            if plane == Plane::Myrror {
                grid.set_enchanted_road(x, y, true);
            }
        }
        for (p, _) in cities {
            grid.set_road(p.x, p.y, true);
        }
    }
}
