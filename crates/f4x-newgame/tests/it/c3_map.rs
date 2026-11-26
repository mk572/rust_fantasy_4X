//! Class 3 (`docs/f4x-newgame-test-design.md`): the generated world keeps its own placement rules.
//!
//! `map_problems` reads a finished map and its placements through public types and the content,
//! and re-derives each rule from the terrain classes and the feature kinds. It calls nothing of
//! the placement code. It returns every violation as `rule: what`, so one test runs it on every
//! world of the matrix and a failure names the world and the rule; the negative tests break a
//! finished world in one way each and show that the matching rule fires.

use crate::support::*;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_def::content::Content;
use f4x_def::feature::FeatureKind;
use f4x_newgame::mapgen::{GenParams, Generated};
use f4x_newgame::place::{NODE_SPACING, POLE_MARGIN, SITE_MARGIN, SITE_SPACING, TOWER_SPACING};

/// Everything a placement says about the square it is on.
struct Site<'a> {
    at: Position,
    kind: FeatureKind,
    p: &'a f4x_core::feature::entity::Placement,
}

fn class_at(g: &Generated, c: &Content, plane: Plane, x: u32, y: u32) -> Option<TerrainClass> {
    let id = g.map.plane(plane).get(x as i32, y as i32)?;
    Some(c.terrains.get(id)?.class)
}

/// Every rule the map and its placements break, as `rule: what`. `count_rules` also compares the
/// numbers of towers, nodes and sites with what the parameters ask for.
pub fn map_problems(g: &Generated, c: &Content, p: &GenParams, count_rules: bool) -> Vec<String> {
    let mut bad = Vec::new();
    let (width, h) = (g.map.width(), g.map.height());
    let mut say = |rule: &str, what: String| bad.push(format!("{rule}: {what}"));

    if (width, h) != (p.width, p.height) {
        say("size", format!("{width}x{h} for {}x{}", p.width, p.height));
    }
    for plane in Plane::all() {
        let grid = g.map.plane(plane);
        if (grid.width(), grid.height()) != (width, h) {
            say("size", format!("{plane:?} has another size"));
        }
        for y in 0..h {
            for x in 0..width {
                if class_at(g, c, plane, x, y).is_none() {
                    say("terrain", format!("{plane:?} ({x},{y}) is no terrain"));
                }
            }
        }
    }

    let sites: Vec<Site> = g
        .placements
        .iter()
        .filter_map(|pl| {
            let kind = c.features.get(pl.feature)?.kind;
            Some(Site {
                at: pl.at,
                kind,
                p: pl,
            })
        })
        .collect();
    if sites.len() != g.placements.len() {
        say(
            "feature",
            "a placement names a feature that is not there".into(),
        );
    }
    // Land is added after the placing by the island setting, and by the original's own islands.
    let grown = p.extend_tiny_islands || p.original_generator;
    let of_kind = |k: FeatureKind| sites.iter().filter(move |s| s.kind == k);

    // Every placement is on the map, on land, and not on another placement.
    let mut seen = std::collections::BTreeSet::new();
    for s in &sites {
        let (plane, x, y) = (s.at.plane, s.at.x, s.at.y);
        if x >= width || y >= h {
            say("on-map", format!("{:?} at {x},{y}", s.kind));
            continue;
        }
        if class_at(g, c, plane, x, y) == Some(TerrainClass::Ocean) {
            say("on-land", format!("{:?} at {plane:?} {x},{y}", s.kind));
        }
        if !seen.insert((plane.index(), x, y)) {
            say(
                "one-per-square",
                format!("two features at {plane:?} {x},{y}"),
            );
        }
        if s.kind == FeatureKind::Tower && plane != Plane::Arcanus {
            say("tower-once", format!("a tower entity on Myrror at {x},{y}"));
        }
        // A tower stands on both planes: its Myrror square is land as well.
        if s.kind == FeatureKind::Tower
            && class_at(g, c, Plane::Myrror, x, y) == Some(TerrainClass::Ocean)
        {
            say(
                "on-land",
                format!("tower at {x},{y} on Myrror is in the sea"),
            );
        }
    }

    // Towers: apart from each other, away from the poles.
    let towers: Vec<(u32, u32)> = of_kind(FeatureKind::Tower)
        .map(|s| (s.at.x, s.at.y))
        .collect();
    for (i, &a) in towers.iter().enumerate() {
        if a.1 < POLE_MARGIN || a.1 >= h.saturating_sub(POLE_MARGIN) {
            say("tower-pole", format!("tower at {a:?} is in the polar rows"));
        }
        for &b in &towers[i + 1..] {
            if dist_sq(width, a, b) < TOWER_SPACING * TOWER_SPACING {
                say("tower-spacing", format!("towers at {a:?} and {b:?}"));
            }
        }
    }
    if count_rules {
        let want = usize::from(p.tower_count);
        if towers.len() > want {
            say("tower-count", format!("{} towers for {want}", towers.len()));
        }
        // Fewer is right only when the map has no square left that is far enough from the others.
        if towers.len() < want {
            let free = (POLE_MARGIN..h.saturating_sub(POLE_MARGIN))
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .find(|&sq| {
                    towers
                        .iter()
                        .all(|&t| dist_sq(width, t, sq) >= TOWER_SPACING * TOWER_SPACING)
                });
            if let Some(sq) = free {
                say(
                    "tower-count",
                    format!("{} towers for {want}, and {sq:?} was free", towers.len()),
                );
            }
        }
    }

    // Nodes: the square's own terrain, no river, apart from towers and each other, off the poles;
    // each has a node record whose aura lies on the map.
    for s in of_kind(FeatureKind::Node) {
        let (plane, x, y) = (s.at.plane, s.at.x, s.at.y);
        let def = c.features.get(s.p.feature).expect("a node feature");
        let want = def.realm.and_then(TerrainClass::of_node_realm);
        if want.is_none() || class_at(g, c, plane, x, y) != want {
            say(
                "node-terrain",
                format!(
                    "node at {plane:?} {x},{y} stands on {:?}",
                    class_at(g, c, plane, x, y)
                ),
            );
        }
        if g.map.plane(plane).has_river(x as i32, y as i32) {
            say("node-river", format!("a river under the node at {x},{y}"));
        }
        if y < POLE_MARGIN || y >= h.saturating_sub(POLE_MARGIN) {
            say("node-pole", format!("node at {x},{y}"));
        }
        for &t in &towers {
            if dist_sq(width, t, (x, y)) < NODE_SPACING * NODE_SPACING {
                say("node-spacing", format!("node {x},{y} near the tower {t:?}"));
            }
        }
        for o in of_kind(FeatureKind::Node) {
            if o.at != s.at
                && o.at.plane == plane
                && dist_sq(width, (o.at.x, o.at.y), (x, y)) < NODE_SPACING * NODE_SPACING
            {
                say(
                    "node-spacing",
                    format!("nodes {x},{y} and {},{}", o.at.x, o.at.y),
                );
            }
        }
        let Some(node) = &s.p.node else {
            say("node-record", format!("no node record at {x},{y}"));
            continue;
        };
        if Some(node.realm) != def.realm {
            say(
                "node-record",
                format!("the realm of the node at {x},{y} differs"),
            );
        }
        if node.power == 0 {
            say("node-power", format!("node at {x},{y} has no power"));
        }
        let mut aura = node.aura.clone();
        let squares = aura.len();
        aura.sort_unstable();
        aura.dedup();
        if aura.len() != squares {
            say("aura", format!("a square twice in the aura of {x},{y}"));
        }
        if !aura.contains(&(x, y)) {
            say("aura", format!("the aura of {x},{y} lacks its own square"));
        }
        if squares > usize::from(node.power) {
            say(
                "aura",
                format!("{squares} squares for power {}", node.power),
            );
        }
        for &(ax, ay) in &aura {
            if ax >= width
                || ay >= h
                || chebyshev(width, (ax, ay), (x, y)) > f4x_newgame::node::AURA_REACH as u32
            {
                say("aura", format!("the aura of {x},{y} reaches {ax},{ay}"));
            }
        }
    }
    if count_rules {
        for plane in Plane::all() {
            let n = of_kind(FeatureKind::Node)
                .filter(|s| s.at.plane == plane)
                .count();
            let want = usize::from(p.planes[plane.index()].node_count);
            if n > want {
                say("node-count", format!("{n} nodes on {plane:?} for {want}"));
            }
            // Fewer is right only when the land has no square left where one more could stand.
            // (Not asked where an island is built out after the placing: the land that was added
            // was sea when the nodes were placed.)
            if n < want && !grown {
                if let Some((x, y)) = free_node_square(g, c, plane, &sites) {
                    say(
                        "node-count",
                        format!("{n} nodes on {plane:?} for {want}, and {x},{y} was free"),
                    );
                }
            }
        }
    }

    // Encounter sites: off the margins, apart from everything else that is a site.
    let every: Vec<&Site> = sites
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                FeatureKind::Tower | FeatureKind::Node | FeatureKind::Encounter
            )
        })
        .collect();
    for s in of_kind(FeatureKind::Encounter) {
        let at = (s.at.x, s.at.y);
        if at.1 < SITE_MARGIN || at.1 >= h.saturating_sub(SITE_MARGIN) {
            say("site-margin", format!("site at {at:?}"));
        }
        for o in &every {
            // Apart from every tower, node and site of either plane, the one square counted once.
            if o.at != s.at && chebyshev(width, (o.at.x, o.at.y), at) < SITE_SPACING {
                say("site-spacing", format!("site {at:?} near {:?}", o.kind));
            }
        }
        if s.p.lair.is_none() {
            say("lair", format!("the site at {at:?} has no lair"));
        }
    }
    if count_rules {
        let n = of_kind(FeatureKind::Encounter).count();
        let want = (p.strong_sites + p.weak_sites) as usize;
        if n > want {
            say("site-count", format!("{n} sites for {want}"));
        }
        if n < want && !grown {
            if let Some((plane, x, y)) = free_site_square(g, c, &sites) {
                say(
                    "site-count",
                    format!("{n} sites for {want}, and {plane:?} {x},{y} was free"),
                );
            }
        }
    }

    // Every tower, node and site has its lair; nothing else has one; nothing else has a node record.
    for s in &sites {
        let has_lair = s.p.lair.is_some();
        let needs = matches!(
            s.kind,
            FeatureKind::Tower | FeatureKind::Node | FeatureKind::Encounter
        );
        if has_lair != needs {
            say(
                "lair",
                format!("{:?} at {:?}: lair is {has_lair}", s.kind, s.at),
            );
        }
        if s.p.node.is_some() != (s.kind == FeatureKind::Node) {
            say("node-record", format!("{:?} at {:?}", s.kind, s.at));
        }
    }

    // Minerals: on land, of a kind that is allowed there, never under a placement.
    for plane in Plane::all() {
        let grid = g.map.plane(plane);
        for y in 0..h {
            for x in 0..width {
                let Some(m) = grid.mineral(x as i32, y as i32) else {
                    continue;
                };
                let Some(def) = c.features.get(m) else {
                    say("mineral", format!("{plane:?} {x},{y} names no feature"));
                    continue;
                };
                if def.kind != FeatureKind::Mineral {
                    say("mineral", format!("{:?} is in the mineral layer", def.kind));
                }
                match class_at(g, c, plane, x, y) {
                    Some(TerrainClass::Ocean) | None => {
                        say("mineral-land", format!("{plane:?} {x},{y} in the sea"))
                    }
                    Some(class) => {
                        if !c
                            .features
                            .weighted(FeatureKind::Mineral, class, plane)
                            .iter()
                            .any(|(f, _)| *f == m)
                        {
                            say(
                                "mineral-allowed",
                                format!("{} on {class:?} of {plane:?}", def.id),
                            );
                        }
                    }
                }
                let under = sites.iter().any(|s| {
                    (s.at.x, s.at.y) == (x, y)
                        && (s.at.plane == plane || s.kind == FeatureKind::Tower)
                });
                if under {
                    say(
                        "mineral-under",
                        format!("{plane:?} {x},{y} is under a feature"),
                    );
                }
            }
        }
    }

    // Rivers run on land or cross a lake, never on open sea; a lake touches no sea (only land, or another lake); the poles are
    // tundra or sea.
    for plane in Plane::all() {
        let grid = g.map.plane(plane);
        for y in 0..h {
            for x in 0..width {
                let (xi, yi) = (x as i32, y as i32);
                if grid.has_river(xi, yi)
                    && class_at(g, c, plane, x, y) == Some(TerrainClass::Ocean)
                    && !grid.is_lake(xi, yi)
                {
                    say("river", format!("{plane:?} {x},{y} runs on open sea"));
                }
                if grid.is_lake(xi, yi) {
                    for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let n = grid.get(grid.wrap_x(xi + ox) as i32, yi + oy);
                        let water = n
                            .and_then(|t| c.terrains.get(t))
                            .is_some_and(|d| d.class == TerrainClass::Ocean)
                            && !grid.is_lake(grid.wrap_x(xi + ox) as i32, yi + oy);
                        if water {
                            say("lake", format!("{plane:?} {x},{y} touches the sea"));
                        }
                    }
                }
            }
        }
        // The original generator's desert walks may paint a pole row (ReMoM `MAPGEN.c`,
        // `Generate_Climate_Terrain_Types`: a walk of up to ten steps from a square eight or more rows
        // in, with nothing to stop it at the pole). The rule is the noise generator's.
        for y in [0, h - 1].into_iter().filter(|_| !p.original_generator) {
            for x in 0..width {
                let class = class_at(g, c, plane, x, y);
                if !matches!(class, Some(TerrainClass::Tundra | TerrainClass::Ocean)) {
                    say("pole", format!("{plane:?} {x},{y} is {class:?}"));
                }
            }
        }
    }
    bad
}

/// A square where one more node could stand on `plane`, by the placement rules: land, off the
/// polar rows, apart from the towers and the nodes of the plane, and not under another feature
/// (an encounter site counts as free: sites are placed after the nodes).
fn free_node_square(
    g: &Generated,
    c: &Content,
    plane: Plane,
    sites: &[Site],
) -> Option<(u32, u32)> {
    let (width, h) = (g.map.width(), g.map.height());
    for y in POLE_MARGIN..h.saturating_sub(POLE_MARGIN) {
        for x in 0..width {
            if matches!(
                class_at(g, c, plane, x, y),
                Some(TerrainClass::Ocean) | None
            ) {
                continue;
            }
            let blocked = sites.iter().any(|s| {
                let on_it = (s.at.x, s.at.y) == (x, y)
                    && (s.at.plane == plane || s.kind == FeatureKind::Tower);
                let too_near = (s.kind == FeatureKind::Tower
                    || (s.kind == FeatureKind::Node && s.at.plane == plane))
                    && dist_sq(width, (s.at.x, s.at.y), (x, y)) < NODE_SPACING * NODE_SPACING;
                (on_it && s.kind != FeatureKind::Encounter) || too_near
            });
            if !blocked {
                return Some((x, y));
            }
        }
    }
    None
}

/// A square where one more encounter site could stand, by the rules: land that some site kind is
/// allowed on, off the margins, apart from every tower, node and site of either plane.
fn free_site_square(g: &Generated, c: &Content, sites: &[Site]) -> Option<(Plane, u32, u32)> {
    let (width, h) = (g.map.width(), g.map.height());
    for plane in Plane::all() {
        for y in SITE_MARGIN..h.saturating_sub(SITE_MARGIN) {
            for x in 0..width {
                let Some(class) = class_at(g, c, plane, x, y).filter(|&k| k != TerrainClass::Ocean)
                else {
                    continue;
                };
                if c.features
                    .weighted(FeatureKind::Encounter, class, plane)
                    .is_empty()
                {
                    continue;
                }
                let blocked = sites.iter().any(|s| {
                    matches!(
                        s.kind,
                        FeatureKind::Tower | FeatureKind::Node | FeatureKind::Encounter
                    ) && chebyshev(width, (s.at.x, s.at.y), (x, y)) < SITE_SPACING
                });
                let taken = sites.iter().any(|s| {
                    (s.at.x, s.at.y) == (x, y)
                        && (s.at.plane == plane || s.kind == FeatureKind::Tower)
                });
                if !blocked && !taken {
                    return Some((plane, x, y));
                }
            }
        }
    }
    None
}

fn tag(problems: &[String], rule: &str) -> bool {
    problems.iter().any(|p| p.starts_with(&format!("{rule}:")))
}

/// What the problems of a world say, with the world, so a failure can be repeated.
fn report(case: WorldCase, problems: &[String]) -> String {
    format!("{case:?}\n  {}", problems.join("\n  "))
}

#[test]
fn every_world_of_the_matrix_keeps_the_placement_rules() {
    let mut failures = Vec::new();
    for case in worlds() {
        let g = generated(case);
        let problems = map_problems(&g, content(), &params(case), true);
        if !problems.is_empty() {
            failures.push(report(case, &problems));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn standard_map() -> Generated {
    (*generated(standard())).clone()
}

fn problems_of(g: &Generated) -> Vec<String> {
    map_problems(g, content(), &params(standard()), true)
}

#[test]
fn the_standard_world_has_no_problem_to_start_from() {
    assert_eq!(problems_of(&standard_map()), Vec::<String>::new());
}

/// The first placement of this kind.
fn first(g: &Generated, kind: FeatureKind) -> usize {
    g.placements
        .iter()
        .position(|p| {
            content()
                .features
                .get(p.feature)
                .is_some_and(|d| d.kind == kind)
        })
        .unwrap_or_else(|| panic!("the standard map has no {kind:?}"))
}

fn ocean(g: &Generated, plane: Plane) -> (u32, u32) {
    let grid = g.map.plane(plane);
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if class_at(g, content(), plane, x, y) == Some(TerrainClass::Ocean) {
                return (x, y);
            }
        }
    }
    panic!("no sea on {plane:?}");
}

#[test]
fn a_tower_in_the_sea_is_found() {
    let mut g = standard_map();
    let i = first(&g, FeatureKind::Tower);
    let (x, y) = ocean(&g, Plane::Arcanus);
    g.placements[i].at = Position {
        plane: Plane::Arcanus,
        x,
        y,
    };
    assert!(tag(&problems_of(&g), "on-land"), "{:?}", problems_of(&g));
}

#[test]
fn two_towers_side_by_side_are_found() {
    let mut g = standard_map();
    let t: Vec<usize> = (0..g.placements.len())
        .filter(|&i| {
            content()
                .features
                .get(g.placements[i].feature)
                .is_some_and(|d| d.kind == FeatureKind::Tower)
        })
        .collect();
    let at = g.placements[t[0]].at;
    g.placements[t[1]].at = Position { x: at.x + 1, ..at };
    assert!(tag(&problems_of(&g), "tower-spacing"));
}

#[test]
fn a_tower_in_the_polar_rows_is_found() {
    let mut g = standard_map();
    let i = first(&g, FeatureKind::Tower);
    g.placements[i].at.y = 0;
    assert!(tag(&problems_of(&g), "tower-pole"));
}

#[test]
fn a_node_on_a_tower_is_found() {
    let mut g = standard_map();
    let (n, t) = (first(&g, FeatureKind::Node), first(&g, FeatureKind::Tower));
    let at = g.placements[t].at;
    g.placements[n].at = Position {
        plane: g.placements[n].at.plane,
        ..at
    };
    assert!(tag(&problems_of(&g), "node-spacing"));
}

#[test]
fn a_node_on_the_wrong_ground_is_found() {
    let mut g = standard_map();
    let n = first(&g, FeatureKind::Node);
    let at = g.placements[n].at;
    // Another node kind's terrain under it.
    let other = content()
        .terrains
        .first_of_class(TerrainClass::Grassland, at.plane)
        .expect("grassland");
    g.map.plane_mut(at.plane).set(at.x, at.y, other);
    assert!(tag(&problems_of(&g), "node-terrain"));
}

#[test]
fn a_node_without_its_own_square_in_the_aura_is_found() {
    let mut g = standard_map();
    let n = first(&g, FeatureKind::Node);
    let own = (g.placements[n].at.x, g.placements[n].at.y);
    let node = g.placements[n].node.as_mut().expect("a node record");
    node.aura.retain(|&s| s != own);
    assert!(tag(&problems_of(&g), "aura"));
}

#[test]
fn an_aura_off_the_map_is_found() {
    let mut g = standard_map();
    let n = first(&g, FeatureKind::Node);
    let h = g.map.height();
    g.placements[n]
        .node
        .as_mut()
        .expect("a node record")
        .aura
        .push((0, h + 5));
    assert!(tag(&problems_of(&g), "aura"));
}

#[test]
fn a_site_on_the_margin_is_found() {
    let mut g = standard_map();
    let s = first(&g, FeatureKind::Encounter);
    g.placements[s].at.y = 0;
    assert!(tag(&problems_of(&g), "site-margin"));
}

#[test]
fn a_site_without_a_lair_is_found() {
    let mut g = standard_map();
    let s = first(&g, FeatureKind::Encounter);
    g.placements[s].lair = None;
    assert!(tag(&problems_of(&g), "lair"));
}

#[test]
fn a_mineral_under_a_feature_is_found() {
    let mut g = standard_map();
    let n = first(&g, FeatureKind::Node);
    let at = g.placements[n].at;
    let mineral = (0..content().features.defs().len() as u16)
        .map(f4x_core::map::FeatureId)
        .find(|&f| {
            content()
                .features
                .get(f)
                .is_some_and(|d| d.kind == FeatureKind::Mineral)
        })
        .expect("a mineral kind");
    g.map
        .plane_mut(at.plane)
        .set_mineral(at.x, at.y, Some(mineral));
    assert!(tag(&problems_of(&g), "mineral-under"));
}

#[test]
fn a_mineral_in_the_sea_is_found() {
    let mut g = standard_map();
    let (x, y) = ocean(&g, Plane::Arcanus);
    let mineral = (0..content().features.defs().len() as u16)
        .map(f4x_core::map::FeatureId)
        .find(|&f| {
            content()
                .features
                .get(f)
                .is_some_and(|d| d.kind == FeatureKind::Mineral)
        })
        .expect("a mineral kind");
    g.map
        .plane_mut(Plane::Arcanus)
        .set_mineral(x, y, Some(mineral));
    assert!(tag(&problems_of(&g), "mineral-land"));
}

#[test]
fn a_missing_tower_is_found_by_the_count() {
    let mut g = standard_map();
    let i = first(&g, FeatureKind::Tower);
    g.placements.remove(i);
    assert!(tag(&problems_of(&g), "tower-count"));
}

#[test]
fn a_river_in_the_sea_is_found() {
    let mut g = standard_map();
    let (x, y) = ocean(&g, Plane::Myrror);
    g.map.plane_mut(Plane::Myrror).set_river(x, y, true);
    assert!(tag(&problems_of(&g), "river"));
}

#[test]
fn a_pole_of_grass_is_found() {
    let mut g = standard_map();
    let grass = content()
        .terrains
        .first_of_class(TerrainClass::Grassland, Plane::Arcanus)
        .expect("grassland");
    g.map.plane_mut(Plane::Arcanus).set(3, 0, grass);
    assert!(tag(&problems_of(&g), "pole"));
}

// ---- properties of the map over the matrix ----

use f4x_newgame::mapgen::land_percent;

#[test]
fn the_land_is_the_share_asked_for_or_a_little_more() {
    // The noise generator takes the top share of the open sea and adds the polar caps on top; the
    // original's walks stop short by at most a point. Never less than asked, less one.
    let mut failures = Vec::new();
    for case in worlds() {
        let g = generated(case);
        for plane in Plane::all() {
            let got = land_percent(g.map.plane(plane), &content().terrains);
            let asked = u32::from(case.settings().basic.land_percent);
            if got + 1 < asked {
                failures.push(format!("{case:?} {plane:?}: {got}% for {asked}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn more_land_asked_for_is_more_land_made() {
    let share = |variant, land| {
        let case = WorldCase {
            land,
            variant,
            seed: 5,
            ..standard()
        };
        land_percent(
            generated(case).map.plane(Plane::Arcanus),
            &content().terrains,
        )
    };
    // The noise generator: every step up is more land, all the way to 99.
    let noise: Vec<u32> = [10, 30, 60, 90, 99]
        .map(|l| share(Variant::Plain, l))
        .into();
    assert!(noise.windows(2).all(|p| p[0] < p[1]), "{noise:?}");
    // The original's walks, up to the most the settings let it be asked for.
    let most = f4x_def::settings::ORIGINAL_GENERATOR_MAX_LAND;
    let original: Vec<u32> = [10, 20, 30, most]
        .map(|l| share(Variant::OriginalGenerator, l))
        .into();
    assert!(original.windows(2).all(|p| p[0] < p[1]), "{original:?}");
}

#[test]
fn the_two_planes_are_not_the_same_land() {
    for case in worlds()
        .into_iter()
        .filter(|case| case.width * case.height >= 45 * 30)
    {
        let g = generated(case);
        let (a, b) = (g.map.plane(Plane::Arcanus), g.map.plane(Plane::Myrror));
        // Their terrain ids differ by plane, so compare the land and the sea.
        let is_land = |grid: &f4x_core::map::Grid, x, y| {
            class_at_grid(grid, x, y) != Some(TerrainClass::Ocean)
        };
        let same = (0..a.height())
            .flat_map(|y| (0..a.width()).map(move |x| (x, y)))
            .all(|(x, y)| is_land(a, x, y) == is_land(b, x, y));
        assert!(!same, "{case:?}: both planes have the same coast");
    }
}

fn class_at_grid(grid: &f4x_core::map::Grid, x: u32, y: u32) -> Option<TerrainClass> {
    Some(content().terrains.get(grid.get(x as i32, y as i32)?)?.class)
}

#[test]
fn the_minerals_are_the_share_of_the_land_the_settings_ask_for() {
    // Without an island built out afterwards, the number of minerals of a plane is the share of
    // the free land squares a mineral may stand on, rounded down (what the placing could use).
    let c = content();
    for case in worlds().into_iter().filter(|case| {
        !matches!(
            case.variant,
            Variant::OriginalGenerator | Variant::ExtendTinyIslands
        ) && !params(*case).extend_tiny_islands
    }) {
        let g = generated(case);
        let p = params(case);
        for plane in Plane::all() {
            let grid = g.map.plane(plane);
            let minerals = (0..grid.height())
                .flat_map(|y| (0..grid.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| grid.mineral(x as i32, y as i32).is_some())
                .count();
            // The squares a mineral could be on: land that no placement is on, that has a mineral
            // allowed on its class. Squares with a mineral count as free (they were, before).
            let free = (0..grid.height())
                .flat_map(|y| (0..grid.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let class = class_at_grid(grid, x, y);
                    let occupied = g.placements.iter().any(|s| {
                        (s.at.x, s.at.y) == (x, y)
                            && (s.at.plane == plane
                                || c.features
                                    .get(s.feature)
                                    .is_some_and(|d| d.kind == FeatureKind::Tower))
                    });
                    !occupied
                        && class.is_some_and(|k| {
                            k != TerrainClass::Ocean
                                && !c
                                    .features
                                    .weighted(FeatureKind::Mineral, k, plane)
                                    .is_empty()
                        })
                })
                .count();
            let share = usize::from(p.planes[plane.index()].mineral_percent);
            // The share is taken of the squares before the minerals were laid, and a mineral
            // square is among those `free` now: the count is that share of `free`.
            assert_eq!(minerals, free * share / 100, "{case:?} {plane:?}");
        }
    }
}

#[test]
fn every_node_kind_has_its_share_of_the_nodes() {
    // "Every kind of node gets the same share of the map's nodes": the kinds differ by one at
    // most, when the land had room for them all.
    let c = content();
    for case in worlds().into_iter().filter(|case| case.land >= 30) {
        let g = generated(case);
        let p = params(case);
        let want: usize = p.planes.iter().map(|x| usize::from(x.node_count)).sum();
        let mut by_kind = std::collections::BTreeMap::new();
        for s in &g.placements {
            if c.features
                .get(s.feature)
                .is_some_and(|d| d.kind == FeatureKind::Node)
            {
                *by_kind.entry(s.feature.0).or_insert(0usize) += 1;
            }
        }
        let have: usize = by_kind.values().sum();
        if have < want {
            continue; // the land had no room: the share is of what was placed
        }
        let (min, max) = (
            by_kind.values().min().copied().unwrap_or(0),
            by_kind.values().max().copied().unwrap_or(0),
        );
        assert!(max - min <= 1, "{case:?}: {by_kind:?}");
    }
}

#[test]
fn nodes_of_myrror_are_stronger_than_nodes_of_arcanus_on_the_whole() {
    let (mut arcanus, mut myrror) = ((0usize, 0usize), (0usize, 0usize));
    for case in worlds() {
        for s in &generated(case).placements {
            if let Some(n) = &s.node {
                let side = if s.at.plane == Plane::Arcanus {
                    &mut arcanus
                } else {
                    &mut myrror
                };
                side.0 += usize::from(n.power);
                side.1 += 1;
            }
        }
    }
    assert!(arcanus.1 > 0 && myrror.1 > 0);
    // Compare the means without a division: mean_m > mean_a  <=>  sum_m * n_a > sum_a * n_m.
    assert!(
        myrror.0 * arcanus.1 > arcanus.0 * myrror.1,
        "{arcanus:?} {myrror:?}"
    );
}

#[test]
fn the_setting_scales_the_guards_of_the_sites() {
    let guards = |percent: u16| -> usize {
        let mut s = standard().settings();
        s.world.lair_guard_percent = percent;
        // The patch scale of guards by difficulty replaces the percent: the plain rule is tested.
        s.rules.stronger_lairs = false;
        let g = f4x_newgame::mapgen::generate(3, &f4x_newgame::setup::gen_params(&s), content())
            .expect("a map");
        g.placements
            .iter()
            .filter_map(|p| p.lair.as_ref())
            .map(|l| l.guards.iter().map(|&(_, n)| n as usize).sum::<usize>())
            .sum()
    };
    // The content's own percent is the normal; none and double are either side of it.
    let normal_percent = standard().settings().world.lair_guard_percent;
    let (none, normal, most) = (
        guards(0),
        guards(normal_percent),
        guards(normal_percent * 2),
    );
    assert_eq!(none, 0, "no guards at 0 percent");
    assert!(none < normal && normal < most, "{none} {normal} {most}");
}

#[test]
fn rivers_and_lakes_exist_on_the_standard_map() {
    // A generator that drew no river would pass every rule above; the standard map has both.
    let g = generated(standard());
    let count = |f: &dyn Fn(&f4x_core::map::Grid, i32, i32) -> bool| -> usize {
        Plane::all()
            .map(|plane| {
                let grid = g.map.plane(plane);
                (0..grid.height() as i32)
                    .flat_map(|y| (0..grid.width() as i32).map(move |x| (x, y)))
                    .filter(|&(x, y)| f(grid, x, y))
                    .count()
            })
            .sum()
    };
    assert!(count(&|g, x, y| g.has_river(x, y)) > 0, "no river");
    assert!(count(&|g, x, y| g.is_lake(x, y)) > 0, "no lake");
}

#[test]
fn every_mineral_kind_a_plane_allows_is_made_on_it_somewhere_in_the_matrix() {
    let c = content();
    let mut made = std::collections::BTreeSet::new();
    for case in worlds() {
        let g = generated(case);
        for plane in Plane::all() {
            let grid = g.map.plane(plane);
            for y in 0..grid.height() {
                for x in 0..grid.width() {
                    if let Some(m) = grid.mineral(x as i32, y as i32) {
                        made.insert((plane.index(), m.0));
                    }
                }
            }
        }
    }
    for plane in Plane::all() {
        let allowed: std::collections::BTreeSet<u16> = c
            .terrains
            .defs()
            .iter()
            .map(|d| d.class)
            .flat_map(|k| c.features.weighted(FeatureKind::Mineral, k, plane))
            .map(|(f, _)| f.0)
            .collect();
        assert!(!allowed.is_empty());
        for f in allowed {
            assert!(
                made.contains(&(plane.index(), f)),
                "{plane:?}: feature {f} never made"
            );
        }
    }
}

#[test]
fn every_size_the_settings_allow_makes_a_map_that_keeps_the_rules() {
    // Every seventh width and height, so each end and the odd and the even sizes are met.
    let std = standard();
    let (ws, hs) = (
        f4x_def::settings::MAP_WIDTHS,
        f4x_def::settings::MAP_HEIGHTS,
    );
    for width in ws.clone().step_by(7).chain([*ws.end()]) {
        for height in hs.clone().step_by(7).chain([*hs.end()]) {
            let case = WorldCase {
                width,
                height,
                seed: 9,
                ..std
            };
            let problems = map_problems(&generated(case), content(), &params(case), true);
            assert!(problems.is_empty(), "{case:?}: {problems:?}");
        }
    }
}
