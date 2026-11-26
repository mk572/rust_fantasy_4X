//! The map-content settings (tasks 590 and 594): what the generator is asked for follows the
//! settings, and the defaults are numbers for a 60 by 40 map, scaled by the area.

use f4x_core::world::setting::GameSettings;
use f4x_def::testdir::TestResult;
use f4x_newgame::setup::gen_params;

fn sized(width: u16, height: u16) -> GameSettings {
    let mut s = GameSettings::default();
    s.basic.map_width = width;
    s.basic.map_height = height;
    s
}

#[test]
fn the_counts_follow_the_area_of_the_map() {
    let standard = gen_params(&sized(60, 40));
    let huge = gen_params(&sized(90, 60));
    let small = gen_params(&sized(45, 30));
    for (bigger, smaller) in [(&huge, &standard), (&standard, &small)] {
        assert!(bigger.planes[0].node_count > smaller.planes[0].node_count);
        assert!(bigger.planes[1].node_count > smaller.planes[1].node_count);
        assert!(bigger.strong_sites > smaller.strong_sites);
        assert!(bigger.weak_sites > smaller.weak_sites);
    }
    // The share of minerals is a share, not a count: the map's size does not change it.
    assert_eq!(
        huge.planes[0].mineral_percent,
        small.planes[0].mineral_percent
    );
}

#[test]
fn a_number_in_the_settings_replaces_the_scaled_default() {
    let mut s = sized(90, 60);
    s.world.nodes_arcanus = Some(3);
    s.world.nodes_myrror = Some(0);
    s.world.mineral_percent_arcanus = 30;
    s.world.mineral_percent_myrror = 0;
    s.world.strong_sites = Some(7);
    s.world.weak_sites = Some(0);
    let p = gen_params(&s);
    assert_eq!(p.planes[0].node_count, 3);
    assert_eq!(p.planes[1].node_count, 0);
    assert_eq!(p.planes[0].mineral_percent, 30);
    assert_eq!(p.planes[1].mineral_percent, 0);
    assert_eq!((p.strong_sites, p.weak_sites), (7, 0));
    assert!(s.validate().is_empty(), "{:?}", s.validate());
}

#[test]
fn the_generated_map_has_as_many_nodes_as_the_settings_name() -> TestResult {
    use f4x_def::feature::FeatureKind;
    let content = f4x_def::testdir::shipped();
    let mut s = sized(45, 30);
    s.world.nodes_arcanus = Some(4);
    s.world.nodes_myrror = Some(2);
    let map = f4x_newgame::mapgen::generate(5, &gen_params(&s), content)?;
    let nodes = |plane: f4x_core::map::Plane| {
        map.placements
            .iter()
            .filter(|p| p.at.plane == plane)
            .filter(|p| {
                content
                    .features
                    .get(p.feature)
                    .is_some_and(|d| d.kind == FeatureKind::Node)
            })
            .count()
    };
    assert_eq!(nodes(f4x_core::map::Plane::Arcanus), 4);
    assert_eq!(nodes(f4x_core::map::Plane::Myrror), 2);
    Ok(())
}
