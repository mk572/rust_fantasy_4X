use super::*;
use crate::game::testutil::{content_with_economy, unit};
use crate::rules::{Registry, Stat, UnitDef};

fn content() -> Content {
    let mut c = content_with_economy();
    let mut defs: Vec<UnitDef> = c.rules.units.defs().to_vec();
    let mut add = |id: &str, move_: MovementType, moves, cost, stats: &[(Stat, i32)]| {
        let mut u = unit(id, move_, moves, &[]);
        u.kind = UnitKind::Normal;
        u.race = Some("elf".into());
        u.cost = cost;
        for &(s, v) in stats {
            u.stats.insert(s, v);
        }
        defs.push(u);
    };
    // A sturdy defender, a hard hitter, an archer, a scout on wings and an engineer.
    add(
        "pikeman",
        MovementType::Walking,
        1,
        30,
        &[(Stat::Defense, 5), (Stat::MaxHp, 4)],
    );
    add(
        "swordsman",
        MovementType::Walking,
        1,
        40,
        &[(Stat::Melee, 6), (Stat::Defense, 2)],
    );
    add(
        "archer",
        MovementType::Walking,
        1,
        40,
        &[(Stat::Melee, 1), (Stat::Ranged, 4)],
    );
    add(
        "skylancer",
        MovementType::Flying,
        4,
        30,
        &[(Stat::Melee, 1), (Stat::Sight, 3)],
    );
    add(
        "engineer",
        MovementType::Walking,
        1,
        40,
        &[(Stat::Melee, 0), (Stat::Construction, 1)],
    );
    c.rules.units = Registry::new(defs);
    c
}

fn building(c: &Content, id: &str) -> BuildingId {
    c.rules.buildings.id_of(id).unwrap()
}

fn unit_id(c: &Content, id: &str) -> UnitId {
    c.rules.units.id_of(id).unwrap()
}

/// A size-4 city with room to grow, rich enough, that may build everything in the content
/// except what needs a building it does not have.
fn facts(c: &Content) -> CityFacts {
    let possible: Vec<BuildingId> = c.rules.buildings.iter().map(|(id, _)| id).collect();
    let free = |id: BuildingId| c.rules.buildings.get(id).requires.is_empty();
    CityFacts {
        figures: 4,
        max_size: 12,
        food: 4,
        net_gold: 20,
        defenders: 2,
        buildable: possible.iter().copied().filter(|&b| free(b)).collect(),
        possible,
        units: ["pikeman", "swordsman", "archer", "skylancer", "engineer"]
            .iter()
            .map(|n| unit_id(c, n))
            .collect(),
        cities: 3,
        ..CityFacts::default()
    }
}

fn first(_: u32) -> u32 {
    1
}

fn plan(mix: Mix, primary: Focus) -> AutoBuild {
    AutoBuild::new(BuildPlan {
        mix,
        primary,
        ..BuildPlan::default()
    })
}

#[test]
fn focus_weights_follow_the_plan() {
    let sum = |w: [f32; 7]| w.iter().sum::<f32>();
    let one = BuildPlan {
        primary: Focus::Gold,
        ..BuildPlan::default()
    };
    assert_eq!(one.weights()[6], 1.0);
    assert!((sum(one.weights()) - 1.0).abs() < 1e-6);
    let two = BuildPlan {
        primary: Focus::Gold,
        secondary: Some(Focus::Defense),
        ..BuildPlan::default()
    };
    let w = two.weights();
    assert!((w[6] - 0.65).abs() < 1e-6 && (w[3] - 0.35).abs() < 1e-6);
    let balanced = BuildPlan::default().weights();
    assert!(balanced.iter().all(|&x| (x - 1.0 / 7.0).abs() < 1e-6));
    // A secondary equal to the primary, or Balanced, changes nothing.
    let same = BuildPlan {
        secondary: Some(Focus::Magic),
        primary: Focus::Magic,
        ..BuildPlan::default()
    };
    assert_eq!(same.weights()[0], 1.0);
}

#[test]
fn every_affinity_row_names_a_known_focus_count_and_is_small() {
    assert!(AFFINITY.iter().all(|(_, a)| a.iter().all(|&v| v <= 3)));
    let mut names: Vec<&str> = AFFINITY.iter().map(|(n, _)| *n).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), AFFINITY.len(), "a building is listed twice");
}

#[test]
fn a_city_below_its_garrison_minimum_builds_the_sturdiest_defender() {
    let c = content();
    let mut f = facts(&c);
    f.defenders = 1;
    let item = choose(&c, &plan(Mix::Both, Focus::Gold), &f, &mut first);
    assert_eq!(item, ProductionItem::Unit(unit_id(&c, "pikeman")));
    // Buildings only never builds units.
    let item = choose(&c, &plan(Mix::Buildings, Focus::Gold), &f, &mut first);
    assert!(matches!(item, ProductionItem::Building(_)), "{item:?}");
    // At the minimum, the rule is satisfied.
    f.defenders = 2;
    let item = choose(&c, &plan(Mix::Both, Focus::Gold), &f, &mut first);
    assert!(matches!(item, ProductionItem::Building(_)), "{item:?}");
}

#[test]
fn the_garrison_minimum_rises_with_size_a_rival_and_the_defense_focus() {
    assert_eq!(
        [0, 1, 2, 3, 5, 6, 8, 9, 20].map(garrison_minimum),
        [0, 1, 1, 2, 2, 3, 3, 4, 4]
    );
    let c = content();
    let mut f = facts(&c);
    f.defenders = 2;
    let p = plan(Mix::Both, Focus::Gold);
    assert!(matches!(
        choose(&c, &p, &f, &mut first),
        ProductionItem::Building(_)
    ));
    f.rival_near = true;
    assert!(matches!(
        choose(&c, &p, &f, &mut first),
        ProductionItem::Unit(_)
    ));
    f.rival_near = false;
    let defense = plan(Mix::Both, Focus::Defense);
    f.defenders = 3;
    assert!(matches!(
        choose(&c, &defense, &f, &mut first),
        ProductionItem::Unit(_)
    ));
    f.defenders = 4;
    assert!(!matches!(
        choose(&c, &defense, &f, &mut first),
        ProductionItem::Unit(_)
    ));
    // As a secondary focus it asks for one more.
    let second = AutoBuild::new(BuildPlan {
        secondary: Some(Focus::Defense),
        ..plan(Mix::Both, Focus::Gold).plan
    });
    f.defenders = 2;
    assert!(matches!(
        choose(&c, &second, &f, &mut first),
        ProductionItem::Unit(_)
    ));
    // An outpost needs nobody.
    f.figures = 0;
    f.defenders = 0;
    assert!(!matches!(
        choose(&c, &p, &f, &mut first),
        ProductionItem::Unit(_)
    ));
}

#[test]
fn rebels_call_for_a_building_that_lowers_unrest() {
    let c = content();
    let mut f = facts(&c);
    f.rebels = 2;
    let item = choose(&c, &plan(Mix::Buildings, Focus::Production), &f, &mut first);
    assert_eq!(item, ProductionItem::Building(building(&c, "temple")));
}

#[test]
fn low_gold_builds_a_gold_building_then_trade_goods() {
    let c = content();
    let mut f = facts(&c);
    f.net_gold = 1;
    let p = plan(Mix::Buildings, Focus::Offense);
    assert_eq!(
        choose(&c, &p, &f, &mut first),
        ProductionItem::Building(building(&c, "marketplace"))
    );
    let market = building(&c, "marketplace");
    f.buildable.retain(|&b| b != market);
    f.possible.retain(|&b| b != market);
    assert_eq!(choose(&c, &p, &f, &mut first), ProductionItem::TradeGoods);
}

#[test]
fn a_building_whose_upkeep_the_treasury_cannot_bear_is_skipped() {
    let c = content();
    let mut f = facts(&c);
    f.net_gold = 3;
    f.buildable = vec![building(&c, "temple"), building(&c, "library")];
    f.rebels = 0;
    let item = choose(&c, &plan(Mix::Buildings, Focus::Magic), &f, &mut first);
    // The temple costs 2 and the library 1; both are borne at 3.
    assert!(matches!(item, ProductionItem::Building(_)));
    f.net_gold = 3;
    f.buildable = vec![building(&c, "war_college")];
    let item = choose(&c, &plan(Mix::Buildings, Focus::Offense), &f, &mut first);
    assert_ne!(
        item,
        ProductionItem::Building(building(&c, "war_college")),
        "its upkeep is 5"
    );
}

#[test]
fn the_focus_decides_which_building_comes_first() {
    let c = content();
    let f = facts(&c);
    let pick = |focus| match choose(&c, &plan(Mix::Buildings, focus), &f, &mut first) {
        ProductionItem::Building(b) => c.rules.buildings.get(b).id.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(pick(Focus::Gold), "marketplace");
    assert_eq!(pick(Focus::Production), "sawmill");
    assert_eq!(pick(Focus::Magic), "library");
    assert_eq!(pick(Focus::Population), "granary");
}

#[test]
fn a_building_that_opens_a_better_one_gets_credit() {
    let c = content();
    let mut f = facts(&c);
    // Only the barracks (no value to a gold plan) and the guild above it are in play.
    f.buildable = vec![building(&c, "barracks")];
    f.possible = vec![building(&c, "barracks"), building(&c, "fighters_guild")];
    let offense = AutoBuild::new(BuildPlan {
        mix: Mix::Buildings,
        primary: Focus::Offense,
        ..BuildPlan::default()
    });
    let with_dependent = building_score(&c, &offense.plan, &f, building(&c, "barracks"));
    f.possible = vec![building(&c, "barracks")];
    let alone = building_score(&c, &offense.plan, &f, building(&c, "barracks"));
    assert!(with_dependent > alone);
}

#[test]
fn hunger_and_a_full_city_favour_food_buildings() {
    let c = content();
    let mut f = facts(&c);
    let p = BuildPlan {
        mix: Mix::Buildings,
        ..BuildPlan::default()
    };
    let granary = building(&c, "granary");
    let calm = building_score(&c, &p, &f, granary);
    f.food = 0;
    assert!((building_score(&c, &p, &f, granary) - 2.0 * calm).abs() < 1e-6);
    f.food = 4;
    f.figures = 11;
    assert!((building_score(&c, &p, &f, granary) - 2.0 * calm).abs() < 1e-6);
}

#[test]
fn both_keeps_production_on_units_and_buildings_level() {
    let c = content();
    let f = facts(&c);
    let mut a = plan(Mix::Both, Focus::Offense);
    assert!(
        matches!(choose(&c, &a, &f, &mut first), ProductionItem::Building(_)),
        "a tie goes to buildings"
    );
    a.spent_buildings = 100;
    assert!(matches!(
        choose(&c, &a, &f, &mut first),
        ProductionItem::Unit(_)
    ));
    a.spent_units = 100;
    assert!(matches!(
        choose(&c, &a, &f, &mut first),
        ProductionItem::Building(_)
    ));
    // Units only, whatever has been spent.
    let u = plan(Mix::Units, Focus::Offense);
    assert!(matches!(
        choose(&c, &u, &f, &mut first),
        ProductionItem::Unit(_)
    ));
}

#[test]
fn the_unit_roll_prefers_the_best_but_varies() {
    let c = content();
    let f = facts(&c);
    let p = BuildPlan {
        mix: Mix::Units,
        primary: Focus::Offense,
        ..BuildPlan::default()
    };
    let ranked = ranked_units(&c, &p, &f, false);
    let best = ranked[0].0;
    assert_eq!(c.rules.units.get(best).id, "swordsman");
    assert_eq!(
        roll_unit(&ranked, &mut |_| 1),
        Some(best),
        "a low roll takes the best"
    );
    let high = roll_unit(&ranked, &mut |total| total).unwrap();
    assert_ne!(high, best, "a high roll takes another unit");
    // Every roll lands on someone, and the weak are never drawn.
    let ids: std::collections::BTreeSet<UnitId> = (1..=2000)
        .filter_map(|n| roll_unit(&ranked, &mut |t| n.min(t)))
        .collect();
    assert!(ids.len() >= 2 && ids.len() <= ranked.len());
}

#[test]
fn defense_and_exploration_pick_their_own_units() {
    let c = content();
    let f = facts(&c);
    let top = |focus| {
        let p = BuildPlan {
            mix: Mix::Units,
            primary: focus,
            ..BuildPlan::default()
        };
        let r = ranked_units(&c, &p, &f, false);
        c.rules.units.get(r[0].0).id.clone()
    };
    assert_eq!(top(Focus::Defense), "pikeman");
    assert_eq!(top(Focus::Offense), "swordsman");
    assert_eq!(top(Focus::Exploration), "skylancer");
    // Engineers and scouts are only for exploration.
    let p = BuildPlan {
        mix: Mix::Units,
        primary: Focus::Offense,
        ..BuildPlan::default()
    };
    let ids: Vec<String> = ranked_units(&c, &p, &f, false)
        .iter()
        .map(|&(u, _)| c.rules.units.get(u).id.clone())
        .collect();
    assert!(!ids.contains(&"engineer".to_string()), "{ids:?}");
}

#[test]
fn settlers_need_room_space_and_a_wish() {
    let c = content();
    let mut f = facts(&c);
    f.units.push(unit_id(&c, "settler"));
    f.figures = 10;
    f.max_size = 11;
    f.open_sites = 3;
    let on = BuildPlan {
        settlers: true,
        ..BuildPlan::default()
    };
    assert!(wants_settlers(&on, &f));
    assert!(!wants_settlers(&BuildPlan::default(), &f), "off by default");
    let mut g = f.clone();
    g.figures = 3;
    g.max_size = 4;
    assert!(!wants_settlers(&on, &g), "below the minimum population");
    let mut g = f.clone();
    g.max_size = 20;
    assert!(!wants_settlers(&on, &g), "far from the limit");
    let mut g = f.clone();
    g.open_sites = 0;
    assert!(!wants_settlers(&on, &g), "no open space");
    let mut g = f.clone();
    g.settlers_near = 2;
    assert!(!wants_settlers(&on, &g), "enough settlers already");
    g.open_sites = 5;
    assert!(!wants_settlers(&on, &g), "never more than two at a time");
    g.settlers_near = 1;
    assert!(wants_settlers(&on, &g), "wide open land wants two");

    // They come before the focus choice.
    f.defenders = 4;
    let auto = AutoBuild::new(BuildPlan {
        primary: Focus::Gold,
        ..on
    });
    assert_eq!(
        choose(&c, &auto, &f, &mut first),
        ProductionItem::Unit(unit_id(&c, "settler"))
    );
    // But not before the garrison.
    f.defenders = 0;
    assert_eq!(
        choose(&c, &auto, &f, &mut first),
        ProductionItem::Unit(unit_id(&c, "pikeman"))
    );
    // And buildings-only cities never build them.
    f.defenders = 4;
    let buildings = AutoBuild::new(BuildPlan {
        mix: Mix::Buildings,
        ..auto.plan
    });
    assert!(matches!(
        choose(&c, &buildings, &f, &mut first),
        ProductionItem::Building(_)
    ));
}

#[test]
fn at_the_end_of_the_list_gold_is_the_last_fallback() {
    let c = content();
    let mut f = facts(&c);
    f.buildable.clear();
    f.possible.clear();
    let p = plan(Mix::Buildings, Focus::Offense);
    assert_eq!(choose(&c, &p, &f, &mut first), ProductionItem::TradeGoods);
    // A city that can still grow, and cares for population, takes housing.
    let pop = plan(Mix::Buildings, Focus::Population);
    assert_eq!(choose(&c, &pop, &f, &mut first), ProductionItem::Housing);
    f.figures = f.max_size;
    assert_eq!(choose(&c, &pop, &f, &mut first), ProductionItem::TradeGoods);
}

#[test]
fn a_plan_survives_a_save_and_the_command_starts_an_idle_city() {
    use crate::game::Command;
    use crate::map::{Grid, Map, Plane};
    use crate::player::Player;
    use crate::player::PlayerId;
    let c = content();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(3);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    let mut p = Player::new(PlayerId(0), "W");
    p.gold = 500;
    p.tax_rate = 2;
    g.players.push(p);
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner: PlayerId(0),
        pos: crate::map::Position {
            plane: Plane::Arcanus,
            x: 10,
            y: 6,
        },
        name: "Town".into(),
        race,
        population: 4,
    });
    let city = g.economy().cities()[0].0;
    // A new city starts on housing; with a plan it takes over at once.
    assert_eq!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Housing)
    );
    let plan = BuildPlan {
        mix: Mix::Buildings,
        primary: Focus::Gold,
        ..BuildPlan::default()
    };
    g.apply(Command::SetAutoBuild {
        city,
        plan: Some(plan),
    });
    assert!(matches!(
        g.economy().city_production(city).item,
        Some(ProductionItem::Building(_))
    ));
    assert_eq!(g.autobuild_of(city).unwrap().plan, plan);
    let snap = g.snapshot();
    let loaded = crate::game::save::Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(content())
        .unwrap();
    let again = loaded.economy().cities()[0].0;
    assert_eq!(loaded.autobuild_of(again).unwrap().plan, plan);
    // Switched off, the city has none (its owner has no Vizier).
    g.apply(Command::SetAutoBuild { city, plan: None });
    assert!(g.autobuild_of(city).is_none());
}
