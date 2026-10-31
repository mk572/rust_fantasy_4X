use super::*;
use crate::game::testutil::num;
use crate::hero::game::tests::{at, world, P0};
use crate::item::ItemBonus;
use crate::map::{Grid, Map};
use crate::rules::ItemKind;
use crate::testdir::TestResult;

use crate::unit::MovesLeft;

fn item(name: &str, cost: u32) -> Item {
    Item {
        name: name.into(),
        kind: ItemKind::Misc,
        cost,
        icon: 0,
        bonus: ItemBonus::default(),
        powers: Vec::new(),
        charges: None,
        prefab: None,
        made: None,
    }
}

#[test]
fn the_wizard_sees_the_shortfall_and_picks_who_goes() -> TestResult {
    let mut g = world();
    let (a, b) = (
        g.spawn_hero(P0, at(1, 1), 0)
            .map_err(|e| format!("{e:?}"))?,
        g.spawn_hero(P0, at(1, 2), 1)
            .map_err(|e| format!("{e:?}"))?,
    );
    g.tr_mut(0).gold = 2;
    let short = g
        .treasury_shortfall(P0)
        .expect("4 gold of heroes against 2");
    assert_eq!(
        short.default_release,
        vec![b],
        "the turn change lets the newest go"
    );
    assert_eq!(short.missing(&[]), (0, 2));
    assert_eq!(short.missing(&[a]), (0, 0));
    assert!(short.troops.iter().all(|l| l.hero && l.gold == 2));
    // The wizard keeps the newest and gives up the first; its item comes home.
    g.give_item(P0, item("Sword", 100));
    let events = g
        .release_troops(P0, &[a], &[])
        .map_err(|e| format!("{e:?}"))?;
    assert!(g.treasury_shortfall(P0).is_none());
    assert!(g.world.get::<&crate::unit::Unit>(a).is_err() && g.world.contains(b));
    assert!(events.iter().any(|e| matches!(e, Event::HeroLeft { .. })));
    assert!(events.contains(&Event::Shortage {
        player: P0,
        what: Shortage::Gold
    }));
    assert_eq!(
        crate::hero::game::tests::state(&g, 0),
        crate::hero::HeroState::Free
    );
    Ok(())
}

#[test]
fn a_released_heros_items_return_to_the_armory() -> TestResult {
    let mut g = world();
    let h = g
        .spawn_hero(P0, at(1, 1), 0)
        .map_err(|e| format!("{e:?}"))?;
    g.spawn_item(item("Axe", 200), crate::item::entity::HeldBy::Hero(h, 0));
    g.tr_mut(0).gold = 0;
    g.release_troops(P0, &[h], &[])
        .map_err(|e| format!("{e:?}"))?;
    let names: Vec<_> = g
        .items()
        .armory(P0)
        .items()
        .map(|i| i.name.clone())
        .collect();
    assert_eq!(names, ["Axe"]);
    Ok(())
}

#[test]
fn the_vault_has_room_for_all_so_nothing_overflows_and_scrapping_is_the_wizards_choice(
) -> TestResult {
    let mut g = world();
    for i in 0..12 {
        g.give_item(P0, item(&format!("Junk{i}"), 10 + 10 * i as u32));
    }
    let h = g
        .spawn_hero(P0, at(1, 1), 0)
        .map_err(|e| format!("{e:?}"))?;
    g.spawn_item(item("Crown", 1000), crate::item::entity::HeldBy::Hero(h, 1));
    assert!(g.item_overflow(P0, &[]).is_none());
    assert!(
        g.item_overflow(P0, &[h]).is_none(),
        "thirteen items, no limit"
    );
    // The wizard may still scrap an item for mana while releasing a hero.
    g.tr_mut(0).mana = 0;
    g.release_troops(P0, &[h], &[ItemSpot::Vault(6)])
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.tr(0).mana, 35, "Junk6, worth 70 gold, gives half in mana");
    assert!(g.items().armory(P0).items().any(|i| i.name == "Crown"));
    Ok(())
}

/// A 30x16 map, grass but for a sea from x = 3 to 26 and y = 3 to 13.
fn sea_world() -> Game {
    let mut g = world();
    let (grass, ocean) = (
        g.content
            .terrains
            .id_numbered(num::terrain::GRASSLAND)
            .expect("id_of succeeds"),
        g.content
            .terrains
            .id_numbered(num::terrain::OCEAN)
            .expect("id_of succeeds"),
    );
    let build = || {
        let mut grid = Grid::new(30, 16, grass);
        for x in 3..27 {
            for y in 3..14 {
                grid.set(x, y, ocean);
            }
        }
        grid
    };
    g.map = Some(Map::new(build(), build()));
    g
}

fn spawn(g: &mut Game, id: &str, pos: crate::map::Position) -> hecs::Entity {
    let u = g.content.rules.units.id_of(id).expect("id_of succeeds");
    g.spawn_unit(P0, pos, u)
}

#[test]
fn a_ship_let_go_at_sea_sails_to_the_shore_and_unloads() -> TestResult {
    let mut g = sea_world();
    let boat = spawn(&mut g, "galley", at(15, 8));
    let riders: Vec<_> = (0..2)
        .map(|_| spawn(&mut g, "high_men_cavalry", at(15, 8)))
        .collect();
    for &r in &riders {
        let serial = g.units().serial_of(boat);
        let _ = g
            .world
            .insert_one(r, crate::unit::transport::Aboard(serial));
    }
    g.tr_mut(0).gold = 0;
    // 3 gold of upkeep, nothing to pay it: the boat goes, the riders are the next to be paid for.
    let events = g
        .release_troops(P0, &[boat], &[])
        .map_err(|e| format!("{e:?}"))?;
    assert!(events.iter().any(|e| matches!(
        e,
        Event::UnitLost {
            cause: LossCause::Gold,
            ..
        }
    )));
    assert_eq!(g.units().unit_owner(boat), Some(MONSTERS));
    assert!(g.world.get::<&Marooned>(boat).is_ok());
    // It keeps sailing, a turn's worth at a time, with the riders aboard.
    let mut turns = 0;
    let mut landed = Vec::new();
    while g.world.contains(boat) && turns < 10 {
        let before = g
            .units()
            .unit_position(boat)
            .ok_or("unit_position is missing")?;
        landed.extend(g.sail_marooned());
        turns += 1;
        if g.world.contains(boat) {
            let after = g
                .units()
                .unit_position(boat)
                .ok_or("unit_position is missing")?;
            assert_ne!(before, after, "it moves every turn");
            assert!(riders
                .iter()
                .all(|&r| g.units().unit_position(r) == Some(after)));
        }
    }
    assert!(turns > 1, "the shore is far: {turns}");
    assert!(!g.world.contains(boat), "released after unloading");
    assert!(landed
        .iter()
        .any(|e| matches!(e, Event::ShipLanded { player, units: 2, .. } if *player == P0)));
    for r in riders {
        let at = g
            .units()
            .unit_position(r)
            .ok_or("unit_position is missing")?;
        assert!(!g.economy().is_sea(at));
        assert_eq!(g.units().unit_owner(r), Some(P0), "still the wizard's");
        assert!(g.economy().carrier_of(r).is_none());
    }
    Ok(())
}

#[test]
fn a_ship_in_port_or_without_passengers_is_simply_released() -> TestResult {
    let mut g = sea_world();
    let empty = spawn(&mut g, "galley", at(15, 4));
    g.release_troops(P0, &[empty], &[])
        .map_err(|e| format!("{e:?}"))?;
    assert!(!g.world.contains(empty));
    let docked = spawn(&mut g, "galley", at(2, 4));
    let rider = spawn(&mut g, "high_men_cavalry", at(2, 4));
    let serial = g.units().serial_of(docked);
    let _ = g
        .world
        .insert_one(rider, crate::unit::transport::Aboard(serial));
    g.release_troops(P0, &[docked], &[])
        .map_err(|e| format!("{e:?}"))?;
    assert!(!g.world.contains(docked));
    assert!(g.economy().carrier_of(rider).is_none());
    assert_eq!(g.units().unit_position(rider), Some(at(2, 4)));
    Ok(())
}

#[test]
fn a_ship_with_no_free_shore_loses_its_passengers() -> TestResult {
    let mut g = sea_world();
    let boat = spawn(&mut g, "galley", at(15, 8));
    let rider = spawn(&mut g, "high_men_cavalry", at(15, 8));
    let serial = g.units().serial_of(boat);
    let _ = g
        .world
        .insert_one(rider, crate::unit::transport::Aboard(serial));
    g.release_troops(P0, &[boat], &[])
        .map_err(|e| format!("{e:?}"))?;
    // Every land square is taken by an enemy.
    let u = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    for x in 0..30 {
        for y in 0..16 {
            if !g.economy().is_sea(at(x, y)) {
                g.spawn_unit(crate::player::PlayerId(1), at(x, y), u);
            }
        }
    }
    let events = g.sail_marooned();
    assert!(!g.world.contains(boat) && !g.world.contains(rider));
    assert!(events.contains(&Event::UnitsDrowned {
        player: P0,
        units: 1
    }));
    Ok(())
}

#[test]
fn only_the_wizards_own_units_can_be_released() -> TestResult {
    let mut g = sea_world();
    let theirs = {
        let u = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        g.spawn_unit(crate::player::PlayerId(1), at(1, 1), u)
    };
    assert!(g.release_troops(P0, &[theirs], &[]).is_err());
    assert!(g.world.contains(theirs));
    assert!(g.release_troops(P0, &[], &[ItemSpot::Vault(0)]).is_err());
    let _ = MovesLeft(0);
    Ok(())
}

#[test]
fn a_unit_let_go_for_food_is_not_counted_for_gold() {
    let mut world = hecs::World::new();
    let (a, b) = (world.spawn(()), world.spawn(()));
    let line = |unit, gold, food| TroopLine {
        unit,
        label: String::new(),
        gold,
        food,
        hero: false,
        cost: 0,
        ship: false,
        aboard: 0,
    };
    let short = TreasuryShortfall {
        food_upkeep: 2,
        food_surplus: 1,
        gold: 1,
        fame: 0,
        troops: vec![line(a, 1, 1), line(b, 1, 1)],
        default_release: vec![b],
    };
    assert_eq!(short.missing(&[]), (1, 1));
    // One unit lets go of a mouth and of a gold coin at once: both books balance.
    assert_eq!(short.missing(&[b]), (0, 0));
}

#[test]
fn a_marooned_ship_survives_saving() -> TestResult {
    let mut g = sea_world();
    let boat = spawn(&mut g, "galley", at(15, 8));
    let rider = spawn(&mut g, "high_men_cavalry", at(15, 8));
    let serial = g.units().serial_of(boat);
    let _ = g
        .world
        .insert_one(rider, crate::unit::transport::Aboard(serial));
    g.release_troops(P0, &[boat], &[])
        .map_err(|e| format!("{e:?}"))?;
    let content = g.content.clone();
    let text = g.snapshot().to_text()?;
    let back = crate::game::save::Snapshot::from_text(&text)?.restore(content)?;
    let ships: Vec<_> = back
        .world
        .query::<&Marooned>()
        .iter()
        .map(|(e, _)| e)
        .collect();
    assert_eq!(ships.len(), 1);
    assert_eq!(back.units().unit_owner(ships[0]), Some(MONSTERS));
    Ok(())
}
