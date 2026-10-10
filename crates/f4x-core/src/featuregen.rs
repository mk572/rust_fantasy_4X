//! Places feature overlays (towers, nodes, encounter sites, minerals) on a
//! generated map. Order matters: towers first (they convert water to grass and
//! constrain nodes), then nodes, encounter sites, and finally minerals.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::content::{Content, FeatureKind, TerrainClass};
use crate::map::{FeatureId, Grid, Map, Plane};
use crate::mapgen::{GenError, GenParams};

/// Minimum distance between towers (when the map has room for it).
const TOWER_SPACING: u32 = 10;
/// Nodes keep at least this distance from towers and from each other.
const NODE_SPACING: u32 = 4;
/// Towers and nodes stay away from the polar rows.
const POLE_MARGIN: u32 = 3;

pub fn place_features(
    seed: u64,
    params: &GenParams,
    content: &Content,
    map: &mut Map,
) -> Result<(), GenError> {
    let towers = place_towers(seed, params, content, map)?;
    for plane in Plane::ALL {
        let salt = 0x51ED_2701_F00D_0000u64 + plane.index() as u64;
        let mut rng = Pcg64Mcg::seed_from_u64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let pp = params.planes[plane.index()];
        let grid = map.plane_mut(plane);

        place_nodes(
            &mut rng,
            plane,
            pp.node_count as usize,
            &towers,
            content,
            grid,
        );

        let land = candidates(grid, content, |_| true).len() as u32;
        if let Some(n) = land.checked_div(pp.encounter_ratio) {
            place_weighted(
                &mut rng,
                plane,
                FeatureKind::Encounter,
                n as usize,
                content,
                grid,
            );
        }

        let mineral_tiles = candidates(grid, content, |c| {
            !content
                .features
                .weighted(FeatureKind::Mineral, c, plane)
                .is_empty()
        })
        .len();
        let n = mineral_tiles * pp.mineral_percent as usize / 100;
        place_weighted(&mut rng, plane, FeatureKind::Mineral, n, content, grid);
    }
    Ok(())
}

/// Towers share coordinates across planes; water under a tower becomes grass.
fn place_towers(
    seed: u64,
    params: &GenParams,
    content: &Content,
    map: &mut Map,
) -> Result<Vec<(u32, u32)>, GenError> {
    if params.tower_count == 0 {
        return Ok(Vec::new());
    }
    let tower =
        content
            .features
            .first_of_kind(FeatureKind::Tower)
            .ok_or(GenError::MissingFeature {
                kind: FeatureKind::Tower,
            })?;
    let (w, h) = (map.width(), map.height());
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x70E4_5EED_0000_0001);
    let mut spots: Vec<(u32, u32)> = Vec::new();
    for _ in 0..params.tower_count {
        for _ in 0..200 {
            let pos = (
                rng.gen_range(0..w),
                rng.gen_range(POLE_MARGIN..h - POLE_MARGIN),
            );
            let reference = map.plane(Plane::Arcanus);
            if spots
                .iter()
                .all(|&o| reference.distance_sq(o, pos) >= TOWER_SPACING.pow(2))
            {
                spots.push(pos);
                break;
            }
        }
    }
    for plane in Plane::ALL {
        let grass = content
            .terrains
            .first_of_class(TerrainClass::Grassland, plane)
            .ok_or(GenError::MissingTerrain {
                class: TerrainClass::Grassland,
                plane,
            })?;
        let grid = map.plane_mut(plane);
        for &(x, y) in &spots {
            let is_water = grid
                .get(x as i32, y as i32)
                .and_then(|t| content.terrains.get(t))
                .is_none_or(|d| d.class == TerrainClass::Ocean);
            if is_water {
                grid.set(x, y, grass);
            }
            grid.set_feature(x, y, Some(tower));
        }
    }
    Ok(spots)
}

fn place_nodes(
    rng: &mut Pcg64Mcg,
    plane: Plane,
    count: usize,
    towers: &[(u32, u32)],
    content: &Content,
    grid: &mut Grid,
) {
    let mut placed: Vec<(u32, u32)> = Vec::new();
    let mut spots = candidates(grid, content, |_| true);
    shuffle(rng, &mut spots);
    for (x, y, class) in spots {
        if placed.len() >= count {
            break;
        }
        let far_enough = |o: &(u32, u32)| grid.distance_sq(*o, (x, y)) >= NODE_SPACING.pow(2);
        if y < POLE_MARGIN
            || y >= grid.height() - POLE_MARGIN
            || !towers.iter().all(far_enough)
            || !placed.iter().all(far_enough)
        {
            continue;
        }
        if let Some(id) = pick(
            rng,
            &content.features.weighted(FeatureKind::Node, class, plane),
        ) {
            grid.set_feature(x, y, Some(id));
            placed.push((x, y));
        }
    }
}

/// Place up to `count` features of `kind` on free land tiles that allow one.
fn place_weighted(
    rng: &mut Pcg64Mcg,
    plane: Plane,
    kind: FeatureKind,
    count: usize,
    content: &Content,
    grid: &mut Grid,
) {
    let mut spots = candidates(grid, content, |_| true);
    shuffle(rng, &mut spots);
    let mut placed = 0;
    for (x, y, class) in spots {
        if placed >= count {
            break;
        }
        if let Some(id) = pick(rng, &content.features.weighted(kind, class, plane)) {
            grid.set_feature(x, y, Some(id));
            placed += 1;
        }
    }
}

/// Free (feature-less) land tiles whose terrain class passes `filter`, in scan order.
fn candidates(
    grid: &Grid,
    content: &Content,
    filter: impl Fn(TerrainClass) -> bool,
) -> Vec<(u32, u32, TerrainClass)> {
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if grid.feature(x as i32, y as i32).is_some() {
                continue;
            }
            let class = grid
                .get(x as i32, y as i32)
                .and_then(|t| content.terrains.get(t))
                .map(|d| d.class);
            if let Some(class) = class.filter(|&c| c != TerrainClass::Ocean && filter(c)) {
                out.push((x, y, class));
            }
        }
    }
    out
}

fn shuffle<T>(rng: &mut Pcg64Mcg, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        v.swap(i, rng.gen_range(0..=i));
    }
}

fn pick(rng: &mut Pcg64Mcg, weights: &[(FeatureId, u32)]) -> Option<FeatureId> {
    let total: u32 = weights.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.gen_range(0..total);
    for &(id, w) in weights {
        if roll < w {
            return Some(id);
        }
        roll -= w;
    }
    None
}
