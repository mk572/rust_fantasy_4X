//! Places feature overlays (towers, nodes, encounter sites, minerals) on a
//! generated map. Order matters: towers first (they convert water to grass and
//! constrain nodes), then nodes, encounter sites, and finally minerals.
//!
//! Tests: below, on maps made from the shipped content.

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::content::Content;
use crate::feature::def::FeatureKind;
use crate::map::mapgen::{GenError, GenParams};
use crate::map::terrain::TerrainClass;
use crate::map::{FeatureId, Grid, Map, Plane};

/// Minimum distance between towers (when the map has room for it).
const TOWER_SPACING: u32 = 10;
/// Nodes keep at least this distance from towers and from each other.
const NODE_SPACING: u32 = 4;
/// Random sites keep this far from the map's top and bottom edges.
const SITE_MARGIN: u32 = 2;
/// Random sites keep at least this distance from every other site.
const SITE_SPACING: u32 = 2;
/// Towers and nodes stay away from the polar rows.
const POLE_MARGIN: u32 = 3;

/// Every feature placed on the map so far, one layer per plane. While a map is made minerals,
/// towers, nodes and encounter sites are all placed here; the minerals then go into the map's
/// layer and the others become entities (`Placement`).
#[derive(Debug, Clone)]
pub struct Placed {
    width: u32,
    height: u32,
    layers: [Vec<Option<FeatureId>>; 2],
}

impl Placed {
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;
        Self {
            width,
            height,
            layers: [vec![None; len], vec![None; len]],
        }
    }

    /// The feature on `(x, y)` (x wraps, y outside the map yields `None`).
    pub fn get(&self, plane: Plane, x: i32, y: i32) -> Option<FeatureId> {
        if y < 0 || y >= self.height as i32 {
            return None;
        }
        let x = x.rem_euclid(self.width as i32) as u32;
        self.layers[plane.index()][(y as u32 * self.width + x) as usize]
    }

    pub(crate) fn set(&mut self, plane: Plane, x: u32, y: u32, feature: FeatureId) {
        self.layers[plane.index()][(y * self.width + x) as usize] = Some(feature);
    }
}

pub fn place_features(
    seed: u64,
    params: &GenParams,
    content: &Content,
    map: &mut Map,
) -> Result<Placed, GenError> {
    let mut placed = Placed::new(map.width(), map.height());
    let towers = place_towers(seed, params, content, map, &mut placed)?;
    let mut rngs = Vec::new();
    // Every kind of node gets the same share of the map's nodes (10 of 30 at the standard size),
    // so no realm's magic is scarce: the kind of a node still follows its terrain, among the
    // kinds that have some left.
    let kinds: Vec<FeatureId> = (0..content.features.defs().len() as u16)
        .map(FeatureId)
        .filter(|&f| {
            content
                .features
                .get(f)
                .is_some_and(|d| d.kind == FeatureKind::Node)
        })
        .collect();
    let total: usize = params.planes.iter().map(|p| p.node_count as usize).sum();
    let mut left: Vec<usize> = (0..kinds.len())
        .map(|i| total / kinds.len().max(1) + usize::from(i < total % kinds.len().max(1)))
        .collect();
    for plane in Plane::ALL {
        let salt = 0x51ED_2701_F00D_0000u64 + plane.index() as u64;
        let mut rng = Pcg64Mcg::seed_from_u64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let pp = params.planes[plane.index()];
        let grid = map.plane_mut(plane);

        place_nodes(
            &mut rng,
            plane,
            pp.node_count as usize,
            &mut NodeKinds {
                kinds: &kinds,
                left: &mut left,
                towers: &towers,
            },
            content,
            grid,
            &mut placed,
        );
        rngs.push(rng);
    }
    place_random_sites(seed, params, content, map, &mut placed, &towers);
    for plane in Plane::ALL {
        let mut rng = rngs.remove(0);
        let pp = params.planes[plane.index()];
        let grid = map.plane_mut(plane);
        let mineral_tiles = candidates(grid, &placed, plane, content, |c| {
            !content
                .features
                .weighted(FeatureKind::Mineral, c, plane)
                .is_empty()
        })
        .len();
        let n = mineral_tiles * pp.mineral_percent as usize / 100;
        place_weighted(
            &mut rng,
            plane,
            FeatureKind::Mineral,
            n,
            content,
            grid,
            &mut placed,
        );
    }
    Ok(placed)
}

/// The random encounter sites of the original's `Generate_Lairs`: a fixed number (25 strong and
/// 32 weak, told apart when the lairs are made) on land of either plane, a plane and a square
/// rolled for each, none within one square of a tower, node or other site on either plane and
/// none within 2 squares of the map's top or bottom.
fn place_random_sites(
    seed: u64,
    params: &GenParams,
    content: &Content,
    map: &mut Map,
    placed_features: &mut Placed,
    towers: &[(u32, u32)],
) {
    let want = (params.strong_sites + params.weak_sites) as usize;
    let (w, h) = (map.width(), map.height());
    if want == 0 || h <= 2 * SITE_MARGIN {
        return;
    }
    let mut rng = Pcg64Mcg::seed_from_u64(seed ^ 0x517E_5EED_0000_0003);
    let mut taken: Vec<(u32, u32)> = towers.to_vec();
    for plane in Plane::ALL {
        for y in 0..h {
            for x in 0..w {
                let is_node = placed_features
                    .get(plane, x as i32, y as i32)
                    .and_then(|f| content.features.get(f))
                    .is_some_and(|d| d.kind == FeatureKind::Node);
                if is_node {
                    taken.push((x, y));
                }
            }
        }
    }
    let (mut placed, mut tries) = (0, 0);
    while placed < want && tries < want * 100 {
        tries += 1;
        let plane = Plane::ALL[rng.gen_range(0..2)];
        let (x, y) = (
            rng.gen_range(SITE_MARGIN..w - SITE_MARGIN),
            rng.gen_range(SITE_MARGIN..h - SITE_MARGIN),
        );
        let grid = map.plane(plane);
        if placed_features.get(plane, x as i32, y as i32).is_some()
            || taken
                .iter()
                .any(|&o| grid.chebyshev(o, (x, y)) < SITE_SPACING)
        {
            continue;
        }
        let Some(class) = grid
            .get(x as i32, y as i32)
            .and_then(|t| content.terrains.get(t))
            .map(|d| d.class)
            .filter(|&c| c != TerrainClass::Ocean)
        else {
            continue;
        };
        let Some(id) = pick(
            &mut rng,
            &content
                .features
                .weighted(FeatureKind::Encounter, class, plane),
        ) else {
            continue;
        };
        placed_features.set(plane, x, y, id);
        taken.push((x, y));
        placed += 1;
    }
}

/// Towers share coordinates across planes; water under a tower becomes grass.
fn place_towers(
    seed: u64,
    params: &GenParams,
    content: &Content,
    map: &mut Map,
    placed: &mut Placed,
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
            placed.set(plane, x, y, tower);
        }
    }
    Ok(spots)
}

/// The kinds of node, how many of each are still to place on the map, and the tower squares.
struct NodeKinds<'a> {
    kinds: &'a [FeatureId],
    left: &'a mut [usize],
    towers: &'a [(u32, u32)],
}

fn place_nodes(
    rng: &mut Pcg64Mcg,
    plane: Plane,
    count: usize,
    NodeKinds {
        kinds,
        left,
        towers,
    }: &mut NodeKinds,
    content: &Content,
    grid: &mut Grid,
    layer: &mut Placed,
) {
    let mut placed: Vec<(u32, u32)> = Vec::new();
    // The first plane takes no more than its even share of a kind, which leaves the second room
    // for what is left of every kind.
    let share = if plane == Plane::Arcanus {
        count.div_ceil(kinds.len().max(1))
    } else {
        usize::MAX
    };
    let mut room: Vec<usize> = vec![share; kinds.len()];
    let mut spots = candidates(grid, layer, plane, content, |_| true);
    shuffle(rng, &mut spots);
    // Two rounds over the spots: the second takes any kind that has some left, for the nodes
    // the terrain has no room for in the first.
    let again = spots.clone();
    for (round, (x, y, class)) in spots
        .into_iter()
        .map(|s| (0, s))
        .chain(again.into_iter().map(|s| (1, s)))
    {
        if placed.len() >= count {
            break;
        }
        if round == 1 && layer.get(plane, x as i32, y as i32).is_some() {
            continue;
        }
        let far_enough = |o: &(u32, u32)| grid.distance_sq(*o, (x, y)) >= NODE_SPACING.pow(2);
        if y < POLE_MARGIN
            || y >= grid.height() - POLE_MARGIN
            || !towers.iter().all(far_enough)
            || !placed.iter().all(far_enough)
        {
            continue;
        }
        let weighted = content.features.weighted(FeatureKind::Node, class, plane);
        let weighted = if round == 1 {
            kinds.iter().map(|&k| (k, 1)).collect()
        } else {
            weighted
        };
        let options: Vec<(FeatureId, u32)> = weighted
            .into_iter()
            .filter(|(f, _)| {
                kinds
                    .iter()
                    .position(|k| k == f)
                    .is_some_and(|k| left[k] > 0 && room[k] > 0)
            })
            .collect();
        if let Some(id) = pick(rng, &options) {
            if let Some(k) = kinds.iter().position(|&k| k == id) {
                left[k] -= 1;
                room[k] -= 1;
            }
            layer.set(plane, x, y, id);
            put_node_terrain(content, plane, id, x, y, grid);
            placed.push((x, y));
        }
    }
}

/// A node's own square takes the node's base terrain (the original sets `tt_SorceryNode`,
/// `tt_NatureNode` or `tt_ChaosNode` there, which also replaces a river tile). Its aura
/// squares keep what they are.
fn put_node_terrain(
    content: &Content,
    plane: Plane,
    node: FeatureId,
    x: u32,
    y: u32,
    grid: &mut Grid,
) {
    let terrain = content
        .features
        .get(node)
        .and_then(|d| d.id.strip_suffix("_node"))
        .and_then(TerrainClass::of_node_realm)
        .and_then(|class| content.terrains.first_of_class(class, plane));
    if let Some(terrain) = terrain {
        grid.set(x, y, terrain);
        grid.set_river(x, y, false);
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
    layer: &mut Placed,
) {
    let mut spots = candidates(grid, layer, plane, content, |_| true);
    shuffle(rng, &mut spots);
    let mut placed = 0;
    for (x, y, class) in spots {
        if placed >= count {
            break;
        }
        if let Some(id) = pick(rng, &content.features.weighted(kind, class, plane)) {
            layer.set(plane, x, y, id);
            placed += 1;
        }
    }
}

/// Free (feature-less) land tiles whose terrain class passes `filter`, in scan order.
fn candidates(
    grid: &Grid,
    layer: &Placed,
    plane: Plane,
    content: &Content,
    filter: impl Fn(TerrainClass) -> bool,
) -> Vec<(u32, u32, TerrainClass)> {
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if layer.get(plane, x as i32, y as i32).is_some() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::shipped;
    use crate::map::mapgen::{generate, Generated};
    use crate::testdir::TestResult;

    fn made(seed: u64) -> Result<(Generated, GenParams, Content), GenError> {
        let (c, p) = (shipped(), GenParams::default());
        Ok((generate(seed, &p, &c)?, p, c))
    }

    fn of_kind<'a>(
        g: &'a Generated,
        c: &'a Content,
        kind: FeatureKind,
    ) -> impl Iterator<Item = &'a crate::feature::entity::Placement> {
        g.placements
            .iter()
            .filter(move |p| c.features.get(p.feature).is_some_and(|d| d.kind == kind))
    }

    #[test]
    fn a_map_gets_the_requested_towers_apart_from_each_other_and_on_land() -> TestResult {
        let (g, p, c) = made(5)?;
        let towers: Vec<_> = of_kind(&g, &c, FeatureKind::Tower).collect();
        assert_eq!(towers.len(), p.tower_count as usize);
        let arcanus = g.map.plane(Plane::Arcanus);
        for (i, a) in towers.iter().enumerate() {
            let at = (a.at.x, a.at.y);
            assert!((POLE_MARGIN..p.height - POLE_MARGIN).contains(&a.at.y));
            for plane in Plane::ALL {
                let class = g
                    .map
                    .plane(plane)
                    .get(a.at.x as i32, a.at.y as i32)
                    .and_then(|t| c.terrains.get(t))
                    .map(|d| d.class);
                assert_ne!(class, Some(TerrainClass::Ocean), "a tower stands on land");
            }
            for b in &towers[i + 1..] {
                assert!(arcanus.distance_sq(at, (b.at.x, b.at.y)) >= TOWER_SPACING.pow(2));
            }
        }
        Ok(())
    }

    #[test]
    fn nodes_keep_their_distance_and_every_kind_gets_a_share() -> TestResult {
        let (g, p, c) = made(5)?;
        let nodes: Vec<_> = of_kind(&g, &c, FeatureKind::Node).collect();
        let wanted: usize = p.planes.iter().map(|q| q.node_count as usize).sum();
        assert!(
            nodes.len() + 3 >= wanted && nodes.len() <= wanted,
            "{} nodes of {wanted}",
            nodes.len()
        );
        let towers: Vec<_> = of_kind(&g, &c, FeatureKind::Tower).collect();
        let grid = g.map.plane(Plane::Arcanus);
        for (i, a) in nodes.iter().enumerate() {
            assert!((POLE_MARGIN..p.height - POLE_MARGIN).contains(&a.at.y));
            let near = |b: &&crate::feature::entity::Placement| {
                grid.distance_sq((a.at.x, a.at.y), (b.at.x, b.at.y)) < NODE_SPACING.pow(2)
            };
            assert!(!towers.iter().any(near), "a node is clear of the towers");
            assert!(
                !nodes[i + 1..]
                    .iter()
                    .filter(|b| b.at.plane == a.at.plane)
                    .any(near),
                "and of the other nodes of its plane"
            );
        }
        let mut per_kind = std::collections::BTreeMap::new();
        for n in &nodes {
            *per_kind.entry(n.feature.0).or_insert(0usize) += 1;
        }
        let (least, most) = (
            per_kind.values().min().copied().unwrap_or(0),
            per_kind.values().max().copied().unwrap_or(0),
        );
        assert!(
            most - least <= 3,
            "no realm's magic is scarce: {per_kind:?}"
        );
        Ok(())
    }

    #[test]
    fn encounter_sites_keep_off_the_margins_and_two_squares_apart() -> TestResult {
        let (g, p, c) = made(5)?;
        let sites: Vec<_> = of_kind(&g, &c, FeatureKind::Encounter).collect();
        let wanted = (p.strong_sites + p.weak_sites) as usize;
        assert!(
            sites.len() >= wanted * 9 / 10,
            "{} sites of {wanted}",
            sites.len()
        );
        let grid = g.map.plane(Plane::Arcanus);
        for (i, a) in sites.iter().enumerate() {
            assert!((SITE_MARGIN..p.height - SITE_MARGIN).contains(&a.at.y));
            assert!(
                !sites[i + 1..].iter().any(|b| {
                    a.at.plane == b.at.plane
                        && grid.chebyshev((a.at.x, a.at.y), (b.at.x, b.at.y)) < SITE_SPACING
                }),
                "{:?} is crowded",
                a.at
            );
        }
        Ok(())
    }

    #[test]
    fn minerals_lie_on_both_planes_and_never_under_a_feature() -> TestResult {
        let (g, ..) = made(5)?;
        for plane in Plane::ALL {
            let grid = g.map.plane(plane);
            let minerals = (0..grid.height() as i32)
                .flat_map(|y| (0..grid.width() as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| grid.mineral(x, y).is_some())
                .count();
            assert!(minerals > 0, "{plane:?} has minerals");
            // A feature square holds no mineral.
            for pl in g.placements.iter().filter(|pl| pl.at.plane == plane) {
                assert!(grid.mineral(pl.at.x as i32, pl.at.y as i32).is_none());
            }
        }
        Ok(())
    }

    #[test]
    fn no_towers_and_no_sites_when_none_are_asked_for() -> TestResult {
        let (c, mut p) = (shipped(), GenParams::default());
        p.tower_count = 0;
        p.strong_sites = 0;
        p.weak_sites = 0;
        let g = generate(5, &p, &c)?;
        assert_eq!(of_kind(&g, &c, FeatureKind::Tower).count(), 0);
        assert_eq!(of_kind(&g, &c, FeatureKind::Encounter).count(), 0);
        Ok(())
    }

    #[test]
    fn a_placed_layer_reads_none_off_the_map_and_wraps_east_and_west() {
        let mut placed = Placed::new(10, 6);
        placed.set(Plane::Arcanus, 9, 2, FeatureId(4));
        assert_eq!(placed.get(Plane::Arcanus, 9, 2), Some(FeatureId(4)));
        assert_eq!(placed.get(Plane::Arcanus, -1, 2), Some(FeatureId(4)));
        assert_eq!(placed.get(Plane::Arcanus, 19, 2), Some(FeatureId(4)));
        assert_eq!(placed.get(Plane::Myrror, 9, 2), None, "the other plane");
        assert_eq!(placed.get(Plane::Arcanus, 9, -1), None);
        assert_eq!(placed.get(Plane::Arcanus, 9, 6), None);
    }

    #[test]
    fn a_map_without_a_tower_kind_is_refused() {
        let (mut c, p) = (shipped(), GenParams::default());
        let defs: Vec<_> = c
            .features
            .defs()
            .iter()
            .filter(|d| d.kind != FeatureKind::Tower)
            .cloned()
            .collect();
        c.features = crate::feature::def::Features::new(defs);
        assert_eq!(
            generate(5, &p, &c).err(),
            Some(GenError::MissingFeature {
                kind: FeatureKind::Tower
            })
        );
    }

    #[test]
    fn the_same_seed_places_the_same_features() -> TestResult {
        let (a, ..) = made(9)?;
        let (b, ..) = made(9)?;
        assert_eq!(a.placements, b.placements);
        let (other, ..) = made(10)?;
        assert_ne!(a.placements, other.placements);
        Ok(())
    }
}
