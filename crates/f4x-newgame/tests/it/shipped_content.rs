//! The shipped content through the map generator, in the tests that need the generated maps; the
//! loader's own tests are in `f4x-def`.

use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Plane;
use f4x_def::content::Content;
use f4x_def::feature::FeatureKind;
use f4x_def::testdir::TestResult;

/// The game's own `content/` folder, loaded.
fn real_content() -> Content {
    f4x_def::testdir::shipped().clone()
}

#[test]
fn shipped_content_generates_a_map() -> TestResult {
    let generated = f4x_newgame::mapgen::generate(1, &Default::default(), &real_content())?;
    assert!(generated.map.width() > 0 && !generated.placements.is_empty());
    Ok(())
}

/// The minerals and the placed features on one plane of a generated map (a tower is on both).
fn feature_ids(
    generated: &f4x_newgame::mapgen::Generated,
    plane: Plane,
    c: &Content,
) -> Vec<(u32, u32, u16)> {
    let grid = generated.map.plane(plane);
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if let Some(f) = grid.mineral(x as i32, y as i32) {
                out.push((x, y, f.0));
            }
        }
    }
    for p in &generated.placements {
        let def = c.features.get(p.feature).expect("a placed feature exists");
        if p.at.plane == plane || def.kind == FeatureKind::Tower {
            out.push((p.at.x, p.at.y, def.num));
        }
    }
    out
}

#[test]
fn generated_maps_obey_placement_rules() -> TestResult {
    let c = real_content();
    let p = f4x_newgame::mapgen::GenParams::default();
    for seed in 0..5 {
        let map = f4x_newgame::mapgen::generate(seed, &p, &c)?;
        let towers = |plane| -> Vec<(u32, u32)> {
            feature_ids(&map, plane, &c)
                .into_iter()
                .filter(|&(_, _, id)| id == 21 /* tower */)
                .map(|(x, y, _)| (x, y))
                .collect()
        };
        // Towers: right number, identical coordinates on both planes.
        assert_eq!(
            towers(Plane::Arcanus).len(),
            p.tower_count as usize,
            "seed {seed}"
        );
        assert_eq!(towers(Plane::Arcanus), towers(Plane::Myrror), "seed {seed}");
        for plane in Plane::all() {
            let grid = map.map.plane(plane);
            let nodes = feature_ids(&map, plane, &c)
                .into_iter()
                .filter(|&(_, _, id)| {
                    c.features
                        .id_numbered(id)
                        .and_then(|f| c.features.get(f))
                        .is_some_and(|d| d.kind == FeatureKind::Node)
                })
                .count();
            assert_eq!(
                nodes,
                p.planes[plane.index()].node_count as usize,
                "seed {seed}"
            );
            // No feature sits on ocean; every feature is on terrain its rules allow.
            for (x, y, id) in feature_ids(&map, plane, &c) {
                let class = c
                    .terrains
                    .get(grid.get(x as i32, y as i32).ok_or("the value is missing")?)
                    .ok_or("the value is missing")?
                    .class;
                assert_ne!(class, TerrainClass::Ocean, "{id} on ocean, seed {seed}");
                let def = c
                    .features
                    .get(c.features.id_numbered(id).ok_or("the value is missing")?)
                    .ok_or("the value is missing")?;
                if def.kind == FeatureKind::Node {
                    // A node's square has its own base terrain, and no river runs there.
                    let realm = def.realm.ok_or("a node has a realm")?;
                    assert_eq!(
                        Some(class),
                        TerrainClass::of_node_realm(realm),
                        "{id}'s square, seed {seed}"
                    );
                    assert!(!grid.has_river(x as i32, y as i32), "{id}, seed {seed}");
                } else if def.kind != FeatureKind::Tower {
                    assert!(
                        c.features
                            .weighted(def.kind, class, plane)
                            .iter()
                            .any(|(f, _)| f.0 == id),
                        "{id} not allowed on {class:?}/{plane:?}, seed {seed}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn myrror_has_its_own_minerals_and_more_of_them() -> TestResult {
    let c = real_content();
    let p = f4x_newgame::mapgen::GenParams::default();
    let (mut arc_minerals, mut myr_minerals) = (0, 0);
    for seed in 0..10 {
        let map = f4x_newgame::mapgen::generate(seed, &p, &c)?;
        let arcanus = feature_ids(&map, Plane::Arcanus, &c);
        let myrror = feature_ids(&map, Plane::Myrror, &c);
        let is_myrror_only = |id: u16| id == 2 /* adamantium_ore */ || id == 6 /* crysx_crystal */;
        assert!(
            arcanus.iter().all(|&(_, _, id)| !is_myrror_only(id)),
            "seed {seed}"
        );
        let is_mineral = |id: u16| {
            c.features
                .id_numbered(id)
                .and_then(|f| c.features.get(f))
                .is_some_and(|f| f.kind == FeatureKind::Mineral)
        };
        arc_minerals += arcanus.iter().filter(|&&(_, _, id)| is_mineral(id)).count();
        myr_minerals += myrror.iter().filter(|&&(_, _, id)| is_mineral(id)).count();
    }
    assert!(
        myr_minerals > arc_minerals,
        "myrror {myr_minerals} vs arcanus {arc_minerals}"
    );
    Ok(())
}

#[test]
fn myrror_only_minerals_appear_across_many_seeds() -> TestResult {
    let c = real_content();
    let p = f4x_newgame::mapgen::GenParams::default();
    let mut found = false;
    for seed in 0..10 {
        let map = f4x_newgame::mapgen::generate(seed, &p, &c)?;
        found |= feature_ids(&map, Plane::Myrror, &c)
            .iter()
            .any(|&(_, _, id)| id == 2 /* adamantium_ore */);
        if found {
            break;
        }
    }
    assert!(found, "adamantium never generated on Myrror");
    Ok(())
}
