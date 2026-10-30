use super::*;
use crate::feature::def::{FeatureDef, FeatureKind, Features};
use crate::feature::lair::Lair;
use crate::game::testutil::{content_with_magic, unit};
use crate::game::Command;
use crate::map::{Grid, Map, Plane};
use crate::player::{PlayerId, MONSTERS};
use crate::rules::{MovementType, Registry, Stat, UnitKind};

const P0: PlayerId = PlayerId(0);
const TOWN: PlayerId = PlayerId(100);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn content() -> crate::content::Content {
    let mut c = content_with_magic();
    let mut units = c.rules.units.defs().to_vec();
    let mut spear = unit("spearmen", MovementType::Walking, 1, &[]);
    spear.kind = UnitKind::Normal;
    spear.upkeep.gold = 1;
    units.push(spear);
    for (id, realm, cost) in [
        ("skeletons", "death", 25),
        ("ghouls", "death", 80),
        ("angel", "life", 550),
    ] {
        let mut u = unit(id, MovementType::Walking, 2, &[]);
        u.kind = UnitKind::Summoned;
        u.realm = Some(realm.into());
        u.cost = cost;
        units.push(u);
    }
    c.rules.units = Registry::new_sorted(units);
    let mut features = c.features.defs().to_vec();
    features.push(FeatureDef {
        id: "ruins".into(),
        name: "Ruins".into(),
        kind: FeatureKind::Encounter,
        city: Default::default(),
        spawn: Vec::new(),
    });
    c.features = Features::new(features);
    c
}

/// A grassland world of 24 by 14 with a wizard at (10, 4) and an independent city at (4, 4)
/// that has `troops` spearmen. Raiders and monsters are switched on and Impossible is played.
fn world(troops: usize) -> Game {
    let c = content();
    let grass = c.terrains.id_of("grassland").unwrap();
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
    let spear = g.content.rules.units.id_of("spearmen").unwrap();
    for _ in 0..troops {
        g.spawn_unit(TOWN, at(4, 4), spear);
    }
    g
}

fn lair(guards: Vec<(&str, u32)>) -> Lair {
    Lair {
        intact: true,
        guards: guards
            .into_iter()
            .map(|(g, n)| (g.to_string(), n))
            .collect(),
        gold: 120,
        mana: 0,
        items: Vec::new(),
        prisoner: false,
        special: 0,
        fought_by: vec![P0],
    }
}

#[test]
fn the_raider_clock_runs_to_thirty_and_a_city_raises_free_troops_and_goes_to_war() {
    let mut g = world(4);
    g.threat.raider_clock = 29;
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
    assert!(g.is_aggressive_player(TOWN));
    assert!(!g.is_aggressive_player(P0));
}

#[test]
fn below_thirty_nothing_happens() {
    let mut g = world(4);
    g.threat.raider_clock = 0;
    assert!(g.process_threats().is_empty());
    assert!((1..=5).contains(&g.threat.raider_clock));
    assert!(!g.is_aggressive_player(TOWN));
}

#[test]
fn a_neutral_city_alone_on_its_landmass_raises_nobody_and_feeds_the_monsters_clock() {
    let mut g = world(4);
    let sea = g.content.terrains.id_of("ocean").unwrap();
    // A strip of sea between the two cities.
    for y in 0..14 {
        for x in [7, 20] {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(x, y, sea);
        }
    }
    g.threat.raider_clock = 29;
    let before = g.threat.monster_clock;
    assert!(g.process_threats().is_empty());
    assert_eq!(g.threat.monster_clock, before + 15);
    assert!(!g.is_aggressive_player(TOWN));
}

#[test]
fn a_full_city_has_no_room_for_a_party() {
    let mut g = world(crate::unit::MAX_TILE_LIMIT);
    g.threat.raider_clock = 29;
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
    g.threat.raider_clock = 29;
    assert!(g.process_threats().is_empty());
    assert_eq!(
        g.threat,
        Threat {
            raider_clock: 29,
            monster_clock: 0
        }
    );
}

#[test]
fn a_new_stack_takes_a_lair_and_the_guards_that_lived_there_are_driven_out() {
    let mut g = world(0);
    g.settings.world.raiders_enabled = false;
    g.turn = 100;
    let site = g.put_lair(at(14, 8), "lair", lair(vec![("skeletons", 2)]));
    g.threat.monster_clock = 1000;
    let events = g.process_threats();
    let Some(Event::MonstersSettled {
        pos,
        count,
        evicted,
        ..
    }) = events.first().cloned()
    else {
        panic!("monsters settled: {events:?}");
    };
    assert_eq!(pos, at(14, 8));
    assert!(count >= 1);
    assert_eq!(evicted, 2);
    let lair = g.economy().lair_of(site).unwrap();
    assert_eq!(lair.guards.iter().map(|(_, n)| n).sum::<u32>(), count);
    assert!(lair
        .guards
        .iter()
        .all(|(id, _)| id == "skeletons" || id == "ghouls"));
    assert!(lair.fought_by.is_empty(), "nobody knows the new guards");
    let roaming = g.units().player_units(MONSTERS);
    assert_eq!(roaming.len(), 2);
    let square = g.units().unit_position(roaming[0]).unwrap();
    assert_ne!(square, at(14, 8));
    assert!(roaming
        .iter()
        .all(|&u| g.units().unit_position(u) == Some(square)));
    assert_eq!(g.threat.monster_clock, 0);
}

#[test]
fn life_lairs_keep_their_guards() {
    let mut g = world(0);
    g.turn = 100;
    g.put_lair(at(14, 8), "lair", lair(vec![("angel", 1)]));
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
    assert_eq!(
        g.threat.monster_clock, 1000,
        "no lair can send monsters, so the clock waits"
    );
}

#[test]
fn nothing_comes_before_turn_fifty_and_the_clock_starts_again() {
    let mut g = world(0);
    g.turn = 49;
    g.put_lair(at(14, 8), "lair", lair(vec![("skeletons", 2)]));
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
    assert_eq!(g.threat.monster_clock, 0);
}

#[test]
fn a_lair_on_a_landmass_without_a_player_is_not_chosen() {
    let mut g = world(0);
    g.turn = 100;
    let sea = g.content.terrains.id_of("ocean").unwrap();
    for y in 0..14 {
        for x in [12, 20] {
            g.map
                .as_mut()
                .unwrap()
                .plane_mut(Plane::Arcanus)
                .set(x, y, sea);
        }
    }
    g.put_lair(at(16, 8), "lair", lair(vec![("skeletons", 2)]));
    g.threat.monster_clock = 1000;
    assert!(g.process_threats().is_empty());
}

#[test]
fn roaming_monsters_settle_in_a_cleared_node_nobody_holds() {
    let mut g = world(0);
    let site = g.put_lair(
        at(14, 8),
        "node",
        Lair {
            intact: false,
            guards: Vec::new(),
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: vec![P0],
        },
    );
    let ghoul = g.content.rules.units.id_of("ghouls").unwrap();
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
    let lair = g.economy().lair_of(site).unwrap();
    assert!(lair.intact);
    assert_eq!(lair.guards, vec![("ghouls".to_string(), 2)]);
    assert!(
        g.units().player_units(MONSTERS).is_empty(),
        "the units became the guards"
    );
}

#[test]
fn a_node_a_wizard_holds_is_not_taken() {
    let mut g = world(0);
    let node = crate::feature::node::Node {
        realm: "chaos".into(),
        power: 5,
        aura: vec![(14, 8)],
        guardian: false,
        warped: false,
    };
    let e = g.put_node_data(at(14, 8), "node", node, Some(P0));
    g.world
        .insert_one(
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
            },
        )
        .unwrap();
    assert_eq!(g.vacant_zone(at(14, 8)), None);
}

#[test]
fn monsters_that_win_a_city_wreck_it_and_vanish_and_sometimes_leave_ruins() {
    let mut ruined = 0;
    let mut kept = 0;
    for seed in 0..40 {
        let mut g = world(0);
        g.rng = crate::game::rng::GameRng::new(seed);
        let ghoul = g.content.rules.units.id_of("ghouls").unwrap();
        let stack = vec![
            g.spawn_unit(MONSTERS, at(10, 5), ghoul),
            g.spawn_unit(MONSTERS, at(10, 5), ghoul),
        ];
        // A second city, so the wizard's last one is not what falls: the fortress is spared.
        g.spawn_city(P0, at(16, 9), "Colony".into(), None, 6);
        let colony = g.economy().city_at(at(16, 9)).unwrap();
        for &m in &stack {
            g.world.insert_one(m, at(16, 8)).unwrap();
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
            panic!("a rampage: {events:?}");
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
            assert_eq!(lair.guards, vec![("ghouls".to_string(), 2)]);
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
}

#[test]
fn monsters_that_beat_the_defenders_in_a_battle_rampage_through_the_city() {
    let mut rampaged = 0;
    for seed in 0..12 {
        let mut g = world(0);
        g.rng = crate::game::rng::GameRng::new(seed);
        let spear = g.content.rules.units.id_of("spearmen").unwrap();
        let ghoul = g.content.rules.units.id_of("ghouls").unwrap();
        g.spawn_city(P0, at(16, 9), "Colony".into(), None, 6);
        let defenders = vec![g.spawn_unit(P0, at(16, 9), spear)];
        let monsters: Vec<_> = (0..9)
            .map(|_| g.spawn_unit(MONSTERS, at(16, 8), ghoul))
            .collect();
        let events = g.apply(Command::AutoResolve {
            mana_budget: None,
            attackers: monsters,
            defenders,
        });
        if events
            .iter()
            .any(|e| matches!(e, Event::CityRampaged { .. }))
        {
            rampaged += 1;
            assert!(g.units().player_units(MONSTERS).is_empty());
            if let Some(city) = g.economy().city_at(at(16, 9)) {
                assert_eq!(g.economy().city_owner(city), Some(P0));
            }
        }
    }
    assert!(rampaged > 6, "nine ghouls beat one spearman: {rampaged}");
}

#[test]
fn the_clocks_and_the_marks_survive_a_save() {
    let mut g = world(4);
    g.threat.raider_clock = 29;
    g.process_threats();
    g.threat.monster_clock = 7;
    let text = g.snapshot();
    let back = text.restore(g.content.clone()).unwrap();
    assert_eq!(back.threat, g.threat);
    assert!(back.is_aggressive_player(TOWN));
    let free = back
        .units()
        .player_units(TOWN)
        .into_iter()
        .filter(|&u| back.units().unit_is_unpaid(u))
        .count();
    assert_eq!(free, 2);
}

#[test]
fn the_free_troops_cost_their_city_nothing() {
    let mut g = world(4);
    let before = g.budget(TOWN).unit_upkeep;
    g.threat.raider_clock = 29;
    g.process_threats();
    assert_eq!(g.budget(TOWN).unit_upkeep, before);
    let _ = Stat::Melee;
}
