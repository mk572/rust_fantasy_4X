use super::*;
use crate::game::testutil::content_with_economy;
use crate::game::Command;
use crate::map::{Grid, Plane};

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// A small lived-in world: two cities and an outpost, an army with an order,
/// roads, a mine, explored land, and some turns played.
fn game() -> Game {
    let c = content_with_economy();
    let grass = c.terrains.id_of("grassland").unwrap();
    let ocean = c.terrains.id_of("ocean").unwrap();
    let hills = c.terrains.id_of("hills").unwrap();
    let gold = c.features.id_of("gold").unwrap();
    let build = || {
        let mut g = Grid::new(24, 14, grass);
        for y in 0..14 {
            g.set(0, y, ocean);
        }
        g.set(5, 5, hills);
        g.set_mineral(10, 8, Some(gold));
        g.set_road(12, 6, true);
        g.set_road(13, 6, true);
        g.set_river(14, 6, true);
        g.set_lake(16, 6, true);
        g.set_corrupted(17, 6, true);
        g
    };
    let map = Map::new(build(), build());
    let node = c.features.id_of("node").unwrap();
    let mut g = Game::new(99);
    g.content = c;
    g.settings.basic.magic_strength = 2;
    g.settings.basic.difficulty = 4;
    g.map = Some(map);
    g.add_player(P0, "Merlin");
    g.treasury_mut(P0).unwrap().gold = 300;
    g.player_component_mut::<crate::player::Tax>(P0).unwrap().0 = 2;
    g.set_grand_vizier(P0, true);
    g.add_player(PlayerId(1), "Rival");
    let magic_node = g.spawn_feature(crate::feature::entity::Placement {
        at: Position {
            plane: Plane::Myrror,
            x: 3,
            y: 3,
        },
        feature: node,
        node: Some(crate::feature::node::Node {
            realm: "chaos".into(),
            power: 12,
            aura: vec![(3, 3), (4, 3), (3, 4)],
            guardian: true,
            warped: false,
        }),
        lair: None,
    });
    g.set_feature_owner(magic_node, P0);
    g.spawn_volcano(P0, at(15, 6));
    let elf = g.content.rules.races.id_of("elf");
    let dwarf = g.content.rules.races.id_of("dwarf");
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 6),
        name: "Home".into(),
        race: elf,
        population: 4,
    });
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(18, 6),
        name: "Delve".into(),
        race: dwarf,
        population: 2,
    });
    g.apply(Command::FoundCity {
        owner: P0,
        pos: at(10, 11),
        name: "Camp".into(),
        race: elf,
        population: 0,
    });
    let spear = g.content.rules.units.id_of("spear").unwrap();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    for _ in 0..2 {
        g.spawn_unit(P0, at(10, 6), spear);
    }
    let scout = g.spawn_unit(P0, at(10, 6), walker);
    g.apply(Command::GoTo {
        units: vec![scout],
        to: at(22, 2),
    });
    let hall = g.content.rules.buildings.id_of("builders_hall").unwrap();
    let home = g.economy().city_at(at(10, 6)).unwrap();
    g.apply(Command::SetProduction {
        city: home,
        item: ProductionItem::Building(hall),
    });
    for _ in 0..4 {
        g.apply(Command::EndTurn);
    }
    g
}

fn city_named(g: &Game, name: &str) -> hecs::Entity {
    g.economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.name == name)
        .unwrap()
        .0
}

#[test]
fn a_snapshot_survives_text() {
    let snap = game().snapshot();
    let text = snap.to_text().unwrap();
    assert!(text.starts_with("// f4x save, version 2"));
    assert_eq!(Snapshot::from_text(&text).unwrap(), snap);
}

#[test]
fn restoring_gives_the_same_game() {
    let g = game();
    let snap = g.snapshot();
    let back = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(g.content.clone())
        .unwrap();
    assert_eq!(back.snapshot(), snap);
    assert_eq!(back.turn, g.turn);
    assert_eq!(back.economy().cities().len(), 3);
    assert_eq!(back.units().all_units().len(), g.units().all_units().len());
    assert_eq!(
        back.player(P0).unwrap().treasury().gold,
        g.player(P0).unwrap().treasury().gold
    );
    assert!(back.player(P0).is_some_and(|p| p.has_grand_vizier()));
    // Details that are easy to lose: roads, features, exploration, orders, the outpost.
    let grid = back.map.as_ref().unwrap().plane(Plane::Arcanus);
    assert!(grid.has_road(12, 6) && grid.has_road(13, 6) && !grid.has_road(14, 6));
    assert!(grid.has_river(14, 6) && !grid.has_river(13, 6));
    assert_eq!(back.economy().volcano_at(at(15, 6)), Some(PlayerId(0)));
    assert_eq!(back.economy().volcano_at(at(14, 6)), None);
    assert!(grid.is_lake(16, 6) && !grid.is_lake(15, 6));
    assert!(grid.is_corrupted(17, 6) && !grid.is_corrupted(16, 6));
    let nodes = |game: &Game| {
        game.economy()
            .nodes()
            .into_iter()
            .map(|n| {
                (
                    n.pos, n.realm, n.power, n.aura, n.owner, n.guardian, n.warped,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(nodes(&back), nodes(&g));
    assert_eq!(nodes(&g).len(), 1);
    assert_eq!(back.settings.basic.magic_strength, 2);
    assert_eq!(back.settings.basic.difficulty, 4);
    assert!(grid.mineral(10, 8).is_some());
    assert_eq!(
        back.fog.explored_count(P0, Plane::Arcanus),
        g.fog.explored_count(P0, Plane::Arcanus)
    );
    let walkers_with_orders = back
        .units()
        .all_units()
        .into_iter()
        .filter(|&u| back.world.get::<&Order>(u).is_ok())
        .count();
    assert_eq!(walkers_with_orders, 1);
    let camp = city_named(&back, "Camp");
    assert_eq!(back.world.get::<&Population>(camp).unwrap().figures, 0);
}

#[test]
fn the_random_stream_resumes_where_it_stopped() {
    let mut g = game();
    assert!(
        g.rng.draws() > 0,
        "the world used random numbers (outposts, healing)"
    );
    let mut back = g.snapshot().restore(g.content.clone()).unwrap();
    for _ in 0..5 {
        assert_eq!(g.next_u64(), back.next_u64());
    }
}

#[test]
fn a_loaded_game_continues_exactly_like_the_original() {
    let mut original = game();
    // Make entity ids get reused: remove a unit, then create another.
    let spear = original.content.rules.units.id_of("spear").unwrap();
    let victim = original.units().units_at(at(10, 6))[0];
    original.world.despawn(victim).unwrap();
    original.spawn_unit(P0, at(11, 6), spear);
    let mut loaded = original
        .snapshot()
        .restore(original.content.clone())
        .unwrap();
    for turn in 0..12 {
        let (a, b) = (
            original.apply(Command::EndTurn),
            loaded.apply(Command::EndTurn),
        );
        assert_eq!(a.len(), b.len(), "same events on turn {turn}");
        // Same command to both games, found by name rather than entity id.
        if turn == 3 {
            for g in [&mut original, &mut loaded] {
                let delve = city_named(g, "Delve");
                let hall = g.content.rules.buildings.id_of("builders_hall").unwrap();
                g.apply(Command::SetProduction {
                    city: delve,
                    item: ProductionItem::Building(hall),
                });
                g.apply(Command::BuyProduction { city: delve });
            }
        }
        assert_eq!(
            original.snapshot(),
            loaded.snapshot(),
            "diverged on turn {turn}"
        );
    }
}

#[test]
fn saves_do_not_depend_on_the_order_of_the_content() {
    let g = game();
    let snap = g.snapshot();
    let mut shuffled = g.content.clone();
    let mut units = shuffled.rules.units.defs().to_vec();
    units.reverse();
    shuffled.rules.units = crate::rules::Registry::new(units);
    let mut buildings = shuffled.rules.buildings.defs().to_vec();
    buildings.reverse();
    shuffled.rules.buildings = crate::rules::Registry::new(buildings);
    let back = snap.clone().restore(shuffled).unwrap();
    assert_eq!(
        back.snapshot(),
        snap,
        "the same game under a different table order"
    );
}

#[test]
fn missing_content_is_reported_by_name() {
    let g = game();
    let snap = g.snapshot();
    let mut without = g.content.clone();
    let units: Vec<_> = without
        .rules
        .units
        .defs()
        .iter()
        .filter(|u| u.id != "spear")
        .cloned()
        .collect();
    without.rules.units = crate::rules::Registry::new(units);
    assert_eq!(
        snap.clone().restore(without).err(),
        Some(LoadError::UnknownId("unit", "spear".into()))
    );
    let mut no_terrain = g.content.clone();
    let terrains: Vec<_> = no_terrain
        .terrains
        .defs()
        .iter()
        .filter(|t| t.id != "hills")
        .cloned()
        .collect();
    no_terrain.terrains = crate::map::terrain::Terrains::new(terrains);
    assert_eq!(
        snap.restore(no_terrain).err(),
        Some(LoadError::UnknownId("terrain", "hills".into()))
    );
}

#[test]
fn a_save_carries_the_game_version() {
    let snap = game().snapshot();
    assert_eq!(snap.game_version, crate::game::save::GAME_VERSION);
    let text = snap.to_text().unwrap();
    assert!(text.contains(&format!("game {}", crate::game::save::GAME_VERSION)));
    let back = Snapshot::from_text(&text).unwrap();
    assert_eq!(back.game_version, crate::game::save::GAME_VERSION);
}

#[test]
fn unreadable_and_foreign_saves_are_refused() {
    assert!(matches!(
        Snapshot::from_text("not a save"),
        Err(LoadError::Parse(_))
    ));
    let mut snap = game().snapshot();
    snap.version = 99;
    let text = ron::to_string(&snap).unwrap();
    assert_eq!(
        Snapshot::from_text(&text).err(),
        Some(LoadError::Version {
            found: 99,
            supported: SAVE_VERSION
        })
    );
}

#[test]
fn contradictory_saves_are_refused() {
    let g = game();
    let content = g.content.clone();
    let base = g.snapshot();

    let mut dup = base.clone();
    dup.units[1].serial = dup.units[0].serial;
    assert!(
        matches!(dup.restore(content.clone()), Err(LoadError::Inconsistent(m)) if m.contains("creation number"))
    );

    let mut off = base.clone();
    off.units[0].pos = at(500, 3);
    assert!(
        matches!(off.restore(content.clone()), Err(LoadError::Inconsistent(m)) if m.contains("off the map"))
    );

    let mut farm = base.clone();
    farm.cities[0].farmers = farm.cities[0].figures + 5;
    assert!(matches!(
        farm.restore(content.clone()),
        Err(LoadError::Inconsistent(_))
    ));

    let mut fog = base.clone();
    fog.fog.players[0].1.pop();
    assert!(matches!(
        fog.restore(content.clone()),
        Err(LoadError::Inconsistent(_))
    ));

    let mut twice = base;
    twice.players.push(twice.players[0].clone());
    assert!(matches!(
        twice.restore(content),
        Err(LoadError::Inconsistent(_))
    ));
}

#[test]
fn an_empty_game_round_trips() {
    let g = Game::new(5);
    let snap = g.snapshot();
    let back = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(Default::default())
        .unwrap();
    assert_eq!(back.snapshot(), snap);
}

#[test]
fn every_setting_survives_a_save() {
    let mut g = Game::new(5);
    g.settings.basic = crate::game::setting::BasicSettings {
        difficulty: 3,
        opponents: 4,
        land_percent: 15,
        magic_strength: 0,
        map_width: 75,
        map_height: 50,
        diagonal_extra_cost: true,
        original_map_generator: true,
    };
    let text = g.snapshot().to_text().unwrap();
    let back = Snapshot::from_text(&text)
        .unwrap()
        .restore(Default::default())
        .unwrap();
    assert_eq!(back.settings(), g.settings());
}

#[test]
fn a_save_with_out_of_range_settings_is_brought_into_range() {
    let mut snap = Game::new(5).snapshot();
    snap.settings.basic.difficulty = 9;
    let back = snap.restore(Default::default()).unwrap();
    assert_eq!(back.settings().basic.difficulty, 4);
}

#[test]
fn weapon_quality_survives_a_save() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let unit = g.spawn_unit(P0, at(11, 6), walker);
    let _ = g.world.insert_one(
        unit,
        crate::unit::WeaponQuality(crate::unit::WEAPON_MITHRIL),
    );
    let snap = g.snapshot();
    let back = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(g.content.clone())
        .unwrap();
    let again = back.units().units_at(at(11, 6)).into_iter().next().unwrap();
    assert_eq!(
        back.units().unit_weapon_quality(again),
        crate::unit::WEAPON_MITHRIL
    );
}

#[test]
fn an_undead_unit_stays_undead_through_a_save() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let unit = g.spawn_unit(P0, at(11, 6), walker);
    g.make_undead(unit);
    let snap = g.snapshot();
    let back = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(g.content.clone())
        .unwrap();
    let again = back.units().units_at(at(11, 6)).into_iter().next().unwrap();
    assert!(back.units().unit_is_undead(again));
    assert_eq!(back.snapshot(), snap);
}

#[test]
fn creation_numbers_keep_counting_after_a_load() {
    let g = game();
    let next = g.next_serial;
    let mut back = g.snapshot().restore(g.content.clone()).unwrap();
    let walker = back.content.rules.units.id_of("walker").unwrap();
    let fresh = back.spawn_unit(P0, at(11, 6), walker);
    assert_eq!(back.serial_of(fresh), Serial(next));
}

#[test]
fn bit_packing_round_trips() {
    for len in [0usize, 1, 7, 8, 9, 100] {
        let bits: Vec<bool> = (0..len).map(|i| i % 3 == 0).collect();
        assert_eq!(unpack(&pack(&bits), len), Some(bits));
    }
    assert_eq!(unpack(&[0, 0], 8), None);
}

#[test]
fn a_unit_in_stasis_stays_there_through_a_save() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let unit = g.spawn_unit(P0, at(11, 6), walker);
    let _ = g
        .world
        .insert_one(unit, crate::unit::stasis::Stasis::Lingering);
    let snap = g.snapshot();
    let back = Snapshot::from_text(&snap.to_text().unwrap())
        .unwrap()
        .restore(g.content.clone())
        .unwrap();
    let again = back.units().units_at(at(11, 6)).into_iter().next().unwrap();
    assert!(back.unit_in_stasis(again));
    assert_eq!(back.snapshot(), snap);
}

#[test]
fn a_lasting_spell_on_the_world_stays_through_a_save() {
    let mut g = game();
    g.content = crate::game::testutil::content_with_magic();
    let spell = g
        .content
        .rules
        .spells
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    g.add_global(P0, spell);
    let serial = g.wizard(P0).unwrap().global_instances()[0].1;
    let back = g.snapshot().restore(g.content.clone()).unwrap();
    assert_eq!(
        back.wizard(P0).unwrap().global_instances(),
        vec![(spell, serial)]
    );
    assert_eq!(back.snapshot(), g.snapshot());
}

#[test]
fn a_spell_on_a_city_stays_through_a_save() {
    let mut g = game();
    g.content = crate::game::testutil::content_with_magic();
    let spell = g
        .content
        .rules
        .spells
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    let city = g
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == P0)
        .unwrap()
        .0;
    g.add_city_enchantment(city, spell, P0);
    let back = g.snapshot().restore(g.content.clone()).unwrap();
    let again = back
        .economy()
        .cities()
        .into_iter()
        .find(|(_, c, _)| c.owner == P0)
        .unwrap()
        .0;
    assert_eq!(back.city_enchantments(again), vec![(spell, P0)]);
    assert_eq!(back.snapshot(), g.snapshot());
}

#[test]
fn a_spell_on_a_unit_stays_through_a_save() {
    let mut g = game();
    g.content = crate::game::testutil::content_with_magic();
    let spell = g
        .content
        .rules
        .spells
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    let unit = g.units().player_units(P0)[0];
    g.add_unit_enchantment(unit, spell, P0);
    let back = g.snapshot().restore(g.content.clone()).unwrap();
    let again = back.units().player_units(P0)[0];
    assert_eq!(back.units().unit_enchantments(again), vec![(spell, P0)]);
    assert_eq!(back.snapshot(), g.snapshot());
}

#[test]
fn a_save_from_another_game_version_is_refused_by_name() {
    let text = game().snapshot().to_text().unwrap();
    let old = text.replace(
        &format!("game_version: \"{}\"", crate::game::save::GAME_VERSION),
        "game_version: \"0.0.1\"",
    );
    assert_ne!(old, text);
    let err = Snapshot::from_text(&old).err().unwrap();
    assert_eq!(
        err,
        LoadError::GameVersion {
            found: "0.0.1".into(),
            supported: crate::game::save::GAME_VERSION
        }
    );
    assert_eq!(
        err.to_string(),
        format!(
            "saved by 0.0.1, this is {}",
            crate::game::save::GAME_VERSION
        )
    );
}

#[test]
fn a_save_of_another_layout_is_refused_for_its_version_not_its_parse() {
    // Fields this layout does not have: the header is read first.
    let text = "Snapshot(version: 1, game_version: \"0.0.1\", something_else: 5)";
    assert_eq!(
        Snapshot::from_text(text).err(),
        Some(LoadError::Version {
            found: 1,
            supported: SAVE_VERSION
        })
    );
}

#[test]
fn a_replay_of_another_game_version_is_refused() {
    let mut g = game();
    g.start_recording().unwrap();
    let text = g.stop_recording().unwrap().to_text().unwrap();
    let old = text.replace(
        &format!("game_version: \"{}\"", crate::game::save::GAME_VERSION),
        "game_version: \"0.0.1\"",
    );
    let err = crate::game::replay::Replay::from_text(&old).err().unwrap();
    assert!(format!("{err:?}").contains("saved by 0.0.1"), "{err:?}");
}

#[test]
fn an_edited_save_is_noticed_and_still_loads() {
    use crate::game::save::Integrity;
    let text = game().snapshot().to_text().unwrap();
    assert_eq!(Snapshot::integrity(&text), Integrity::Intact);
    let edited = text.replacen("turn: ", "turn: 1", 1);
    assert_eq!(Snapshot::integrity(&edited), Integrity::Edited);
    assert!(Snapshot::from_text(&edited).is_ok());
    let unsigned = text.replacen(", checksum", ", sum", 1);
    assert_eq!(Snapshot::integrity(&unsigned), Integrity::Unsigned);
    assert!(Snapshot::from_text(&unsigned).is_ok());
}

#[test]
fn a_wizard_without_a_player_is_refused() {
    let g = game();
    let mut snap = g.snapshot();
    let mut extra = snap.wizards[0].clone();
    extra.player = 200;
    snap.wizards.push(extra);
    assert!(matches!(
        snap.restore(g.content.clone()).err(),
        Some(LoadError::Inconsistent(_))
    ));
}
