//! Deterministic procedural map generation.
//!
//! Integer-only value noise (no floats) so a seed gives the same map on every
//! platform. Noise wraps horizontally so the world has no seam.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

mod original;

use crate::content::Content;
use crate::feature::def::FeatureKind;
use crate::feature::entity::Placement;
use crate::feature::place;
use crate::map::terrain::{TerrainClass, Terrains, REQUIRED_CLASSES};
use crate::map::{Grid, Map, Plane, Position, TerrainId};

/// Per-plane feature settings; Myrror is denser in minerals than Arcanus by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneParams {
    /// Share of mineral-capable land tiles that get a mineral, 0..=100.
    pub mineral_percent: u8,
    /// Number of magic nodes to place.
    pub node_count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenParams {
    pub width: u32,
    pub height: u32,
    /// Share of tiles that are land, 1..=99.
    pub land_percent: u8,
    /// Towers placed at identical coordinates on both planes.
    pub tower_count: u8,
    /// Strong and weak random encounter sites, on either plane (the original's 25 and 32).
    pub strong_sites: u32,
    pub weak_sites: u32,
    /// The game's difficulty, 0 (intro) to 4 (impossible), as `Game::with_generated_map` records
    /// it. The lairs read the two percents below, not this.
    pub difficulty: u8,
    /// Guards of lairs, towers and nodes as a percent of the budget (`world.lair_guard_percent`).
    pub lair_guard_percent: u16,
    /// Treasure of lairs as a percent (`world.lair_treasure_percent`).
    pub lair_treasure_percent: u16,
    /// The game's node setting, 0 weak to 2 powerful.
    pub magic_strength: u8,
    /// Make the land with the original's random-walk generator (`mapgen/original.rs`) instead of
    /// the noise-based one.
    pub original_generator: bool,
    /// Indexed by [`Plane::index`].
    pub planes: [PlaneParams; 2],
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            width: 60,
            height: 40,
            land_percent: 30,
            tower_count: 6,
            strong_sites: 25,
            weak_sites: 32,
            difficulty: 2,
            lair_guard_percent: 75,
            lair_treasure_percent: 133,
            magic_strength: 1,
            original_generator: false,
            planes: [
                PlaneParams {
                    mineral_percent: 6,
                    node_count: 16,
                },
                PlaneParams {
                    mineral_percent: 10,
                    node_count: 14,
                },
            ],
        }
    }
}

/// How the noise generator shares out the land it keeps off the polar caps, in thousandths: the
/// original's mix, a little under (`f4x-tools mapgen-stats` compares).
const MOUNTAIN_SHARE: u32 = 165;
const HILL_SHARE: u32 = 165;
const DESERT_SHARE: u32 = 110;
const SWAMP_SHARE: u32 = 57;
const FOREST_SHARE: u32 = 320;
/// Deserts lie in belts this many percent of the way from the equator to a pole, and the pull
/// that drives a square out of the belt, in tenths of a moisture step per percent.
const DESERT_BELT: i32 = 40;
const DESERT_PULL: i32 = 30;
/// The noise lattice of the land across a 60-wide map: more of it, more and smaller continents.
const LAND_LATTICE: u32 = 6;
/// The tundra gradient: a square of grass or forest this many rows from the pole or fewer
/// turns to tundra with the chance that a roll of 1 to this beats its row.
const TUNDRA_REACH: u32 = 3;

/// The size of the map the original's counts are for: 60 by 40 squares.
pub const REFERENCE_AREA: u32 = 60 * 40;

/// `base` per [`REFERENCE_AREA`] squares, for a map of `width` by `height`, rounded.
pub fn scaled_count(base: u32, width: u32, height: u32) -> u32 {
    (base * width * height + REFERENCE_AREA / 2) / REFERENCE_AREA
}

impl GenParams {
    /// The parameters for a map of `width` by `height`: the counts of towers, nodes and
    /// encounter sites (which are the original's for 60 by 40) follow the area.
    pub fn sized(width: u32, height: u32) -> Self {
        let d = Self::default();
        let nodes = |p: PlaneParams| PlaneParams {
            node_count: scaled_count(u32::from(p.node_count), width, height).min(255) as u8,
            ..p
        };
        Self {
            width,
            height,
            tower_count: scaled_count(u32::from(d.tower_count), width, height).min(255) as u8,
            strong_sites: scaled_count(d.strong_sites, width, height),
            weak_sites: scaled_count(d.weak_sites, width, height),
            planes: [nodes(d.planes[0]), nodes(d.planes[1])],
            ..d
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    BadParams(String),
    MissingTerrain { class: TerrainClass, plane: Plane },
    MissingFeature { kind: FeatureKind },
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::BadParams(m) => write!(f, "bad map parameters: {m}"),
            GenError::MissingTerrain { class, plane } => {
                write!(f, "no terrain of class {class:?} defined for {plane:?}")
            }
            GenError::MissingFeature { kind } => write!(f, "no feature of kind {kind:?} defined"),
        }
    }
}

impl std::error::Error for GenError {}

/// A generated map: the terrain layers, with the minerals, and the features to spawn as entities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub map: Map,
    pub placements: Vec<Placement>,
}

pub fn generate(seed: u64, params: &GenParams, content: &Content) -> Result<Generated, GenError> {
    let terrains = &content.terrains;
    if params.width < 8 || params.height < 8 {
        return Err(GenError::BadParams(
            "width and height must be at least 8".into(),
        ));
    }
    if params.width > 1024 || params.height > 1024 {
        return Err(GenError::BadParams(
            "width and height must be at most 1024".into(),
        ));
    }
    if !(1..=99).contains(&params.land_percent) {
        return Err(GenError::BadParams("land_percent must be in 1..=99".into()));
    }
    if params.planes.iter().any(|p| p.mineral_percent > 100) {
        return Err(GenError::BadParams(
            "mineral_percent must be at most 100".into(),
        ));
    }
    let [arcanus, myrror] = Plane::ALL.map(|p| generate_plane(seed, p, params, terrains));
    let mut map = Map::new(arcanus?, myrror?);
    let placed = place::place_features(seed, params, content, &mut map)?;
    let nodes = crate::feature::node::generate_nodes(seed, &map, &placed, content);
    if params.original_generator {
        original::extend_islands(seed, &mut map, &placed, content);
    }
    let lairs = crate::feature::lair::generate_lairs(seed, params, &map, &placed, &nodes, content);
    let mut placements = Vec::new();
    for plane in Plane::ALL {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(feature) = placed.get(plane, x as i32, y as i32) else {
                    continue;
                };
                let Some(def) = content.features.get(feature) else {
                    continue;
                };
                let at = Position { plane, x, y };
                match def.kind {
                    FeatureKind::Mineral => map.plane_mut(plane).set_mineral(x, y, Some(feature)),
                    // A tower is one entity, made for its Arcanus square.
                    FeatureKind::Tower if plane == Plane::Myrror => {}
                    _ => placements.push(Placement {
                        at,
                        feature,
                        node: nodes.iter().find(|(p, _)| *p == at).map(|(_, n)| n.clone()),
                        lair: lairs.iter().find(|(p, _)| *p == at).map(|(_, l)| l.clone()),
                    }),
                }
            }
        }
    }
    Ok(Generated { map, placements })
}

fn generate_plane(
    seed: u64,
    plane: Plane,
    params: &GenParams,
    terrains: &Terrains,
) -> Result<Grid, GenError> {
    let pick = |class| {
        terrains
            .first_of_class(class, plane)
            .ok_or(GenError::MissingTerrain { class, plane })
    };
    // Fail early if anything is missing, even if the generator wouldn't place it.
    for class in REQUIRED_CLASSES {
        pick(class)?;
    }
    let (ocean, grass, forest, hills, mountains, swamp, desert, tundra) = (
        pick(TerrainClass::Ocean)?,
        pick(TerrainClass::Grassland)?,
        pick(TerrainClass::Forest)?,
        pick(TerrainClass::Hills)?,
        pick(TerrainClass::Mountains)?,
        pick(TerrainClass::Swamp)?,
        pick(TerrainClass::Desert)?,
        pick(TerrainClass::Tundra)?,
    );

    let (w, h) = (params.width, params.height);
    let plane_seed =
        seed.wrapping_add((plane.index() as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    if params.original_generator {
        return Ok(original::generate_plane(
            plane_seed,
            params,
            original::Terrain {
                ocean,
                grass,
                forest,
                hills,
                mountains,
                swamp,
                desert,
                tundra,
            },
        ));
    }
    // The noise lattice is counted across the width: a wider map gets more of it, so the
    // continents keep their size in squares.
    let across = |freq: u32| ((freq * w + 30) / 60).max(2);
    let elevation = noise_field(plane_seed, w, h, across(LAND_LATTICE), 4);
    let moisture = noise_field(plane_seed ^ 0xA5A5_5A5A_1234_4321, w, h, across(5), 3);

    // Land is the top `land_percent` of elevation, and the polar caps come on top of it, as in
    // the original: the top and bottom rows are tundra, with short tundra strips on the next ones.
    let mut rng = Pcg64Mcg::seed_from_u64(plane_seed ^ 0x504F_4C41_5200);
    let mut cap = vec![false; (w * h) as usize];
    for x in 0..w {
        for row in [0, h - 1] {
            cap[(row * w + x) as usize] = true;
        }
        for row in [1, h - 2] {
            if rng.gen_range(0..4) == 0 {
                for k in 0..rng.gen_range(0..4) {
                    cap[(row * w + (x + k).min(w - 1)) as usize] = true;
                }
            }
        }
    }
    let open_sea: Vec<i32> = (0..cap.len())
        .filter(|&i| !cap[i])
        .map(|i| elevation[i])
        .collect();
    let wanted = (w * h * u32::from(params.land_percent) / 100) as usize;
    let sea_level = percentile(
        &open_sea,
        100 - (wanted * 100 / open_sea.len().max(1)).min(99),
    );
    let mut is_land: Vec<bool> = elevation.iter().map(|&e| e >= sea_level).collect();
    for (land, &c) in is_land.iter_mut().zip(&cap) {
        *land |= c;
    }

    // The rest of the land is shared out by rank: the highest ground is mountains, then hills;
    // the driest ground in the desert belts either side of the equator is desert, the wettest
    // swamp, then forest, and what is left grassland.
    let mut core: Vec<usize> = (0..(w * h) as usize)
        .filter(|&i| is_land[i] && !cap[i])
        .collect();
    let take = |core: &mut Vec<usize>, share: u32| -> Vec<usize> {
        let n = (core.len() * share as usize / 1000).min(core.len());
        core.drain(..n).collect()
    };
    core.sort_by_key(|&i| std::cmp::Reverse((elevation[i], i)));
    let mount = take(&mut core, MOUNTAIN_SHARE);
    let hilly = take(&mut core, HILL_SHARE * 1000 / (1000 - MOUNTAIN_SHARE));
    let belt = |i: usize| {
        let half = (h - 1) as i32;
        let from_equator = ((i as i32 / w as i32) * 2 - half).abs() * 100 / half;
        moisture[i] + (from_equator - DESERT_BELT).abs() * DESERT_PULL / 10
    };
    core.sort_by_key(|&i| (belt(i), i));
    let rest = 1000 - MOUNTAIN_SHARE - HILL_SHARE;
    let dry = take(&mut core, DESERT_SHARE * 1000 / rest);
    core.sort_by_key(|&i| std::cmp::Reverse((moisture[i], i)));
    let rest = rest - DESERT_SHARE;
    let wet = take(&mut core, SWAMP_SHARE * 1000 / rest);
    let green = take(&mut core, FOREST_SHARE * 1000 / (rest - SWAMP_SHARE));

    let mut grid = Grid::new(w, h, ocean);
    let mut paint = |tiles: &[usize], t: TerrainId| {
        for &i in tiles {
            grid.set(i as u32 % w, i as u32 / w, t);
        }
    };
    paint(&mount, mountains);
    paint(&hilly, hills);
    paint(&dry, desert);
    paint(&wet, swamp);
    paint(&green, forest);
    paint(&core, grass);
    let pole_band = (h / 5).min(8);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let from_pole = y.min(h - 1 - y);
            let open = grid
                .get(x as i32, y as i32)
                .is_some_and(|t| t == grass || t == forest);
            if cap[i]
                || (from_pole < pole_band && open && rng.gen_range(1..=TUNDRA_REACH) > from_pole)
            {
                grid.set(x, y, tundra);
            }
        }
    }
    place_rivers(
        &mut grid,
        &elevation,
        &is_land,
        [hills, mountains],
        ocean,
        plane_seed,
    );
    place_lakes(&mut grid, ocean, [hills, mountains], plane_seed);
    Ok(grid)
}

/// Rivers start on mountain and hill tiles and run downhill (4-neighbour steps, lowest
/// neighbour first) until they reach the sea or another river. Only gentle land carries the
/// flag: hills, mountains and ocean never do.
fn place_rivers(
    grid: &mut Grid,
    elevation: &[i32],
    is_land: &[bool],
    steep: [TerrainId; 2],
    ocean: TerrainId,
    seed: u64,
) {
    let (w, h) = (grid.width(), grid.height());
    let land = is_land.iter().filter(|&&l| l).count();
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x5249_5645_5253);
    let mut sources: Vec<(u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            is_land[(y * w + x) as usize]
                && y >= 2
                && y + 2 < h
                && elevation[(y * w + x) as usize] > 150
        })
        .collect();
    for i in (1..sources.len()).rev() {
        sources.swap(i, rng.gen_range(0..=i));
    }
    sources.truncate((land / 80).max(1));
    for (sx, sy) in sources {
        let mut seen = std::collections::HashSet::new();
        let (mut x, mut y) = (sx, sy);
        let mut flagged = Vec::new();
        let mut reached_water = false;
        let mut lake_at = None;
        for _ in 0..(w + h) {
            if !seen.insert((x, y)) {
                break;
            }
            let i = (y * w + x) as usize;
            if !is_land[i] {
                reached_water = true;
                break;
            }
            if grid.has_river(x as i32, y as i32) {
                reached_water = true;
                break;
            }
            flagged.push((x, y));
            let next = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter_map(|&(dx, dy)| {
                    let ny = y as i32 + dy;
                    (ny >= 0 && ny < h as i32).then(|| (grid.wrap_x(x as i32 + dx), ny as u32))
                })
                .filter(|p| !seen.contains(p))
                .min_by_key(|&(nx, ny)| (elevation[(ny * w + nx) as usize], ny, nx));
            match next {
                // A basin: the water has nowhere lower to go, so it pools into a lake.
                Some((nx, ny))
                    if elevation[(ny * w + nx) as usize] > elevation[i]
                        && flagged.len() >= 3
                        && y >= 2
                        && y + 2 < h =>
                {
                    lake_at = Some((x, y));
                    reached_water = true;
                    break;
                }
                Some(p) => (x, y) = p,
                None => break,
            }
        }
        if let Some((lx, ly)) = lake_at {
            flagged.retain(|&p| p != (lx, ly));
            let here = grid.get(lx as i32, ly as i32);
            if here.is_some_and(|t| !steep.contains(&t) && t != ocean) {
                grid.set(lx, ly, ocean);
                grid.set_lake(lx, ly, true);
            }
        }
        // A river that never reaches water or another river would be a puddle: drop it.
        if !reached_water {
            continue;
        }
        for (x, y) in flagged {
            if grid
                .get(x as i32, y as i32)
                .is_some_and(|t| !steep.contains(&t))
            {
                grid.set_river(x, y, true);
            }
        }
    }
}

/// A few standalone lakes: single water squares with land on every side, away from
/// rivers and from each other.
fn place_lakes(grid: &mut Grid, ocean: TerrainId, steep: [TerrainId; 2], seed: u64) {
    let (w, h) = (grid.width(), grid.height());
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x4C41_4B45_5321);
    let land = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.get(x as i32, y as i32).is_some_and(|t| t != ocean))
        .count();
    let want = land / 450;
    let mut tries = want * 40 + 20;
    let mut placed = 0;
    while placed < want && tries > 0 {
        tries -= 1;
        let (x, y) = (
            rng.gen_range(0..w),
            rng.gen_range(2..h.saturating_sub(2).max(3)),
        );
        let quiet = (-2i32..=2).all(|dy| {
            (-2i32..=2).all(|dx| {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                grid.get(nx, ny)
                    .is_some_and(|t| t != ocean && !steep.contains(&t))
                    && !grid.has_river(nx, ny)
            })
        });
        if quiet {
            grid.set(x, y, ocean);
            grid.set_lake(x, y, true);
            placed += 1;
        }
    }
}

fn percentile(values: &[i32], pct: usize) -> i32 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * pct / 100).min(sorted.len() - 1)]
}

/// Multi-octave value noise in `0..=255`, wrapping horizontally.
fn noise_field(seed: u64, w: u32, h: u32, base_freq: u32, octaves: u32) -> Vec<i32> {
    let total_weight: i32 = (1 << octaves) - 1;
    let mut out = vec![0i32; (w * h) as usize];
    for o in 0..octaves {
        let freq = base_freq << o;
        let weight = 1 << (octaves - 1 - o);
        // Cells are square: `freq` cells span the width, so y needs freq*h/w of them.
        let rows = (freq as u64 * h as u64 / w as u64) as u32 + 2;
        let mut rng =
            Pcg64Mcg::seed_from_u64(seed ^ (o as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let lattice: Vec<i32> = (0..freq * rows).map(|_| rng.gen_range(0..=255)).collect();
        let at = |ix: u32, iy: u32| lattice[(iy * freq + ix % freq) as usize];
        for y in 0..h {
            let yf = (y as u64 * freq as u64 * 256 / w as u64) as u32;
            let (iy, fy) = (yf >> 8, (yf & 255) as i32);
            for x in 0..w {
                let xf = (x as u64 * freq as u64 * 256 / w as u64) as u32;
                let (ix, fx) = (xf >> 8, (xf & 255) as i32);
                let top = lerp(at(ix, iy), at(ix + 1, iy), smooth(fx));
                let bottom = lerp(at(ix, iy + 1), at(ix + 1, iy + 1), smooth(fx));
                out[(y * w + x) as usize] += weight * lerp(top, bottom, smooth(fy));
            }
        }
    }
    out.iter_mut().for_each(|v| *v /= total_weight);
    out
}

/// Smoothstep on a 0..=256 fixed-point fraction.
fn smooth(t: i32) -> i32 {
    (t * t * (3 * 256 - 2 * t)) >> 16
}

/// Linear interpolation with a 0..=256 fixed-point fraction.
fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + (((b - a) * t) >> 8)
}

/// Fraction of land tiles on a grid, in percent (for tests and tooling).
pub fn land_percent(grid: &Grid, terrains: &Terrains) -> u32 {
    let mut land = 0;
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let id: TerrainId = grid.get(x as i32, y as i32).expect("in bounds");
            if terrains
                .get(id)
                .is_some_and(|d| d.class != TerrainClass::Ocean)
            {
                land += 1;
            }
        }
    }
    land * 100 / (grid.width() * grid.height())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content;
    use crate::testdir::TestResult;

    #[test]
    fn same_seed_same_map_different_seed_differs() -> TestResult {
        let (c, p) = (content(), GenParams::default());
        assert_eq!(generate(7, &p, &c)?, generate(7, &p, &c)?);
        assert_ne!(generate(7, &p, &c)?, generate(8, &p, &c)?);
        Ok(())
    }

    #[test]
    fn land_share_matches_request_and_planes_differ() -> TestResult {
        let (c, p) = (content(), GenParams::default());
        let map = generate(1, &p, &c)?.map;
        for plane in Plane::ALL {
            let pct = land_percent(map.plane(plane), &c.terrains);
            // The polar caps come on top of the share asked for.
            assert!(
                (p.land_percent as u32..=p.land_percent as u32 + 10).contains(&pct),
                "land {pct}%"
            );
        }
        assert_ne!(map.plane(Plane::Arcanus), map.plane(Plane::Myrror));
        Ok(())
    }

    #[test]
    fn rejects_bad_params_and_missing_content() {
        let c = content();
        let bad = GenParams {
            land_percent: 0,
            ..GenParams::default()
        };
        assert!(matches!(generate(1, &bad, &c), Err(GenError::BadParams(_))));
        let tiny = GenParams {
            width: 2,
            ..GenParams::default()
        };
        assert!(matches!(
            generate(1, &tiny, &c),
            Err(GenError::BadParams(_))
        ));

        let mut no_swamp = c.clone();
        no_swamp.terrains = Terrains::new(
            c.terrains
                .defs()
                .iter()
                .filter(|d| d.class != TerrainClass::Swamp)
                .cloned()
                .collect(),
        );
        assert!(matches!(
            generate(1, &GenParams::default(), &no_swamp),
            Err(GenError::MissingTerrain {
                class: TerrainClass::Swamp,
                ..
            })
        ));

        let mut no_tower = c.clone();
        no_tower.features = crate::feature::def::Features::new(
            c.features
                .defs()
                .iter()
                .filter(|d| d.kind != FeatureKind::Tower)
                .cloned()
                .collect(),
        );
        assert!(matches!(
            generate(1, &GenParams::default(), &no_tower),
            Err(GenError::MissingFeature {
                kind: FeatureKind::Tower
            })
        ));
    }

    #[test]
    fn rivers_exist_on_gentle_land_and_are_deterministic() -> TestResult {
        let c = content();
        let a = generate(7, &GenParams::default(), &c)?;
        let b = generate(7, &GenParams::default(), &c)?;
        assert_eq!(a, b);
        let mut count = 0;
        for plane in Plane::ALL {
            let g = a.map.plane(plane);
            for y in 0..g.height() as i32 {
                for x in 0..g.width() as i32 {
                    if g.has_river(x, y) {
                        count += 1;
                        let class = c
                            .terrains
                            .get(g.get(x, y).ok_or("get is missing")?)
                            .ok_or("get is missing")?
                            .class;
                        assert!(
                            !matches!(
                                class,
                                TerrainClass::Ocean | TerrainClass::Hills | TerrainClass::Mountains
                            ),
                            "river on {class:?}"
                        );
                    }
                }
            }
        }
        assert!(count > 0, "the default map has rivers");
        Ok(())
    }

    #[test]
    fn lakes_are_single_water_squares_away_from_the_sea() -> TestResult {
        let c = content();
        let mut lakes = 0;
        for seed in 1..=30 {
            let generated = generate(seed, &GenParams::default(), &c)?;
            let m = &generated.map;
            for plane in Plane::ALL {
                let g = m.plane(plane);
                for y in 0..g.height() as i32 {
                    for x in 0..g.width() as i32 {
                        if !g.is_lake(x, y) {
                            continue;
                        }
                        lakes += 1;
                        let class = |x, y| {
                            g.get(x, y)
                                .map(|t| c.terrains.get(t).expect("get succeeds").class)
                        };
                        assert_eq!(class(x, y), Some(TerrainClass::Ocean), "a lake is water");
                        assert!(g.mineral(x, y).is_none(), "nothing stands in a lake");
                        assert!(
                            generated
                                .placements
                                .iter()
                                .all(|p| p.at.plane != plane
                                    || (p.at.x as i32, p.at.y as i32) != (x, y)),
                            "nothing stands in a lake"
                        );
                        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            let n = class(x + dx, y + dy);
                            assert!(
                                n != Some(TerrainClass::Ocean) || g.is_lake(x + dx, y + dy),
                                "seed {seed}: lake at ({x},{y}) touches the sea"
                            );
                        }
                    }
                }
            }
        }
        assert!(lakes >= 20, "30 maps should hold lakes, found {lakes}");
        Ok(())
    }

    #[test]
    fn handles_non_square_and_odd_sizes() -> TestResult {
        let c = content();
        for (w, h) in [(8, 8), (61, 37), (120, 20), (20, 120)] {
            let p = GenParams {
                width: w,
                height: h,
                land_percent: 40,
                ..GenParams::default()
            };
            let map = generate(3, &p, &c)?.map;
            assert_eq!((map.width(), map.height()), (w, h));
        }
        Ok(())
    }

    #[test]
    fn the_counts_follow_the_area_with_the_originals_for_60_by_40() {
        assert_eq!(GenParams::sized(60, 40), GenParams::default());
        let huge = GenParams::sized(90, 60);
        assert_eq!(huge.tower_count, 14);
        assert_eq!((huge.strong_sites, huge.weak_sites), (56, 72));
        assert_eq!(huge.planes[0].node_count, 36);
        let small = GenParams::sized(45, 30);
        assert_eq!(
            (small.tower_count, small.strong_sites, small.weak_sites),
            (3, 14, 18)
        );
        assert_eq!(scaled_count(15, 75, 50), 23);
    }

    #[test]
    fn every_size_of_the_range_generates_a_valid_map() -> TestResult {
        let c = content();
        for (w, h) in [(45, 30), (75, 50), (90, 60)] {
            let p = GenParams::sized(w, h);
            let g = generate(5, &p, &c)?;
            assert_eq!((g.map.width(), g.map.height()), (w, h));
            let towers = g
                .placements
                .iter()
                .filter(|p| {
                    c.features
                        .get(p.feature)
                        .is_some_and(|f| f.kind == FeatureKind::Tower)
                })
                .count();
            assert!(
                towers <= usize::from(p.tower_count),
                "{w}x{h}: {towers} towers"
            );
            assert!(
                towers + 3 >= usize::from(p.tower_count),
                "{w}x{h}: {towers} towers"
            );
        }
        Ok(())
    }
}
