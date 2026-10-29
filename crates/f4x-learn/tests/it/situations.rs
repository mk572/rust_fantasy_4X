//! The scripted battle AI fights out battles on ground the generated scenarios do not cover:
//! other terrain, the sites of the map, a hero with an item (`examples/situations.rs` runs the
//! same over many more seeds and prints a table).

use f4x_core::map::terrain::TerrainClass;
use f4x_learn::build::{build_at, Spot};
use f4x_learn::scenario::generate_raw;

fn fights_end(spot: Spot, hero: bool) {
    let content = f4x_data::load_content(std::path::Path::new("../../content")).unwrap();
    let mut fought = 0;
    for seed in 0..4u64 {
        let scenario = generate_raw(&content, seed);
        let Ok(mut game) = build_at(&content, &scenario, &spot, seed as usize, hero) else {
            continue;
        };
        fought += 1;
        let mut steps = 0;
        while game.in_battle() && steps < 5_000 {
            let command = game.next_battle_command().expect("a battle has a command");
            game.battle_command(command)
                .unwrap_or_else(|e| panic!("{spot:?} seed {seed}: refused {e:?}"));
            steps += 1;
        }
        assert!(
            !game.in_battle(),
            "{spot:?} seed {seed}: no end in {steps} commands"
        );
    }
    assert!(fought > 0, "{spot:?}: the map has no such ground");
}

#[test]
fn battles_on_other_terrain_end_without_a_refused_command() {
    for class in [
        TerrainClass::Forest,
        TerrainClass::Hills,
        TerrainClass::Mountains,
        TerrainClass::Swamp,
        TerrainClass::Desert,
        TerrainClass::Tundra,
    ] {
        fights_end(Spot::Class(class), false);
    }
}

#[test]
fn battles_at_sites_end_without_a_refused_command() {
    for site in [
        "tower",
        "chaos_node",
        "sorcery_node",
        "monster_lair",
        "dungeon",
    ] {
        fights_end(Spot::Site(site.into()), false);
    }
}

#[test]
fn a_hero_with_an_item_fights_on_to_the_end() {
    fights_end(Spot::Class(TerrainClass::Grassland), true);
    fights_end(Spot::Site("ancient_temple".into()), true);
}
