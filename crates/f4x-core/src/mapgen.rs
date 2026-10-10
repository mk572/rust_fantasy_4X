//! Deterministic procedural map generation.
//!
//! Integer-only value noise (no floats) so a seed gives the same map on every
//! platform. Noise wraps horizontally so the world has no seam.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::content::{Content, FeatureKind, TerrainClass, Terrains, REQUIRED_CLASSES};
use crate::featuregen;
use crate::map::{Grid, Map, Plane, TerrainId};

/// Per-plane feature settings; Myrror is denser in minerals than Arcanus by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneParams {
    /// Share of mineral-capable land tiles that get a mineral, 0..=100.
    pub mineral_percent: u8,
    /// Number of magic nodes to place.
    pub node_count: u8,
    /// One encounter site per this many land tiles; 0 disables encounters.
    pub encounter_ratio: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenParams {
    pub width: u32,
    pub height: u32,
    /// Share of tiles that are land, 1..=99.
    pub land_percent: u8,
    /// Towers placed at identical coordinates on both planes.
    pub tower_count: u8,
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
            planes: [
                PlaneParams {
                    mineral_percent: 6,
                    node_count: 16,
                    encounter_ratio: 10,
                },
                PlaneParams {
                    mineral_percent: 10,
                    node_count: 14,
                    encounter_ratio: 10,
                },
            ],
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

pub fn generate(seed: u64, params: &GenParams, content: &Content) -> Result<Map, GenError> {
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
    featuregen::place_features(seed, params, content, &mut map)?;
    Ok(map)
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
    let elevation = noise_field(plane_seed, w, h, 3, 4);
    let moisture = noise_field(plane_seed ^ 0xA5A5_5A5A_1234_4321, w, h, 5, 3);

    // Land is the top `land_percent` of elevation.
    let sea_level = percentile(&elevation, 100 - params.land_percent as usize);
    let is_land: Vec<bool> = elevation.iter().map(|&e| e >= sea_level).collect();

    let land_elev: Vec<i32> = pick_where(&elevation, &is_land);
    let land_moist: Vec<i32> = pick_where(&moisture, &is_land);
    let mountain_at = percentile(&land_elev, 94);
    let hills_at = percentile(&land_elev, 80);
    let desert_below = percentile(&land_moist, 10);
    let swamp_above = percentile(&land_moist, 92);
    let forest_above = percentile(&land_moist, 67);
    let polar_rows = h / 12;

    let mut grid = Grid::new(w, h, ocean);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if !is_land[i] {
                continue;
            }
            let (e, m) = (elevation[i], moisture[i]);
            let from_pole = y.min(h - 1 - y);
            let t = if from_pole < polar_rows {
                tundra
            } else if e >= mountain_at {
                mountains
            } else if e >= hills_at {
                hills
            } else if m <= desert_below {
                desert
            } else if m >= swamp_above {
                swamp
            } else if m >= forest_above {
                forest
            } else {
                grass
            };
            grid.set(x, y, t);
        }
    }
    Ok(grid)
}

fn pick_where(values: &[i32], mask: &[bool]) -> Vec<i32> {
    values
        .iter()
        .zip(mask)
        .filter(|(_, &m)| m)
        .map(|(&v, _)| v)
        .collect()
}

/// Value at the given percentile (0..=100) of `values`; empty input yields 0.
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
    use crate::testutil::content;

    #[test]
    fn same_seed_same_map_different_seed_differs() {
        let (c, p) = (content(), GenParams::default());
        assert_eq!(generate(7, &p, &c).unwrap(), generate(7, &p, &c).unwrap());
        assert_ne!(generate(7, &p, &c).unwrap(), generate(8, &p, &c).unwrap());
    }

    #[test]
    fn land_share_matches_request_and_planes_differ() {
        let (c, p) = (content(), GenParams::default());
        let map = generate(1, &p, &c).unwrap();
        for plane in Plane::ALL {
            let pct = land_percent(map.plane(plane), &c.terrains);
            assert!((p.land_percent as u32).abs_diff(pct) <= 2, "land {pct}%");
        }
        assert_ne!(map.plane(Plane::Arcanus), map.plane(Plane::Myrror));
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
        no_tower.features = crate::content::Features::new(
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
    fn handles_non_square_and_odd_sizes() {
        let c = content();
        for (w, h) in [(8, 8), (61, 37), (120, 20), (20, 120)] {
            let p = GenParams {
                width: w,
                height: h,
                land_percent: 40,
                ..GenParams::default()
            };
            let map = generate(3, &p, &c).unwrap();
            assert_eq!((map.width(), map.height()), (w, h));
        }
    }
}
