//! The shipped ships (trireme, galley, warship) in a small world of two islands: what they
//! carry, what they need to be built, how troops board, sail, land and drown
//! (`docs/ai-design.md` 11.3).

#[allow(dead_code)]
#[path = "../../src/testworld.rs"]
mod testworld;

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::{MovementType, Stat};
use f4x_core::unit::movement::{Entry, MoveError};
use testworld::{at, build, game, spawn, COAST_CITY, ENEMY_CITY, INLAND_CITY, P0};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn port() -> Position {
    at(COAST_CITY.0, COAST_CITY.1)
}

/// The sea square next to the port, and the one next to the enemy city's shore.
fn near_sea() -> Position {
    at(COAST_CITY.0 + 1, COAST_CITY.1)
}

fn far_sea() -> Position {
    at(ENEMY_CITY.0 - 1, ENEMY_CITY.1 - 1)
}

fn far_shore() -> Position {
    at(ENEMY_CITY.0, ENEMY_CITY.1 - 1)
}

fn ship_building(game: &mut Game, buildings: &[&str]) {
    build(game, port(), buildings);
}

/// Whether the wizard's city at `at` could start building `item`; `Err` if the test names
/// something that does not exist.
fn can_build_item(
    game: &Game,
    at: Position,
    item: impl FnOnce(&PlayerView) -> Option<ProductionItem>,
) -> Result<Result<(), ProductionError>, &'static str> {
    let view = PlayerView::new(game, P0);
    let item = item(&view).ok_or("the shipped content has it")?;
    let city = view.city_at(at).ok_or("the wizard has a city there")?;
    Ok(view.can_build(city, item))
}

fn can_build(game: &Game, unit: &str) -> Result<Result<(), ProductionError>, &'static str> {
    can_build_item(game, port(), |v| {
        v.content()
            .rules
            .units
            .id_of(unit)
            .map(ProductionItem::Unit)
    })
}

fn can_build_building(
    game: &Game,
    at: Position,
    building: &str,
) -> Result<Result<(), ProductionError>, &'static str> {
    can_build_item(game, at, |v| {
        v.content()
            .rules
            .buildings
            .id_of(building)
            .map(ProductionItem::Building)
    })
}

/// A funded wizard whose city feeds its troops, so end turns do not disband the test army.
fn fund(game: &mut Game) -> TestResult {
    game.treasury_mut(P0).ok_or("player 0 has a treasury")?.gold = 1_000_000;
    game.balance_farmers(P0);
    Ok(())
}

fn at_sea(g: &Game, p: Position) -> bool {
    g.economy().terrain_class_at(p) == Some(TerrainClass::Ocean)
}

#[test]
fn the_shipped_ships_sail_and_carry_as_the_help_says() -> TestResult {
    let g = game(COAST_CITY, "high_men");
    let units = &g.content().rules.units;
    for (id, carries) in [("trireme", 2), ("galley", 5), ("warship", 3)] {
        let def = units.get(units.id_of(id).ok_or(id)?);
        assert_eq!(def.movement, MovementType::Sailing, "{id} sails");
        assert_eq!(
            def.stats.get(&Stat::Transport),
            Some(&carries),
            "{id} carries"
        );
    }
    Ok(())
}

#[test]
fn each_ship_needs_its_building() -> TestResult {
    let mut g = game(COAST_CITY, "high_men");
    assert!(can_build(&g, "trireme")?.is_err());
    assert!(can_build(&g, "galley")?.is_err());
    assert!(can_build(&g, "warship")?.is_err());
    ship_building(&mut g, &["ship_wrights_guild"]);
    assert_eq!(can_build(&g, "trireme")?, Ok(()));
    assert!(can_build(&g, "galley")?.is_err(), "a guild is not a yard");
    ship_building(&mut g, &["sawmill", "ship_yard"]);
    assert_eq!(can_build(&g, "galley")?, Ok(()));
    assert!(
        can_build(&g, "warship")?.is_err(),
        "a yard is not a maritime guild"
    );
    ship_building(&mut g, &["armory", "maritime_guild"]);
    assert!(
        can_build(&g, "warship")?.is_err(),
        "a warship also needs barracks"
    );
    ship_building(&mut g, &["barracks"]);
    assert_eq!(can_build(&g, "warship")?, Ok(()));
    Ok(())
}

#[test]
fn a_race_that_may_not_build_a_ship_yard_cannot_have_a_galley() -> TestResult {
    // Beastmen: a Ship Wrights Guild but no Ship Yard (help 280 and the building's race list).
    let mut g = game(COAST_CITY, "beastmen");
    ship_building(&mut g, &["ship_wrights_guild", "sawmill"]);
    assert_eq!(can_build(&g, "trireme")?, Ok(()));
    assert!(can_build(&g, "galley")?.is_err());
    Ok(())
}

/// Put `count` spearmen on the port and try to board a ship waiting on the sea square.
fn board(ship: &str, count: usize) -> Result<Entry, MoveError> {
    let mut g = game(COAST_CITY, "high_men");
    spawn(&mut g, P0, near_sea(), ship, 1);
    let troops = spawn(&mut g, P0, port(), "high_men_spearmen", count);
    g.move_stack(&troops, near_sea())
}

#[test]
fn each_ship_takes_only_as_many_as_it_carries() {
    for (ship, carries) in [("trireme", 2), ("galley", 5), ("warship", 3)] {
        assert!(board(ship, carries).is_ok(), "{ship} takes {carries}");
        assert_eq!(
            board(ship, carries + 1),
            Err(MoveError::NoRoom),
            "{ship} refuses {}",
            carries + 1
        );
    }
}

#[test]
fn ships_keep_to_the_sea_and_their_own_cities() -> TestResult {
    let mut g = game(COAST_CITY, "high_men");
    let ship = spawn(&mut g, P0, near_sea(), "galley", 1);
    assert_eq!(
        g.move_stack(&ship, at(COAST_CITY.0 + 2, COAST_CITY.1)),
        Ok(Entry::Free)
    );
    // Back to the sea square beside the island, then onto the land: refused.
    g.move_stack(&ship, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    let land = at(COAST_CITY.0, COAST_CITY.1 - 1);
    assert_eq!(g.move_stack(&ship, land), Err(MoveError::Impassable));
    // Its own port is open to it.
    assert!(g.move_stack(&ship, port()).is_ok());
    Ok(())
}

#[test]
fn troops_sail_a_galley_across_and_step_ashore() -> TestResult {
    let mut g = game(COAST_CITY, "high_men");
    let galley = spawn(&mut g, P0, port(), "galley", 1)[0];
    let troops = spawn(&mut g, P0, port(), "high_men_spearmen", 4);
    fund(&mut g)?;
    let stack: Vec<_> = std::iter::once(galley)
        .chain(troops.iter().copied())
        .collect();
    let events = g.apply(Command::GoTo {
        units: stack,
        to: far_sea(),
    });
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    for _ in 0..20 {
        if g.units().unit_position(galley) == Some(far_sea()) {
            break;
        }
        g.apply(Command::EndTurn);
        fund(&mut g)?;
    }
    assert_eq!(
        g.units().unit_position(galley),
        Some(far_sea()),
        "the galley arrived"
    );
    for &t in &troops {
        assert_eq!(g.units().unit_position(t), Some(far_sea()), "riding along");
        assert_eq!(g.economy().carrier_of(t), Some(galley));
    }
    // Ashore beside the enemy city: an ordinary move, one unit at a time or all together.
    assert_eq!(g.move_stack(&troops, far_shore()), Ok(Entry::Free));
    for &t in &troops {
        assert_eq!(g.economy().carrier_of(t), None);
        assert_eq!(g.units().unit_position(t), Some(far_shore()));
    }
    assert_eq!(
        g.economy().ship_room(galley),
        5,
        "the places are free again"
    );
    assert_ne!(far_shore(), at(ENEMY_CITY.0, ENEMY_CITY.1));
    Ok(())
}

#[test]
fn troops_left_at_sea_when_their_ship_is_lost_scramble_ashore_or_drown_at_the_turn_end(
) -> TestResult {
    // Beside a shore they use the fleeing odds, so each either lands or drowns.
    let mut g = game(COAST_CITY, "high_men");
    let galley = spawn(&mut g, P0, near_sea(), "galley", 1)[0];
    let troops = spawn(&mut g, P0, port(), "high_men_spearmen", 2);
    fund(&mut g)?;
    g.move_stack(&troops, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.economy().carrier_of(troops[0]), Some(galley));
    g.scenario_remove_unit(galley);
    let events = g.apply(Command::EndTurn);
    let drowned: u32 = events
        .iter()
        .map(|e| match e {
            Event::UnitsDrowned { units, .. } => *units,
            _ => 0,
        })
        .sum();
    let ashore = troops
        .iter()
        .filter(|&&t| g.units().unit_position(t).is_some())
        .count() as u32;
    assert_eq!(drowned + ashore, 2, "each one lands or drowns");
    for &t in &troops {
        if let Some(p) = g.units().unit_position(t) {
            assert!(!at_sea(&g, p), "nobody is left afloat");
        }
    }
    Ok(())
}

#[test]
fn a_loaded_ship_and_its_passengers_all_stand_on_the_ships_square() -> TestResult {
    let mut g = game(COAST_CITY, "high_men");
    spawn(&mut g, P0, near_sea(), "galley", 1);
    let troops = spawn(&mut g, P0, port(), "high_men_spearmen", 5);
    g.move_stack(&troops, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        g.units().units_at(near_sea()).len(),
        6,
        "the galley and five"
    );
    Ok(())
}

#[test]
fn the_ship_buildings_can_only_be_built_in_a_coastal_city() -> TestResult {
    // As in the original. The inland city has every other requirement met.
    let mut g = game(INLAND_CITY, "high_men");
    let inland = at(INLAND_CITY.0, INLAND_CITY.1);
    build(&mut g, inland, &["sawmill", "armory"]);
    assert_eq!(
        can_build_building(&g, inland, "ship_wrights_guild")?,
        Err(ProductionError::NotCoastal)
    );
    // Give it the guild and the yard anyway (as a captured city might have) to reach the next.
    build(&mut g, inland, &["ship_wrights_guild"]);
    assert_eq!(
        can_build_building(&g, inland, "ship_yard")?,
        Err(ProductionError::NotCoastal)
    );
    build(&mut g, inland, &["ship_yard"]);
    assert_eq!(
        can_build_building(&g, inland, "maritime_guild")?,
        Err(ProductionError::NotCoastal)
    );
    // The same buildings are open to the city on the coast.
    let mut coast = game(COAST_CITY, "high_men");
    assert_eq!(
        can_build_building(&coast, port(), "ship_wrights_guild")?,
        Ok(())
    );
    build(
        &mut coast,
        port(),
        &["ship_wrights_guild", "sawmill", "armory"],
    );
    assert_eq!(can_build_building(&coast, port(), "ship_yard")?, Ok(()));
    build(&mut coast, port(), &["ship_yard"]);
    assert_eq!(
        can_build_building(&coast, port(), "maritime_guild")?,
        Ok(())
    );
    Ok(())
}

#[test]
fn other_buildings_do_not_need_the_coast() -> TestResult {
    let g = game(INLAND_CITY, "high_men");
    let inland = at(INLAND_CITY.0, INLAND_CITY.1);
    assert_eq!(can_build_building(&g, inland, "sawmill")?, Ok(()));
    Ok(())
}
