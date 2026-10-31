use crate::game::testutil::shipped;
use crate::map::mapgen::{generate, land_percent, GenParams};
use crate::map::terrain::TerrainClass;
use crate::map::Plane;

fn original(land_percent: u8) -> GenParams {
    GenParams {
        original_generator: true,
        land_percent,
        ..GenParams::default()
    }
}

use crate::testdir::TestResult;
#[test]
fn deterministic_and_unlike_the_noise_map() -> TestResult {
    let (c, p) = (shipped(), original(30));
    assert_eq!(generate(3, &p, &c)?, generate(3, &p, &c)?);
    assert_ne!(generate(3, &p, &c)?, generate(4, &p, &c)?);
    let noise = GenParams::default();
    assert_ne!(generate(3, &p, &c)?.map, generate(3, &noise, &c)?.map);
    Ok(())
}

#[test]
fn land_reaches_the_wanted_share_and_grows_with_it() -> TestResult {
    let c = shipped();
    let mut shares = Vec::new();
    for pct in [15u8, 20, 30] {
        let map = generate(5, &original(pct), &c)?.map;
        let got = land_percent(map.plane(Plane::Arcanus), &c.terrains);
        assert!(got >= u32::from(pct) - 1, "{pct}% asked, {got}% made");
        shares.push(got);
    }
    assert!(shares[0] < shares[1] && shares[1] < shares[2], "{shares:?}");
    Ok(())
}

#[test]
fn the_margins_are_ocean_or_tundra_and_the_poles_are_tundra() -> TestResult {
    let c = shipped();
    let map = generate(9, &original(30), &c)?.map;
    for plane in Plane::ALL {
        let grid = map.plane(plane);
        let class = |x: i32, y: i32| {
            c.terrains
                .get(grid.get(x, y).expect("get succeeds"))
                .expect("get succeeds")
                .class
        };
        for x in 0..grid.width() as i32 {
            assert_eq!(class(x, 0), TerrainClass::Tundra);
            assert_eq!(class(x, grid.height() as i32 - 1), TerrainClass::Tundra);
            for y in [2, 3, grid.height() as i32 - 3, grid.height() as i32 - 4] {
                let land = class(x, y) != TerrainClass::Ocean;
                // Land at the margin rows can only be the polar climate pass's tundra.
                assert!(!land || class(x, y) == TerrainClass::Tundra, "({x},{y})");
            }
        }
    }
    Ok(())
}

#[test]
fn works_on_every_map_size_and_a_very_high_land_share() -> TestResult {
    let c = shipped();
    for (w, h) in [(45, 30), (90, 60)] {
        for pct in [15, 99] {
            let p = GenParams {
                original_generator: true,
                land_percent: pct,
                ..GenParams::sized(w, h)
            };
            let map = generate(2, &p, &c)?.map;
            assert!(land_percent(map.plane(Plane::Arcanus), &c.terrains) > 5);
        }
    }
    Ok(())
}

#[test]
fn a_lone_node_gets_ground_on_every_side_over_many_seeds() -> TestResult {
    use crate::feature::def::FeatureKind;
    use crate::feature::place::Placed;
    use crate::map::mapgen::original::extend_islands;
    use crate::map::{Grid, Map};
    let c = shipped();
    let ocean = c
        .terrains
        .first_of_class(TerrainClass::Ocean, Plane::Arcanus)
        .ok_or("first_of_class is missing")?;
    let node = c
        .features
        .first_of_kind(FeatureKind::Node)
        .ok_or("first_of_kind is missing")?;
    let mut grew = [[false; 3]; 3];
    for seed in 0..200 {
        let mut map = Map::new(Grid::new(20, 20, ocean), Grid::new(20, 20, ocean));
        let mut placed = Placed::new(20, 20);
        placed.set(Plane::Arcanus, 10, 10, node);
        extend_islands(seed, &mut map, &placed, &c);
        let grid = map.plane(Plane::Arcanus);
        for (dy, row) in grew.iter_mut().enumerate() {
            for (dx, cell) in row.iter_mut().enumerate() {
                let (x, y) = (9 + dx as i32, 9 + dy as i32);
                *cell |= (x, y) != (10, 10) && grid.get(x, y) != Some(ocean);
            }
        }
        // Nothing grows beyond the eight neighbours.
        let outside = (0..20)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .filter(|&(x, y)| !(9..=11).contains(&x) || !(9..=11).contains(&y))
            .any(|(x, y)| grid.get(x, y) != Some(ocean));
        assert!(!outside);
    }
    // The four sides, and a corner once a side has land.
    assert!(
        grew[0][1] && grew[1][0] && grew[1][2] && grew[2][1],
        "{grew:?}"
    );
    assert!(
        grew[0][0] || grew[0][2] || grew[2][0] || grew[2][2],
        "{grew:?}"
    );
    Ok(())
}
