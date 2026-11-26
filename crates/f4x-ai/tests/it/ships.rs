//! The shipped ships (trireme, galley, warship) in a small world of two islands: what they
//! carry, what they need to be built, how troops board, sail, land and drown
//! (`docs/ai-design.md` 11.3).

#[allow(dead_code)]
#[path = "../../src/testworld.rs"]
mod testworld;

use f4x_core::city::production::{ProductionError, ProductionItem};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::unit::movement::{Entry, MoveError};
use f4x_core::world::{Command, Event, World};
use f4x_def::rules::{MovementType, Stat};
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

fn ship_building(world: &mut World, buildings: &[u16]) {
    build(world, port(), buildings);
}

/// Whether the wizard's city at `at` could start building `item`; `Err` if the test names
/// something that does not exist.
fn can_build_item(
    world: &World,
    at: Position,
    item: impl FnOnce(&PlayerView) -> Option<ProductionItem>,
) -> Result<Result<(), ProductionError>, &'static str> {
    let view = PlayerView::new(world, P0);
    let item = item(&view).ok_or("the shipped content has it")?;
    let city = view.city_at(at).ok_or("the wizard has a city there")?;
    Ok(view.can_build(city, item))
}

fn can_build(world: &World, unit: u16) -> Result<Result<(), ProductionError>, &'static str> {
    can_build_item(world, port(), |v| {
        v.content()
            .rules
            .units
            .id_numbered(unit)
            .map(ProductionItem::Unit)
    })
}

fn can_build_building(
    world: &World,
    at: Position,
    building: u16,
) -> Result<Result<(), ProductionError>, &'static str> {
    can_build_item(world, at, |v| {
        v.content()
            .rules
            .buildings
            .id_numbered(building)
            .map(ProductionItem::Building)
    })
}

/// A funded wizard whose city feeds its troops, so end turns do not disband the test army.
fn fund(world: &mut World) -> TestResult {
    world
        .treasury_mut(P0)
        .ok_or("player 0 has a treasury")?
        .gold = f4x_def::half_int::HalfInt::from_int(1_000_000);
    world.balance_farmers(P0);
    Ok(())
}

fn at_sea(w: &World, p: Position) -> bool {
    w.economy().terrain_class_at(p) == Some(TerrainClass::Ocean)
}

#[test]
fn the_shipped_ships_sail_and_carry_as_the_help_says() -> TestResult {
    let w = game(COAST_CITY, 8 /* high_men */);
    let units = &w.content().rules.units;
    for (id, carries) in [
        (35 /* trireme */, 2),
        (36 /* galley */, 5),
        (38 /* warship */, 3),
    ] {
        let def = units.get(units.id_numbered(id).ok_or("no such unit")?);
        assert_eq!(def.movement, MovementType::Sailing, "{id} sails");
        assert_eq!(
            def.stats.get(&Stat::Transport),
            Some(&f4x_def::half_int::HalfInt::from_int(carries)),
            "{id} carries"
        );
    }
    Ok(())
}

#[test]
fn each_ship_needs_its_building() -> TestResult {
    let mut w = game(COAST_CITY, 8 /* high_men */);
    assert!(can_build(&w, 35 /* trireme */)?.is_err());
    assert!(can_build(&w, 36 /* galley */)?.is_err());
    assert!(can_build(&w, 38 /* warship */)?.is_err());
    ship_building(&mut w, &[12 /* ship_wrights_guild */]);
    assert_eq!(can_build(&w, 35 /* trireme */)?, Ok(()));
    assert!(
        can_build(&w, 36 /* galley */)?.is_err(),
        "a guild is not a yard"
    );
    ship_building(&mut w, &[15 /* sawmill */, 13 /* ship_yard */]);
    assert_eq!(can_build(&w, 36 /* galley */)?, Ok(()));
    assert!(
        can_build(&w, 38 /* warship */)?.is_err(),
        "a yard is not a maritime guild"
    );
    ship_building(&mut w, &[4 /* armory */, 14 /* maritime_guild */]);
    assert!(
        can_build(&w, 38 /* warship */)?.is_err(),
        "a warship also needs barracks"
    );
    ship_building(&mut w, &[3 /* barracks */]);
    assert_eq!(can_build(&w, 38 /* warship */)?, Ok(()));
    Ok(())
}

#[test]
fn a_race_that_may_not_build_a_ship_yard_cannot_have_a_galley() -> TestResult {
    // Beastmen: a Ship Wrights Guild but no Ship Yard (help 280 and the building's race list).
    let mut w = game(COAST_CITY, 1 /* beastmen */);
    ship_building(&mut w, &[12 /* ship_wrights_guild */, 15 /* sawmill */]);
    assert_eq!(can_build(&w, 35 /* trireme */)?, Ok(()));
    assert!(can_build(&w, 36 /* galley */)?.is_err());
    Ok(())
}

/// Put `count` spearmen on the port and try to board a ship waiting on the sea square.
fn board(ship: u16, count: usize) -> Result<Entry, MoveError> {
    let mut w = game(COAST_CITY, 8 /* high_men */);
    spawn(&mut w, P0, near_sea(), ship, 1);
    let troops = spawn(&mut w, P0, port(), 104 /* high_men_spearmen */, count);
    w.move_stack(&troops, near_sea())
}

#[test]
fn each_ship_takes_only_as_many_as_it_carries() {
    for (ship, carries) in [
        (35 /* trireme */, 2),
        (36 /* galley */, 5),
        (38 /* warship */, 3),
    ] {
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
    let mut w = game(COAST_CITY, 8 /* high_men */);
    let ship = spawn(&mut w, P0, near_sea(), 36 /* galley */, 1);
    assert_eq!(
        w.move_stack(&ship, at(COAST_CITY.0 + 2, COAST_CITY.1)),
        Ok(Entry::Free)
    );
    // Back to the sea square beside the island, then onto the land: refused.
    w.move_stack(&ship, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    let land = at(COAST_CITY.0, COAST_CITY.1 - 1);
    assert_eq!(w.move_stack(&ship, land), Err(MoveError::Impassable));
    // Its own port is open to it.
    assert!(w.move_stack(&ship, port()).is_ok());
    Ok(())
}

#[test]
fn troops_sail_a_galley_across_and_step_ashore() -> TestResult {
    let mut w = game(COAST_CITY, 8 /* high_men */);
    let galley = spawn(&mut w, P0, port(), 36 /* galley */, 1)[0];
    let troops = spawn(&mut w, P0, port(), 104 /* high_men_spearmen */, 4);
    fund(&mut w)?;
    let stack: Vec<_> = std::iter::once(galley)
        .chain(troops.iter().copied())
        .collect();
    let events = w.apply(Command::GoTo {
        units: stack,
        to: far_sea(),
    });
    assert!(
        !events.iter().any(|e| matches!(e, Event::Rejected(_))),
        "{events:?}"
    );
    for _ in 0..20 {
        if w.units().unit_position(galley) == Some(far_sea()) {
            break;
        }
        w.play_round();
        fund(&mut w)?;
    }
    assert_eq!(
        w.units().unit_position(galley),
        Some(far_sea()),
        "the galley arrived"
    );
    for &t in &troops {
        assert_eq!(w.units().unit_position(t), Some(far_sea()), "riding along");
        assert_eq!(w.economy().carrier_of(t), Some(galley));
    }
    // Ashore beside the enemy city: an ordinary move, one unit at a time or all together.
    assert_eq!(w.move_stack(&troops, far_shore()), Ok(Entry::Free));
    for &t in &troops {
        assert_eq!(w.economy().carrier_of(t), None);
        assert_eq!(w.units().unit_position(t), Some(far_shore()));
    }
    assert_eq!(
        w.economy().ship_room(galley),
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
    let mut w = game(COAST_CITY, 8 /* high_men */);
    let galley = spawn(&mut w, P0, near_sea(), 36 /* galley */, 1)[0];
    let troops = spawn(&mut w, P0, port(), 104 /* high_men_spearmen */, 2);
    fund(&mut w)?;
    w.move_stack(&troops, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(w.economy().carrier_of(troops[0]), Some(galley));
    w.scenario_remove_unit(galley);
    let events = w.play_round();
    let drowned: u32 = events
        .iter()
        .map(|e| match e {
            Event::UnitsDrowned { units, .. } => *units,
            _ => 0,
        })
        .sum();
    let ashore = troops
        .iter()
        .filter(|&&t| w.units().unit_position(t).is_some())
        .count() as u32;
    assert_eq!(drowned + ashore, 2, "each one lands or drowns");
    for &t in &troops {
        if let Some(p) = w.units().unit_position(t) {
            assert!(!at_sea(&w, p), "nobody is left afloat");
        }
    }
    Ok(())
}

#[test]
fn a_loaded_ship_and_its_passengers_all_stand_on_the_ships_square() -> TestResult {
    let mut w = game(COAST_CITY, 8 /* high_men */);
    spawn(&mut w, P0, near_sea(), 36 /* galley */, 1);
    let troops = spawn(&mut w, P0, port(), 104 /* high_men_spearmen */, 5);
    w.move_stack(&troops, near_sea())
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        w.units().units_at(near_sea()).len(),
        6,
        "the galley and five"
    );
    Ok(())
}

#[test]
fn the_ship_buildings_can_only_be_built_in_a_coastal_city() -> TestResult {
    // The inland city has every other requirement met.
    let mut w = game(INLAND_CITY, 8 /* high_men */);
    let inland = at(INLAND_CITY.0, INLAND_CITY.1);
    build(&mut w, inland, &[15 /* sawmill */, 4 /* armory */]);
    assert_eq!(
        can_build_building(&w, inland, 12 /* ship_wrights_guild */)?,
        Err(ProductionError::NotCoastal)
    );
    // Give it the guild and the yard anyway (as a captured city might have) to reach the next.
    build(&mut w, inland, &[12 /* ship_wrights_guild */]);
    assert_eq!(
        can_build_building(&w, inland, 13 /* ship_yard */)?,
        Err(ProductionError::NotCoastal)
    );
    build(&mut w, inland, &[13 /* ship_yard */]);
    assert_eq!(
        can_build_building(&w, inland, 14 /* maritime_guild */)?,
        Err(ProductionError::NotCoastal)
    );
    // The same buildings are open to the city on the coast.
    let mut coast = game(COAST_CITY, 8 /* high_men */);
    assert_eq!(
        can_build_building(&coast, port(), 12 /* ship_wrights_guild */)?,
        Ok(())
    );
    build(
        &mut coast,
        port(),
        &[
            12, /* ship_wrights_guild */
            15, /* sawmill */
            4,  /* armory */
        ],
    );
    assert_eq!(
        can_build_building(&coast, port(), 13 /* ship_yard */)?,
        Ok(())
    );
    build(&mut coast, port(), &[13 /* ship_yard */]);
    assert_eq!(
        can_build_building(&coast, port(), 14 /* maritime_guild */)?,
        Ok(())
    );
    Ok(())
}

#[test]
fn other_buildings_do_not_need_the_coast() -> TestResult {
    let w = game(INLAND_CITY, 8 /* high_men */);
    let inland = at(INLAND_CITY.0, INLAND_CITY.1);
    assert_eq!(can_build_building(&w, inland, 15 /* sawmill */)?, Ok(()));
    Ok(())
}
