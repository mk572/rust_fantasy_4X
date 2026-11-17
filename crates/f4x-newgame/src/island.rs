//! Islands that are too small to play on. A tower, a node or a wizard's start town on a speck of
//! land (one or two squares, sometimes a few more) gives its owner nothing to build on and nowhere
//! to walk: the ground round it is built out until the land mass has [`MIN_LAND`] squares. The
//! generator does it for the towers and nodes of either generator, the setup for the start towns
//! once they are chosen.
//!
//! [`growth`] only plans: it names the ocean squares to turn into grassland, so the same plan
//! works on a map being generated and on a game already begun.
//!
//! Tests: in `tests` below.

use std::collections::HashSet;

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Map, Plane, Position};
use f4x_def::content::Content;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

/// The fewest squares of land a mass holding a tower, a node or a start town gets: a three by
/// three block, room for a landing and a foothold.
pub const MIN_LAND: usize = 12;

/// The four sides of a square.
const SIDES: [(i32, i32); 4] = [(0, 1), (-1, 0), (0, -1), (1, 0)];
/// The eight neighbours, as a unit walks.
const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn is_water(map: &Map, content: &Content, plane: Plane, x: i32, y: i32) -> Option<bool> {
    let grid = map.plane(plane);
    let class = content.terrains.get(grid.get(x, y)?)?.class;
    Some(class == TerrainClass::Ocean)
}

/// The squares of the land mass that `at` is on (walking squares touch at a corner too), at most
/// `limit` of them: a mass of `limit` squares is big enough and the count stops there.
pub fn land_mass(map: &Map, content: &Content, at: Position, limit: usize) -> HashSet<(u32, u32)> {
    let grid = map.plane(at.plane);
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    if is_water(map, content, at.plane, at.x as i32, at.y as i32) != Some(false) {
        return seen;
    }
    let mut todo = vec![(at.x, at.y)];
    seen.insert((at.x, at.y));
    while let Some((x, y)) = todo.pop() {
        if seen.len() >= limit {
            break;
        }
        for (dx, dy) in AROUND {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if is_water(map, content, at.plane, nx, ny) != Some(false) {
                continue;
            }
            let key = (grid.wrap_x(nx), ny as u32);
            if seen.insert(key) {
                todo.push(key);
            }
        }
    }
    seen
}

/// The ocean squares to turn into grassland so that the land mass of `at` has [`MIN_LAND`]
/// squares: none when it has that many already. A square is added beside the land (a side, never
/// a corner alone), picked by `seed`; `taken` names the squares that must stay as they are (a
/// feature stands on them).
pub fn growth(
    map: &Map,
    content: &Content,
    seed: u64,
    at: Position,
    taken: &dyn Fn(i32, i32) -> bool,
) -> Vec<(u32, u32)> {
    let mut land = land_mass(map, content, at, MIN_LAND);
    if land.is_empty() || land.len() >= MIN_LAND {
        return Vec::new();
    }
    let grid = map.plane(at.plane);
    let mut rng =
        Pcg64Mcg::seed_from_u64(seed ^ 0x4252_4944_4745 ^ u64::from(at.x) << 20 ^ u64::from(at.y));
    let mut added = Vec::new();
    while land.len() < MIN_LAND {
        let mut frontier: Vec<(u32, u32)> = Vec::new();
        for &(x, y) in &land {
            for (dx, dy) in SIDES {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                let key = (grid.wrap_x(nx), ny.max(0) as u32);
                if ny < 0
                    || is_water(map, content, at.plane, nx, ny) != Some(true)
                    || grid.is_lake(nx, ny)
                    || taken(nx, ny)
                    || land.contains(&key)
                {
                    continue;
                }
                frontier.push(key);
            }
        }
        frontier.sort_unstable();
        frontier.dedup();
        if frontier.is_empty() {
            break;
        }
        let pick = frontier[rng.gen_range(0..frontier.len())];
        land.insert(pick);
        added.push(pick);
    }
    added
}

/// Build out the island of every tower and node of the map to [`MIN_LAND`] squares, on both
/// planes. `sites` are the squares of the towers and nodes; `taken` says whether a square holds a
/// feature.
pub fn grow_site_islands(
    seed: u64,
    map: &mut Map,
    content: &Content,
    sites: &[Position],
    taken: &dyn Fn(Plane, i32, i32) -> bool,
) {
    for &site in sites {
        let Some(grass) = content
            .terrains
            .first_of_class(TerrainClass::Grassland, site.plane)
        else {
            continue;
        };
        let squares = growth(map, content, seed, site, &|x, y| taken(site.plane, x, y));
        for (x, y) in squares {
            map.plane_mut(site.plane).set(x, y, grass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::map::Grid;
    use f4x_def::testdir::{shipped, TestResult};

    fn ocean_map(c: &Content) -> Result<(Map, f4x_def::map::TerrainId), &'static str> {
        let ocean = c
            .terrains
            .first_of_class(TerrainClass::Ocean, Plane::Arcanus)
            .ok_or("no ocean in the content")?;
        Ok((
            Map::new(Grid::new(30, 20, ocean), Grid::new(30, 20, ocean)),
            ocean,
        ))
    }

    #[test]
    fn a_speck_of_land_is_built_out_to_the_least_a_mass_may_have() -> TestResult {
        let c = shipped();
        let grass = c
            .terrains
            .first_of_class(TerrainClass::Grassland, Plane::Arcanus)
            .ok_or("no grassland in the content")?;
        for seed in 0..40 {
            let (mut map, _) = ocean_map(c)?;
            let site = Position {
                plane: Plane::Arcanus,
                x: 10,
                y: 10,
            };
            map.plane_mut(Plane::Arcanus).set(10, 10, grass);
            grow_site_islands(seed, &mut map, c, &[site], &|_, _, _| false);
            let mass = land_mass(&map, c, site, usize::MAX);
            assert_eq!(mass.len(), MIN_LAND, "seed {seed}");
        }
        Ok(())
    }

    #[test]
    fn a_mass_that_is_big_enough_is_left_alone() -> TestResult {
        let c = shipped();
        let (mut map, _) = ocean_map(c)?;
        let grass = c
            .terrains
            .first_of_class(TerrainClass::Grassland, Plane::Arcanus)
            .ok_or("no grassland in the content")?;
        for x in 5..5 + MIN_LAND as u32 {
            map.plane_mut(Plane::Arcanus).set(x, 5, grass);
        }
        let site = Position {
            plane: Plane::Arcanus,
            x: 5,
            y: 5,
        };
        assert!(growth(&map, c, 1, site, &|_, _| false).is_empty());
        Ok(())
    }

    #[test]
    fn ground_that_must_stay_is_not_built_on_and_the_growth_stays_on_the_map() -> TestResult {
        let c = shipped();
        let (mut map, _) = ocean_map(c)?;
        let grass = c
            .terrains
            .first_of_class(TerrainClass::Grassland, Plane::Arcanus)
            .ok_or("no grassland in the content")?;
        // On the top row, with the square to the east kept free.
        map.plane_mut(Plane::Arcanus).set(3, 0, grass);
        let site = Position {
            plane: Plane::Arcanus,
            x: 3,
            y: 0,
        };
        let squares = growth(&map, c, 7, site, &|x, y| (x, y) == (4, 0));
        assert!(!squares.contains(&(4, 0)));
        assert!(squares.iter().all(|&(_, y)| y < 20));
        assert_eq!(squares.len(), MIN_LAND - 1);
        Ok(())
    }
}
