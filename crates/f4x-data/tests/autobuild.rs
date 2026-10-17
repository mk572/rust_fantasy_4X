//! The auto-builder's table against the shipped content (docs/auto-builder.md).

use f4x_core::autobuild::{choose, AutoBuild, BuildPlan, CityFacts, Focus, Mix, AFFINITY};
use f4x_core::production::ProductionItem;

fn content() -> f4x_core::content::Content {
    f4x_data::load_content(std::path::Path::new("../../content")).expect("the rules data loads")
}

#[test]
fn every_building_has_an_affinity_row_and_every_row_a_building() {
    let c = content();
    let ids: Vec<&str> = c
        .rules
        .buildings
        .iter()
        .map(|(_, d)| d.id.as_str())
        .collect();
    for id in &ids {
        assert!(
            AFFINITY.iter().any(|(n, _)| n == id),
            "building {id} has no row in autobuild::AFFINITY"
        );
    }
    for (name, _) in AFFINITY {
        assert!(
            ids.contains(name),
            "AFFINITY names a building {name} that does not exist"
        );
    }
}

/// A fresh size-5 city of the race with every building that needs nothing buildable.
fn facts(c: &f4x_core::content::Content, race: &str) -> CityFacts {
    let race_id = c.rules.races.id_of(race).unwrap();
    let race = c.rules.races.get(race_id);
    let possible: Vec<_> = c
        .rules
        .buildings
        .iter()
        .filter(|(_, d)| d.races.is_empty() || d.races.contains(&race.id))
        .filter(|(_, d)| !d.coastal)
        .map(|(id, _)| id)
        .collect();
    let buildable = possible
        .iter()
        .copied()
        .filter(|&b| c.rules.buildings.get(b).requires.is_empty())
        .collect();
    let units = c
        .rules
        .units
        .iter()
        .filter(|(_, d)| race.units.contains(&d.id) && d.requires.is_empty())
        .map(|(id, _)| id)
        .collect();
    CityFacts {
        figures: 5,
        max_size: 12,
        food: 3,
        net_gold: 10,
        defenders: 2,
        possible,
        buildable,
        units,
        cities: 4,
        ..CityFacts::default()
    }
}

#[test]
fn every_race_and_plan_gets_an_answer_and_the_focus_shows() {
    let c = content();
    let mut roll = |_: u32| 1;
    for (_, race) in c.rules.races.iter() {
        let f = facts(&c, &race.id);
        for primary in Focus::ALL.iter().copied().chain([Focus::Balanced]) {
            for mix in [Mix::Buildings, Mix::Units, Mix::Both] {
                let plan = BuildPlan {
                    mix,
                    primary,
                    ..BuildPlan::default()
                };
                let item = choose(&c, &AutoBuild::new(plan), &f, &mut roll);
                if let ProductionItem::Building(b) = item {
                    assert!(f.buildable.contains(&b), "{} {primary:?}", race.id);
                }
                if let ProductionItem::Unit(u) = item {
                    assert!(f.units.contains(&u), "{} {primary:?}", race.id);
                }
            }
        }
    }
    // An elf (a race with the whole tree) building for gold starts with the marketplace's way.
    let f = facts(&c, "high_elf");
    let gold = BuildPlan {
        mix: Mix::Buildings,
        primary: Focus::Gold,
        ..BuildPlan::default()
    };
    let item = choose(&c, &AutoBuild::new(gold), &f, &mut roll);
    let name = match item {
        ProductionItem::Building(b) => c.rules.buildings.get(b).id.clone(),
        other => panic!("{other:?}"),
    };
    // Only buildings without a requirement are on offer here; the best of them for gold.
    assert!(
        [
            "builders_hall",
            "barracks",
            "smithy",
            "sawmill",
            "ship_wrights_guild"
        ]
        .contains(&name.as_str()),
        "{name}"
    );
}
