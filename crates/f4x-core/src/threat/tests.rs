use super::*;
use crate::feature::lair::Lair;
use crate::game::testutil::shipped;

use crate::game::Command;
use crate::map::{Grid, Map, Plane};
use crate::player::{PlayerId, MONSTERS};
use crate::rules::{RealmId, Stat, UnitId};
use f4x_def::testdir::TestResult;

const P0: PlayerId = PlayerId(0);
const TOWN: PlayerId = PlayerId(100);

/// A raider clock one tick below its limit (`threat.raider_limit` of `rules.ron`).
fn one_short(g: &Game) -> u32 {
    g.content.rules.config.threat.raider_limit - 1
}

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// A grassland world of 24 by 14 with a wizard at (10, 4) and an independent city at (4, 4)
/// that has `troops` spearmen. Raiders and monsters are switched on and Impossible is played.
fn world(troops: usize) -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(4 /* grassland */)
        .expect("id_of succeeds");
    let mut g = Game::new(11);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    g.add_player(P0, "W");
    g.ensure_human(P0);
    g.settings.basic.difficulty = 4;
    g.settings.world.raiders_enabled = true;
    g.settings.world.monsters_enabled = true;
    g.settings.world.raider_percent = 50;
    g.spawn_city(P0, at(10, 4), "Capital".into(), None, 5);
    g.add_neutral_player(TOWN);
    g.spawn_city(TOWN, at(4, 4), "Free Town".into(), None, 5);
    let spear = g
        .content
        .rules
        .units
        .id_numbered(104)
        .expect("id_of succeeds");
    for _ in 0..troops {
        g.spawn_unit(TOWN, at(4, 4), spear);
    }
    g
}

fn lair(guards: Vec<(u16, u32)>) -> Lair {
    Lair {
        intact: true,
        guards: guards
            .into_iter()
            .map(|(g, n)| (UnitId::new(g), n))
            .collect(),
        gold: 120,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
        fought_by: vec![P0],
        hurt: Vec::new(),
    }
}

#[test]
fn the_raider_clock_runs_to_its_limit_and_a_city_raises_free_troops_and_goes_to_war() {
    let mut g = world(4);
    g.threat.raider_clock = one_short(&g);
    let events = g.process_threats();
    assert_eq!(g.threat.raider_clock, 0, "the clock starts again");
    let Some(Event::RaidersRaised { city, owner, count }) = events.first().cloned() else {
        panic!("raiders were raised: {events:?}");
    };
    assert_eq!(owner, TOWN);
    assert_eq!(count, 2, "half of four troops");
    assert_eq!(g.economy().city_at(at(4, 4)), Some(city));
    let all = g.units().player_units(TOWN);
    assert_eq!(all.len(), 6);
    let free: Vec<_> = all
        .iter()
        .filter(|&&u| g.units().unit_is_unpaid(u))
        .collect();
    assert_eq!(free.len(), 2);
    assert!(free.iter().all(|&&u| g.units().unit_gold_upkeep(u) == 0));
}

#[test]
fn below_the_limit_nothing_happens() {
    let mut g = world(4);
    g.threat.raider_clock = 0;
    assert!(g.process_threats().is_empty());
    assert!((1..=5).contains(&g.threat.raider_clock));
}

#[test]
fn a_neutral_city_alone_on_its_landmass_raises_nobody_and_feeds_the_monsters_clock() -> TestResult {
    let mut g = world(4);
    let sea = g
        .content
        .terrains
        .id_numbered(8 /* ocean */)
        .ok_or("ocean is not in the content")?;
    // A strip of sea between the two cities.
    for y in 0..14 {
        for x in [7, 20] {
            g.map
                .as_mut()
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(x, y, sea);
        }
    }
    g.threat.raider_clock = one_short(&g);
    let before = g.threat.monster_clock;
    assert!(g.process_threats().is_empty());
    assert_eq!(g.threat.monster_clock, before + 15);
    Ok(())
}

#[test]
fn a_full_city_has_no_room_for_a_party() {
    let mut g = world(0);
    let spear = g
        .content
        .rules
        .units
        .id_numbered(104)
        .expect("id_of succeeds");
    for _ in 0..g.max_tile() {
        g.spawn_unit(TOWN, at(4, 4), spear);
    }
    g.threat.raider_clock = one_short(&g);
    let events = g.process_threats();
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::RaidersRaised { .. })));
}

#[test]
fn with_the_setting_off_the_clock_does_not_move() {
    let mut g = world(4);
    g.settings.world.raiders_enabled = false;
    g.settings.world.monsters_enabled = false;
    g.threat.raider_clock = one_short(&g);
    assert!(g.process_threats().is_empty());
    assert_eq!(
        g.threat,
        Threat {
            raider_clock: one_short(&g),
            monster_clock: 0
        }
    );
}

/// The number of a creature of a realm the monsters do come from, and the realm's number.
fn unspared_guard(g: &Game) -> Result<(u16, u16), String> {
    guard_of_realms(g, false)
}

/// The same for a realm that keeps its monsters at home.
fn spared_guard(g: &Game) -> Result<(u16, u16), String> {
    guard_of_realms(g, true)
}

fn guard_of_realms(g: &Game, spared_realm: bool) -> Result<(u16, u16), String> {
    let spared = &g.content.rules.config.threat.spared_realms;
    g.content
        .rules
        .units
        .iter()
        .filter(|(_, d)| d.kind == crate::rules::UnitKind::Summoned)
        .filter_map(|(id, d)| Some((id.num(), d.realm?.num())))
        .find(|(_, realm)| spared.contains(realm) == spared_realm)
        .ok_or("a creature of the realm asked for".into())
}

#[test]
fn a_new_stack_takes_a_lair_and_the_guards_that_lived_there_are_driven_out() -> TestResult {
    let mut g = world(0);
    g.settings.world.raiders_enabled = false;
    g.turn = 100;
    let (guard, realm) = unspared_guard(&g)?;
    let site = g.put_lair(
        at(14, 8),
        13, /* monster_lair */
        lair(vec![(guard, 2)]),
    );
    g.threat.monster_clock = 1000;
    let events = g.process_threats();
    let Some(Event::MonstersSettled {
        pos,
        count,
        evicted,
        ..
    }) = events.first().cloned()
    else {
        return Err(format!("monsters settled: {events:?}").into());
    };
    assert_eq!(pos, at(14, 8));
    assert!(count >= 1);
    assert_eq!(evicted, 2);
    let lair = g.economy().lair_of(site).ok_or("lair_of is missing")?;
    assert_eq!(lair.guards.iter().map(|(_, n)| n).sum::<u32>(), count);
    assert!(lair.guards.iter().all(|(id, _)| {
        let def = g.content.rules.units.get(*id);
        def.kind == crate::rules::UnitKind::Summoned && def.realm.map(|r| r.num()) == Some(realm)
    }));
    assert!(lair.fought_by.is_empty(), "nobody knows the new guards");
    let roaming = g.units().player_units(MONSTERS);
    assert_eq!(roaming.len(), 2);
    let square = g
        .units()
        .unit_position(roaming[0])
        .ok_or("unit_position is missing")?;
    assert_ne!(square, at(14, 8));
    assert!(roaming
        .iter()
        .all(|&u| g.units().unit_position(u) == Some(square)));
    assert_eq!(g.threat.monster_clock, 0);
    Ok(())
}

#[test]
fn life_lairs_keep_their_guards() -> TestResult {
    let mut g = world(0);
    g.turn = 100;
    let (guard, _) = spared_guard(&g)?;
    g.put_lair(
        at(14, 8),
        13, /* monster_lair */
        lair(vec![(guard, 1)]),
    );
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
    assert_eq!(
        g.threat.monster_clock, 1000,
        "no lair can send monsters, so the clock waits"
    );
    Ok(())
}

#[test]
fn nothing_comes_before_the_first_monster_turn_and_the_clock_starts_again() -> TestResult {
    let mut g = world(0);
    g.turn = g.monster_first_turn() - 1;
    let (guard, _) = unspared_guard(&g)?;
    g.put_lair(
        at(14, 8),
        13, /* monster_lair */
        lair(vec![(guard, 2)]),
    );
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
    assert_eq!(g.threat.monster_clock, 0);
    Ok(())
}

#[test]
fn a_lair_on_a_landmass_without_a_player_is_not_chosen() -> TestResult {
    let mut g = world(0);
    g.turn = 100;
    let sea = g
        .content
        .terrains
        .id_numbered(8 /* ocean */)
        .ok_or("ocean is not in the content")?;
    for y in 0..14 {
        for x in [12, 20] {
            g.map
                .as_mut()
                .ok_or("as_mut is missing")?
                .plane_mut(Plane::Arcanus)
                .set(x, y, sea);
        }
    }
    g.put_lair(
        at(16, 8),
        13,                   /* monster_lair */
        lair(vec![(165, 2)]), /* skeletons */
    );
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
    Ok(())
}

#[test]
fn roaming_monsters_settle_in_a_cleared_node_nobody_holds() -> TestResult {
    let mut g = world(0);
    let site = g.put_lair(
        at(14, 8),
        20, /* sorcery_node */
        Lair {
            intact: false,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: vec![P0],
            hurt: Vec::new(),
        },
    );
    let ghoul = g
        .content
        .rules
        .units
        .id_numbered(166)
        .ok_or("ghouls is not in the content")?;
    let stack = vec![
        g.spawn_unit(MONSTERS, at(14, 7), ghoul),
        g.spawn_unit(MONSTERS, at(14, 7), ghoul),
    ];
    let events = g.apply(Command::MoveStack {
        units: stack.clone(),
        to: at(14, 8),
    });
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MonstersSettled {
                count: 2,
                evicted: 0,
                ..
            }
        )),
        "{events:?}"
    );
    let lair = g.economy().lair_of(site).ok_or("lair_of is missing")?;
    assert!(lair.intact);
    assert_eq!(lair.guards, vec![(UnitId::new(166) /* ghouls */, 2)]);
    assert!(
        g.units().player_units(MONSTERS).is_empty(),
        "the units became the guards"
    );
    Ok(())
}

#[test]
fn a_stack_that_settles_on_the_way_does_not_walk_on() -> TestResult {
    let mut g = world(0);
    g.put_lair(
        at(14, 8),
        20, /* sorcery_node */
        Lair {
            intact: false,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: vec![P0],
            hurt: Vec::new(),
        },
    );
    let ghoul = g
        .content
        .rules
        .units
        .id_numbered(166)
        .ok_or("ghouls is not in the content")?;
    let unit = g.spawn_unit(MONSTERS, at(14, 7), ghoul);
    g.world
        .insert_one(unit, crate::unit::Order::GoTo(vec![at(14, 8), at(14, 9)]))?;
    let events = g.follow_orders(&[unit]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MonstersSettled { count: 1, .. })),
        "{events:?}"
    );
    assert!(g.units().player_units(MONSTERS).is_empty());
    Ok(())
}

#[test]
fn a_node_a_wizard_holds_is_not_taken() -> TestResult {
    let mut g = world(0);
    let node = crate::feature::node::Node {
        realm: RealmId::new(2 /* chaos */),
        power: 5,
        aura: vec![(14, 8)],
        guardian: false,
        warped: false,
    };
    let e = g.put_node_data(at(14, 8), 20 /* sorcery_node */, node, Some(P0));
    g.world.insert_one(
        e,
        Lair {
            intact: false,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: Vec::new(),
            hurt: Vec::new(),
        },
    )?;
    assert_eq!(g.vacant_zone(at(14, 8)), None);
    Ok(())
}

#[test]
fn monsters_that_win_a_city_wreck_it_and_vanish_and_sometimes_leave_ruins() -> TestResult {
    let mut ruined = 0;
    let mut kept = 0;
    for seed in 0..40 {
        let mut g = world(0);
        g.rng = crate::game::rng::GameRng::new(seed);
        let ghoul = g
            .content
            .rules
            .units
            .id_numbered(166)
            .ok_or("ghouls is not in the content")?;
        let stack = vec![
            g.spawn_unit(MONSTERS, at(10, 5), ghoul),
            g.spawn_unit(MONSTERS, at(10, 5), ghoul),
        ];
        // A second city, so the wizard's last one is not what falls: the fortress is spared.
        g.spawn_city(P0, at(16, 9), "Colony".into(), None, 6);
        let colony = g.economy().city_at(at(16, 9)).ok_or("city_at is missing")?;
        for &m in &stack {
            g.world.insert_one(m, at(16, 8))?;
        }
        let events = g.apply(Command::MoveStack {
            units: stack.clone(),
            to: at(16, 9),
        });
        let Some(Event::CityRampaged {
            pos,
            owner,
            ruined: gone,
            ..
        }) = events
            .iter()
            .find(|e| matches!(e, Event::CityRampaged { .. }))
            .cloned()
        else {
            return Err(format!("a rampage: {events:?}").into());
        };
        assert_eq!((pos, owner), (at(16, 9), P0));
        if gone {
            ruined += 1;
            assert!(
                g.world.get::<&crate::city::City>(colony).is_err(),
                "the city is gone"
            );
            let lair = g
                .economy()
                .lair_at(at(16, 9))
                .expect("the ruins hold a lair");
            assert_eq!(lair.guards, vec![(UnitId::new(166) /* ghouls */, 2)]);
            assert!(g.units().player_units(MONSTERS).is_empty());
        } else {
            kept += 1;
            assert_eq!(
                g.economy().city_owner(colony),
                Some(P0),
                "still the wizard's"
            );
            assert!(
                g.units().player_units(MONSTERS).is_empty(),
                "they vanish into the wilderness"
            );
        }
    }
    assert!(ruined > 5 && kept > 5, "{ruined} ruined, {kept} kept");
    Ok(())
}

#[test]
fn monsters_that_beat_the_defenders_in_a_battle_rampage_through_the_city() -> TestResult {
    let mut rampaged = 0;
    for seed in 0..12 {
        let mut g = world(0);
        g.rng = crate::game::rng::GameRng::new(seed);
        let spear = g
            .content
            .rules
            .units
            .id_numbered(104)
            .ok_or("high_men_spearmen is not in the content")?;
        let skeleton = g
            .content
            .rules
            .units
            .id_numbered(165)
            .ok_or("skeletons is not in the content")?;
        g.spawn_city(P0, at(16, 9), "Colony".into(), None, 6);
        let defenders = vec![g.spawn_unit(P0, at(16, 9), spear)];
        let monsters: Vec<_> = (0..9)
            .map(|_| g.spawn_unit(MONSTERS, at(16, 8), skeleton))
            .collect();
        let events = g.apply(Command::AutoResolve {
            mana_budget: None,
            attackers: monsters,
            defenders,
        });
        if let Some(Event::CityRampaged { ruined, .. }) = events
            .iter()
            .find(|e| matches!(e, Event::CityRampaged { .. }))
        {
            rampaged += 1;
            assert!(g.units().player_units(MONSTERS).is_empty());
            let city = g.economy().city_at(at(16, 9));
            if *ruined {
                assert!(city.is_none(), "a ruined city is gone");
            } else {
                let city = city.ok_or("a kept city is still there")?;
                assert_eq!(g.economy().city_owner(city), Some(P0));
            }
        }
    }
    assert!(rampaged > 6, "nine skeletons beat one spearman: {rampaged}");
    Ok(())
}

#[test]
fn the_clocks_and_the_free_troops_survive_a_save() -> TestResult {
    let mut g = world(4);
    g.threat.raider_clock = one_short(&g);
    g.process_threats();
    g.threat.monster_clock = 7;
    let text = g.snapshot();
    let back = text.restore(g.content.clone())?;
    assert_eq!(back.threat, g.threat);
    let free = back
        .units()
        .player_units(TOWN)
        .into_iter()
        .filter(|&u| back.units().unit_is_unpaid(u))
        .count();
    assert_eq!(free, 2);
    Ok(())
}

#[test]
fn the_free_troops_cost_their_city_nothing() {
    let mut g = world(4);
    let before = g.budget(TOWN).unit_upkeep;
    g.threat.raider_clock = one_short(&g);
    g.process_threats();
    assert_eq!(g.budget(TOWN).unit_upkeep, before);
    let _ = Stat::Melee;
}

/// The party that `world(troops)` raises at the independent city, counted.
fn party(g: &mut Game) -> usize {
    g.threat.raider_clock = one_short(g);
    match g.process_threats().first() {
        Some(Event::RaidersRaised { count, .. }) => *count as usize,
        _ => 0,
    }
}

#[test]
fn the_party_is_a_third_smaller_beside_an_ai_wizards_fortress_but_not_a_humans() -> TestResult {
    let mut plain = world(8);
    assert_eq!(party(&mut plain), 4, "half of eight");
    let mut g = world(8);
    let rival = PlayerId(1);
    g.add_player(rival, "Rival");
    g.place_fortress(rival, at(14, 8), "Keep".into(), None, 5)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(party(&mut g), 2, "two thirds of four");
    Ok(())
}

#[test]
fn a_party_on_myrror_is_halved_until_turn_two_hundred() {
    let on_myrror = |turn: u32| {
        let mut g = world(0);
        let myr = |x, y| Position {
            plane: Plane::Myrror,
            x,
            y,
        };
        g.spawn_city(P0, myr(10, 4), "Far Capital".into(), None, 5);
        g.spawn_city(TOWN, myr(4, 4), "Far Town".into(), None, 5);
        let spear = g
            .content
            .rules
            .units
            .id_numbered(104)
            .expect("id_of succeeds");
        for _ in 0..8 {
            g.spawn_unit(TOWN, myr(4, 4), spear);
        }
        g.turn = turn;
        party(&mut g)
    };
    assert_eq!(on_myrror(199), 2);
    assert_eq!(on_myrror(200), 4);
}

#[test]
fn the_raiders_are_as_experienced_as_the_turn_allows() {
    let level_at = |turn: u32| {
        let mut g = world(4);
        g.turn = turn;
        let before = g.units().player_units(TOWN);
        party(&mut g);
        let units = g.units();
        units
            .player_units(TOWN)
            .into_iter()
            .filter(|u| !before.contains(u))
            .map(|u| units.unit_xp_level(u))
            .max()
            .unwrap_or(0)
    };
    let levels: Vec<_> = [10, 41, 121, 251].map(level_at).to_vec();
    assert_eq!(levels[0], 0, "a young world raises green troops");
    assert!(
        levels.windows(2).all(|w| w[0] <= w[1]),
        "later parties are no greener: {levels:?}"
    );
    assert!(levels[3] > levels[0], "and the late ones are veterans");
}

#[test]
fn settlers_do_not_make_a_party() -> TestResult {
    let mut g = world(0);
    let settlers = g
        .content
        .rules
        .units
        .id_numbered(111)
        .ok_or("no settlers")?;
    for _ in 0..4 {
        g.spawn_unit(TOWN, at(4, 4), settlers);
    }
    g.threat.raider_clock = one_short(&g);
    let monsters_before = g.threat.monster_clock;
    assert!(g.process_threats().is_empty());
    assert_eq!(
        g.threat.monster_clock,
        monsters_before + 15,
        "a failed raid"
    );
    Ok(())
}

/// An island of grassland (x 2..=9, y 2..=9) in a sea of 24 by 14, on both planes, with a town
/// at its west end (3, 5). The sea raids are on and always come (the chance is 100).
fn island() -> Game {
    let mut g = world(4);
    let sea = g
        .content
        .terrains
        .id_numbered(8 /* ocean */)
        .expect("ocean");
    let grass = g
        .content
        .terrains
        .id_numbered(4 /* grassland */)
        .expect("grass");
    let mut map = Map::new(Grid::new(24, 14, sea), Grid::new(24, 14, sea));
    for plane in [Plane::Arcanus, Plane::Myrror] {
        for y in 2..=9 {
            for x in 2..=9 {
                map.plane_mut(plane).set(x, y, grass);
            }
        }
    }
    g.map = Some(map);
    g.settings.world.sea_raids = true;
    g.content.rules.config.threat.sea_raid.chance_percent = 100;
    // The two cities of `world` stand in the sea now; the island's own town takes the west.
    for (e, ..) in g.economy().cities() {
        let _ = g.world.despawn(e);
    }
    for u in g.units().player_units(TOWN) {
        let _ = g.world.despawn(u);
    }
    g.spawn_city(P0, at(3, 5), "Capital".into(), None, 5);
    g
}

/// The landing of the raid just made: its party, ship and the race.
fn landed(
    g: &mut Game,
) -> (
    Vec<hecs::Entity>,
    hecs::Entity,
    crate::rules::RaceId,
    Position,
) {
    g.threat.raider_clock = one_short(g);
    let events = g.process_threats();
    let Some(Event::SeaRaidLanded {
        owner, at: spot, ..
    }) = events.first().cloned()
    else {
        panic!("a party landed: {events:?}");
    };
    let race = g
        .race_of_player(owner)
        .expect("a race player owns the party");
    let units = g.units().player_units(owner);
    let ship = *units
        .iter()
        .find(|&&u| g.units().unit_stat(u, Stat::Transport) > 0)
        .expect("a ship");
    let party = units.into_iter().filter(|&u| u != ship).collect();
    (party, ship, race, spot)
}

#[test]
fn a_sea_raid_lands_a_fighter_and_a_settler_with_their_ship_beside_them_all_free() {
    for seed in 0..30 {
        let mut g = island();
        g.turn = 1;
        g.rng = crate::game::rng::GameRng::new(seed);
        let (party, ship, race, spot) = landed(&mut g);
        let def = |u| g.units().unit_def(u).expect("a unit def");
        assert_eq!(def(ship).num, 35, "a trireme at first (seed {seed})");
        let sea = g.units().unit_position(ship).expect("the ship's square");
        assert!(!g.units().units_at(sea).is_empty());
        let grid = g.map().expect("map").plane(spot.plane);
        assert!(
            grid.chebyshev((sea.x, sea.y), (spot.x, spot.y)) == 1,
            "seed {seed}"
        );
        // Step 1: 1d2 fighters, each of the race's two cheapest, and the settler.
        assert!(
            (2..=3).contains(&party.len()),
            "seed {seed}: {}",
            party.len()
        );
        let cheap = cheapest_fighters(&g, race, 2);
        for &u in party.iter().filter(|&&u| !is_settler(&g, u)) {
            assert!(cheap.contains(&def(u).cost), "seed {seed}");
        }
        assert!(party
            .iter()
            .all(|&u| g.units().unit_position(u) == Some(spot)));
        assert!(party
            .iter()
            .chain([&ship])
            .all(|&u| g.units().unit_is_unpaid(u)));
        let races: Vec<_> = party
            .iter()
            .map(|&u| def(u).race.map(|r| r.num()))
            .collect();
        assert!(races.iter().all(|&r| r == Some(race.num())), "{races:?}");
    }
}

/// The costs of the `n` cheapest fighters of a race (0: all).
fn cheapest_fighters(g: &Game, race: crate::rules::RaceId, n: usize) -> Vec<u32> {
    let rules = &g.content.rules;
    let mut costs: Vec<u32> = rules
        .races
        .get(race)
        .units
        .iter()
        .map(|&u| rules.units.get(u))
        .filter(|d| {
            d.kind == crate::rules::UnitKind::Normal
                && !d.abilities.contains(&crate::rules::Ability::new(12))
                && d.stat(Stat::Melee).max(d.stat(Stat::Ranged)) > 0.0
        })
        .map(|d| d.cost)
        .collect();
    costs.sort_unstable();
    costs.truncate(if n == 0 { costs.len() } else { n });
    costs
}

fn is_settler(g: &Game, u: hecs::Entity) -> bool {
    g.units()
        .unit_has_ability(u, crate::rules::Ability::new(12))
}

#[test]
fn a_sea_raid_grows_with_the_turn_in_the_steps_of_the_rules_and_never_rolls_a_settler() {
    // (turn, fewest and most fighters, the pool of cheapest units, the ship's unit number)
    for (turn, low, high, pool, ship_num) in
        [(1, 1, 2, 2, 35), (150, 2, 3, 4, 36), (300, 3, 5, 0, 36)]
    {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let mut g = island();
            g.turn = turn;
            g.rng = crate::game::rng::GameRng::new(seed);
            let (party, ship, race, _) = landed(&mut g);
            assert_eq!(g.units().unit_def(ship).map(|d| d.num), Some(ship_num));
            let fighters: Vec<_> = party.iter().filter(|&&u| !is_settler(&g, u)).collect();
            assert_eq!(party.len() - fighters.len(), 1, "one settler");
            assert!(
                (low..=high).contains(&fighters.len()),
                "turn {turn}: {} fighters",
                fighters.len()
            );
            let allowed = cheapest_fighters(&g, race, pool);
            for &&u in &fighters {
                let cost = g.units().unit_def(u).map(|d| d.cost).unwrap_or(0);
                assert!(
                    allowed.contains(&cost),
                    "turn {turn}: cost {cost} of {allowed:?}"
                );
            }
            seen.insert(fighters.len());
        }
        assert_eq!(
            seen.len(),
            high - low + 1,
            "turn {turn}: every size comes up: {seen:?}"
        );
    }
}

#[test]
fn with_the_setting_off_a_raid_is_never_a_sea_raid() {
    let mut g = island();
    g.settings.world.sea_raids = false;
    g.threat.raider_clock = one_short(&g);
    let events = g.process_threats();
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::SeaRaidLanded { .. })));
}

#[test]
fn a_sea_raid_lands_where_the_nearest_town_is_furthest_of_the_four_squares_rolled() {
    let g0 = island();
    let coast: Vec<Position> = g0
        .free_coast(Plane::Arcanus)
        .iter()
        .map(|l| l.land)
        .collect();
    let near = |p: Position| {
        g0.map()
            .expect("map")
            .plane(p.plane)
            .chebyshev((3, 5), (p.x, p.y))
    };
    let furthest = coast.iter().map(|&p| near(p)).max().expect("a coast");
    for seed in 0..20 {
        let mut g = island();
        g.rng = crate::game::rng::GameRng::new(seed);
        let Some(l) = g.sea_raid_landing(Plane::Arcanus) else {
            panic!("a landing");
        };
        assert!(near(l.land) <= furthest);
        assert!(coast.contains(&l.land));
        // The landing is no worse than any of four squares: at least the town's distance 1.
        assert!(near(l.land) >= 1);
    }
}
