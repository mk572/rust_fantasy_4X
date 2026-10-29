//! The original's land generator, for the setting `original_map_generator`.
//!
//! `NEWG_CreateLands` grows continents by random walks and counts the visits to each square;
//! `Translate_Heightmap_To_Base_Terrain_Types` turns the count into terrain (1 grassland or a
//! quarter forest, 2 to 3 forest, 4 to 5 hills, 6 and more mountains) and `Add_Tundra` paints the
//! poles; `Generate_Climate_Terrain_Types` adds the tundra gradient and random-walked deserts and
//! swamps (`~/projects/ReMoM/doc/SRD-NEWG_CreateLands.md`, `doc/NewGame/MAPGEN-*.md`). Ours takes
//! the same steps with the constants scaled to the map size; rivers and lakes are ours, run over the
//! visit counts as the height. `Extend_Islands` follows the features (see [`extend_islands`]).
//! Not ported: the landmass ids (nothing here reads them).
//!
//! The draws are our own seeded generator, so a seed gives another map than the original's.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::content::Content;
use crate::feature::def::FeatureKind;
use crate::feature::place::Placed;
use crate::map::terrain::TerrainClass;
use crate::map::{Map, Plane};

use super::{place_lakes, place_rivers, scaled_count, GenParams};
use crate::map::{Grid, TerrainId};

/// The eight terrains of the plane the generator paints with.
pub struct Terrain {
    pub ocean: TerrainId,
    pub grass: TerrainId,
    pub forest: TerrainId,
    pub hills: TerrainId,
    pub mountains: TerrainId,
    pub swamp: TerrainId,
    pub desert: TerrainId,
    pub tundra: TerrainId,
}

/// South, west, north, east and none: the original's `TILE_Cardinal_XMod`/`YMod`.
const DX: [i32; 5] = [0, -1, 0, 1, 0];
const DY: [i32; 5] = [1, 0, -1, 0, 0];

/// The original's `Random(n)`: 1 to n.
fn random(rng: &mut Pcg64Mcg, n: u32) -> i32 {
    rng.gen_range(1..=n.max(1)) as i32
}

/// Steps of one arm for the land size: the original's small (6 to 15), medium (11 to 20) and
/// large (21 to 30), chosen by the land share its three sizes have (15, 20 and 30 percent).
fn arm_steps(rng: &mut Pcg64Mcg, land_percent: u8) -> i32 {
    let base = match land_percent {
        0..=17 => 5,
        18..=24 => 10,
        _ => 20,
    };
    base + random(rng, 10)
}

/// Land as visit counts (0 is ocean): random-walked continents until the land share is reached.
fn create_lands(rng: &mut Pcg64Mcg, w: i32, h: i32, land_percent: u8) -> Vec<u8> {
    let mut hits = vec![0u8; (w * h) as usize];
    // The walkers keep 2 squares from the sides and 4 from the poles; land cannot reach further.
    let reachable = ((w - 5) * (h - 9)).max(1);
    let wanted = (w * h * i32::from(land_percent) / 100).min(reachable * 7 / 10);
    // Origins are spread over map sections of 16 by 11 squares, up to eight of them at first.
    let (sw, sh) = (16, 11);
    let cols = (w - 8) / sw + 1;
    let rows = (h - 8) / sh + 1;
    let mut used = vec![false; (cols * rows) as usize];
    let mut sections_left = 8.min(cols * rows);
    let mut made = 0;
    let mut blobs = 0;
    while made <= wanted && blobs < 100_000 {
        blobs += 1;
        let (bx, by) = loop {
            let bx = 6 + random(rng, (w - 14) as u32);
            let by = 6 + random(rng, (h - 14) as u32);
            if sections_left <= 0 {
                break (bx, by);
            }
            let cell = (by / sh * cols + bx / sw) as usize;
            if !used[cell] {
                used[cell] = true;
                sections_left -= 1;
                break (bx, by);
            }
        };
        let arms = 2 + random(rng, 3);
        for arm in 0..arms as usize {
            let steps = arm_steps(rng, land_percent);
            let (mut x, mut y) = (bx + DX[arm], by + DY[arm]);
            let (mut last, mut same) = (usize::MAX, 1);
            for _ in 0..steps {
                if made > wanted {
                    break;
                }
                let i = (y * w + x) as usize;
                if hits[i] == 0 {
                    made += 1;
                }
                hits[i] = hits[i].saturating_add(1);
                // The next square: a random side, seldom the same as the last (1 in twice the
                // run so far), never into the margins.
                let (nx, ny) = loop {
                    let dir = (random(rng, 4) - 1) as usize;
                    if dir == last {
                        if random(rng, same * 2) != 1 {
                            continue;
                        }
                        same += 1;
                    } else {
                        same = 1;
                    }
                    let (nx, ny) = (x + DX[dir], y + DY[dir]);
                    if nx < 2 || ny < 4 || nx >= w - 3 || ny >= h - 5 {
                        last = usize::MAX;
                        continue;
                    }
                    last = dir;
                    break (nx, ny);
                };
                (x, y) = (nx, ny);
            }
        }
    }
    hits
}

/// A walk of `steps` squares from `(x, y)`; x wraps, a step off the map's top or bottom is lost.
fn walk(
    rng: &mut Pcg64Mcg,
    grid: &Grid,
    mut at: (i32, i32),
    steps: i32,
    mut paint: impl FnMut((u32, u32)),
) {
    let (w, h) = (grid.width(), grid.height() as i32);
    paint((grid.wrap_x(at.0), at.1.clamp(0, h - 1) as u32));
    for _ in 0..steps {
        let dir = (random(rng, 4) - 1) as usize;
        let y = at.1 + DY[dir];
        at = (at.0 + DX[dir], if (0..h).contains(&y) { y } else { at.1 });
        let x = at.0.rem_euclid(w as i32);
        at.0 = x;
        paint((x as u32, at.1 as u32));
    }
}

/// The poles, then the tundra gradient, the deserts and the swamps.
fn climate(rng: &mut Pcg64Mcg, grid: &mut Grid, t: &Terrain) {
    let (w, h) = (grid.width() as i32, grid.height() as i32);
    let is = |grid: &Grid, x: i32, y: i32, id| grid.get(x, y) == Some(id);
    for y in 2..8 {
        for x in 0..w {
            for row in [y, h - 1 - y] {
                let open = is(grid, x, row, t.grass) || is(grid, x, row, t.forest);
                if open && 2 + random(rng, 8) >= y {
                    grid.set(x as u32, row as u32, t.tundra);
                }
            }
        }
    }
    for _ in 0..scaled_count(8, w as u32, h as u32).max(1) {
        let (bx, by) = (
            2 + random(rng, (w - 6) as u32),
            8 + random(rng, (h - 16) as u32),
        );
        if is(grid, bx, by, t.forest) {
            grid.set(bx as u32, by as u32, t.desert);
        }
        for dir in 0..5 {
            let steps = 4 + random(rng, 6);
            let start = (bx + DX[dir], by + DY[dir]);
            let mut painted = Vec::new();
            walk(rng, grid, start, steps, |p| painted.push(p));
            for (x, y) in painted {
                if !is(grid, x as i32, y as i32, t.ocean) {
                    grid.set(x, y, t.desert);
                }
            }
        }
    }
    for _ in 0..scaled_count(8, w as u32, h as u32).max(1) {
        let (bx, by) = (
            1 + random(rng, (w - 4) as u32),
            10 + random(rng, (h - 20) as u32),
        );
        if is(grid, bx, by, t.forest) {
            grid.set(bx as u32, by as u32, t.swamp);
        }
        for dir in 0..5 {
            let steps = 2 + random(rng, 3);
            let start = (bx + DX[dir], by + DY[dir]);
            let mut painted = Vec::new();
            walk(rng, grid, start, steps, |p| painted.push(p));
            for (x, y) in painted {
                if is(grid, x as i32, y as i32, t.forest) {
                    grid.set(x, y, t.swamp);
                }
            }
        }
    }
}

pub fn generate_plane(plane_seed: u64, params: &GenParams, t: Terrain) -> Grid {
    let mut rng = Pcg64Mcg::seed_from_u64(plane_seed ^ 0x4F52_4947_494E);
    let (w, h) = (params.width, params.height);
    let hits = create_lands(&mut rng, w as i32, h as i32, params.land_percent);
    let mut grid = Grid::new(w, h, t.ocean);
    for y in 0..h {
        for x in 0..w {
            let terrain = match hits[(y * w + x) as usize] {
                0 => continue,
                6.. => t.mountains,
                4..=5 => t.hills,
                2..=3 => t.forest,
                _ if random(&mut rng, 4) == 1 => t.forest,
                _ => t.grass,
            };
            grid.set(x, y, terrain);
        }
    }
    // Add_Tundra: the top and bottom rows, and short strips on the rows next to them.
    for x in 0..w {
        grid.set(x, 0, t.tundra);
        grid.set(x, h - 1, t.tundra);
        for row in [1, h - 2] {
            if random(&mut rng, 4) == 1 {
                for k in 0..random(&mut rng, 4) as u32 {
                    if x + k < w {
                        grid.set(x + k, row, t.tundra);
                    }
                }
            }
        }
    }
    climate(&mut rng, &mut grid, &t);
    let is_land: Vec<bool> = hits.iter().map(|&n| n > 0).collect();
    let height: Vec<i32> = hits.iter().map(|&n| i32::from(n) * 40).collect();
    place_rivers(
        &mut grid,
        &height,
        &is_land,
        [t.hills, t.mountains],
        t.ocean,
        plane_seed,
    );
    place_lakes(&mut grid, t.ocean, [t.hills, t.mountains], plane_seed);
    grid
}

/// `Extend_Islands`: a node or tower with ocean on all four sides gets some ground. With a 2 in 3
/// chance it makes 1 + 1..8 attempts, each turning one random neighbouring ocean square (never one
/// holding a node or tower) into grassland; a diagonal square only when it would not touch the
/// land at a corner alone. The original picks the neighbour with a broken index, so its islands
/// only ever grow east and south; here every direction is possible.
pub fn extend_islands(seed: u64, map: &mut Map, placed: &Placed, content: &Content) {
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x4953_4C41_4E44);
    let (w, h) = (map.width() as i32, map.height() as i32);
    let marks = |plane, x, y| {
        placed
            .get(plane, x, y)
            .and_then(|f| content.features.get(f))
            .is_some_and(|d| matches!(d.kind, FeatureKind::Node | FeatureKind::Tower))
    };
    for plane in Plane::ALL {
        let Some(grass) = content
            .terrains
            .first_of_class(TerrainClass::Grassland, plane)
        else {
            continue;
        };
        let ocean = |map: &Map, x: i32, y: i32| {
            let grid = map.plane(plane);
            !grid.is_lake(x, y)
                && grid
                    .get(x, y)
                    .and_then(|t| content.terrains.get(t))
                    .is_some_and(|d| d.class == TerrainClass::Ocean)
        };
        for y in 1..h - 1 {
            for x in 0..w {
                if !marks(plane, x, y)
                    || !(0..4).all(|d| ocean(map, x + DX[d], y + DY[d]))
                    || random(&mut rng, 3) <= 1
                {
                    continue;
                }
                for _ in 0..1 + random(&mut rng, 8) {
                    let (dx, dy) = (random(&mut rng, 3) - 2, random(&mut rng, 3) - 2);
                    let (nx, ny) = (x + dx, y + dy);
                    if (dx, dy) == (0, 0) || ny < 0 || ny >= h || marks(plane, nx, ny) {
                        continue;
                    }
                    let bridge_only =
                        dx != 0 && dy != 0 && ocean(map, x + dx, y) && ocean(map, x, y + dy);
                    if ocean(map, nx, ny) && !bridge_only {
                        let nx = map.plane(plane).wrap_x(nx);
                        map.plane_mut(plane).set(nx, ny as u32, grass);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
