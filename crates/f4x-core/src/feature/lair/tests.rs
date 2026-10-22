use super::*;
use crate::battle::{BattleCommand, Side};
use crate::game::testutil::{content_with_magic, unit};
use crate::map::{Grid, Plane};
use crate::player::{Player, PlayerId};

use crate::game::Command;
use crate::rules::{MovementType, Registry, UnitDef};

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn summoned(id: &str, realm: &str, cost: u32) -> UnitDef {
    let mut u = unit(id, MovementType::Walking, 2, &[]);
    u.kind = UnitKind::Summoned;
    u.realm = Some(realm.into());
    u.cost = cost;
    u
}

/// Magic test content plus guards of death and chaos, and a weak one to beat.
fn content() -> Content {
    let mut c = content_with_magic();
    let mut units = c.rules.units.defs().to_vec();
    units.extend([
        summoned("skeletons", "death", 25),
        summoned("ghouls", "death", 80),
        summoned("wraiths", "death", 500),
        summoned("hell_hounds", "chaos", 40),
        summoned("great_drake", "chaos", 900),
        summoned("nature_guard", "nature", 70),
        summoned("angel", "life", 550),
        summoned("djinn", "sorcery", 650),
    ]);
    let mut ship = summoned("floating_island", "death", 9999);
    ship.stats.insert(Stat::Transport, 8);
    units.push(ship);
    c.rules.units = Registry::new(units);
    c
}

fn world(seed: u64) -> Game {
    let c = content();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(seed);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.fog = crate::map::fog::Fog::new(24, 14);
    for id in [P0, PlayerId(1)] {
        g.add_player(id, "W", Player::new());
        g.treasury_mut(id).unwrap().gold = 100;
        g.treasury_mut(id).unwrap().mana = 100;
    }
    g.init_wizard(P0, &[("life", 2)], &[], None).unwrap();
    g
}

fn put_feature(g: &mut Game, plane: Plane, x: u32, y: u32, id: &str) {
    let f = g.content.features.id_of(id).unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(plane)
        .set_feature(x, y, Some(f));
}

fn lair(x: u32, y: u32, guards: Vec<(&str, u32)>) -> Lair {
    Lair {
        plane: Plane::Arcanus,
        x,
        y,
        site: "lair".into(),
        intact: true,
        guards: guards
            .into_iter()
            .map(|(g, n)| (g.to_string(), n))
            .collect(),
        gold: 120,
        mana: 30,
        items: vec![400],
        prisoner: true,
        special: 0,
    }
}

#[test]
fn only_summoned_non_transport_units_of_the_realm_can_guard() {
    let c = content();
    let death: Vec<String> = candidates(&c, "death")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(
        death,
        ["skeletons", "ghouls", "wraiths"],
        "no ship, no other realm"
    );
    assert!(candidates(&c, "arcane").is_empty());
}

#[test]
fn the_dearest_affordable_unit_is_chosen_and_the_first_of_equals() {
    let list = vec![
        ("a".to_string(), 25),
        ("b".to_string(), 80),
        ("c".to_string(), 80),
        ("d".to_string(), 500),
    ];
    assert_eq!(dearest_under(&list, 501, None).unwrap().0, "d");
    assert_eq!(
        dearest_under(&list, 500, None).unwrap().0,
        "b",
        "strictly cheaper"
    );
    assert_eq!(dearest_under(&list, 500, Some("b")).unwrap().0, "c");
    assert_eq!(dearest_under(&list, 25, None), None);
}

#[test]
fn guards_fit_the_budget_and_the_stack() {
    let list = candidates(&content(), "death");
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    for budget in [0, 20, 60, 200, 700, 1500, 4000] {
        for _ in 0..50 {
            let (guards, first, second) = pick_guards(&mut rng, &list, budget);
            let total: u32 = guards.iter().map(|g| g.1).sum();
            assert!(total <= MAX_GUARDS, "{total} guards for {budget}");
            assert!(first + second <= budget, "{first}+{second} over {budget}");
            if guards.len() == 2 {
                assert_ne!(guards[0].0, guards[1].0, "two different kinds");
                assert!(guards[0].1 <= MAX_FIRST_GUARDS as u32);
            }
        }
    }
    // A budget below the cheapest unit buys nothing.
    let (guards, ..) = pick_guards(&mut rng, &list, 20);
    assert!(guards.is_empty());
}

#[test]
fn a_site_type_decides_the_guards_realm() {
    let mut rng = Pcg64Mcg::seed_from_u64(2);
    for _ in 0..100 {
        assert_eq!(guard_realm(&mut rng, SiteType::Node("sorcery")), "sorcery");
        assert_eq!(guard_realm(&mut rng, SiteType::Node("nature")), "nature");
        assert_eq!(guard_realm(&mut rng, SiteType::Node("chaos")), "chaos");
        assert!(["death", "life"].contains(&guard_realm(&mut rng, SiteType::Holy)));
        assert!(["death", "chaos", "nature"].contains(&guard_realm(&mut rng, SiteType::Wild)));
    }
    let seen: std::collections::BTreeSet<_> = (0..300)
        .map(|_| guard_realm(&mut rng, SiteType::Tower))
        .collect();
    assert_eq!(seen.len(), 5, "towers draw on every realm");
    assert_eq!(
        site_type("ancient_temple", FeatureKind::Encounter),
        SiteType::Holy
    );
    assert_eq!(site_type("ruins", FeatureKind::Encounter), SiteType::Holy);
    assert_eq!(site_type("dungeon", FeatureKind::Encounter), SiteType::Wild);
    assert_eq!(
        site_type("chaos_node", FeatureKind::Node),
        SiteType::Node("chaos")
    );
}

#[test]
fn every_site_gets_a_lair_and_towers_share_one_on_both_planes() {
    let mut g = world(1);
    put_feature(&mut g, Plane::Arcanus, 3, 3, "tower");
    put_feature(&mut g, Plane::Myrror, 3, 3, "tower");
    put_feature(&mut g, Plane::Arcanus, 8, 5, "lair");
    put_feature(&mut g, Plane::Myrror, 9, 5, "lair");
    put_feature(&mut g, Plane::Myrror, 12, 9, "lair");
    let params = GenParams::default();
    let mut map = g.map.take().unwrap();
    generate_lairs(11, &params, &mut map, &g.content);
    let lairs = map.lairs();
    assert_eq!(lairs.len(), 2 + 3, "tower twice, three encounter sites");
    assert!(lairs.iter().all(|l| l.intact));
    let towers: Vec<_> = lairs.iter().filter(|l| l.site == "tower").collect();
    assert_eq!(towers.len(), 2);
    assert_ne!(towers[0].plane, towers[1].plane);
    assert_eq!(towers[0].guards, towers[1].guards);
    assert_eq!(
        (towers[0].gold, towers[0].special),
        (towers[1].gold, towers[1].special)
    );
    assert!(towers[0].special >= 1, "a tower always holds a spell");
    // The same seed gives the same lairs.
    let mut again = map.clone();
    generate_lairs(11, &params, &mut again, &g.content);
    assert_eq!(again.lairs(), map.lairs());
    generate_lairs(12, &params, &mut again, &g.content);
    assert_ne!(again.lairs(), map.lairs());
}

#[test]
fn hoards_follow_the_original_rules() {
    let g = world(1);
    let mut rng = Pcg64Mcg::seed_from_u64(5);
    let mut saw_items = false;
    let mut saw_big = false;
    for i in 0..3000 {
        let plane = if i % 2 == 0 {
            Plane::Arcanus
        } else {
            Plane::Myrror
        };
        let l = make_lair(
            &mut rng,
            &g.content,
            plane,
            (1, 1),
            "lair",
            SiteType::Wild,
            200 + (i % 40) * 100,
            75,
            133,
        );
        assert_eq!(l.gold % 10, 0);
        assert_eq!(l.mana % 10, 0);
        assert!(l.items.len() <= 3);
        assert!(l.items.iter().all(|&v| v >= 100 && v % 10 == 0));
        assert!(l.special <= 6);
        if l.special > 4 {
            saw_big = true;
            assert_eq!(
                (l.gold, l.mana, l.items.len(), l.prisoner),
                (0, 0, 0, false)
            );
        }
        saw_items |= !l.items.is_empty();
    }
    assert!(saw_items && saw_big);
    // Impossible keeps more of the budget (125%): more guards than on Intro (25%).
    let value = |guard_percent| {
        let mut rng = Pcg64Mcg::seed_from_u64(9);
        (0..200)
            .map(|_| {
                make_lair(
                    &mut rng,
                    &g.content,
                    Plane::Arcanus,
                    (1, 1),
                    "lair",
                    SiteType::Wild,
                    900,
                    guard_percent,
                    100,
                )
                .guard_count()
            })
            .sum::<u32>()
    };
    assert!(value(125) > value(25));
}

#[test]
fn a_stack_is_stopped_by_a_guarded_site_and_nothing_moves() {
    let mut g = world(2);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 3)])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    let events = g.apply(Command::MoveStack {
        units: vec![w],
        to: at(5, 5),
    });
    assert_eq!(
        events,
        vec![Event::SiteEncounter {
            units: vec![w],
            from: at(4, 5),
            to: at(5, 5),
            lair: 0
        }]
    );
    assert_eq!(g.units().unit_position(w), Some(at(4, 5)));
    assert_eq!(g.units().unit_moves_left(w), 4, "no moves spent");
}

#[test]
fn walking_into_an_unguarded_site_takes_the_treasure_and_clears_it() {
    let mut g = world(3);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map.as_mut().unwrap().set_lairs(vec![lair(5, 5, vec![])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    let events = g.apply(Command::MoveStack {
        units: vec![w],
        to: at(5, 5),
    });
    assert!(matches!(events[0], Event::UnitsMoved { .. }));
    let Some(Event::SiteCleared { player, loot, .. }) = events.get(1) else {
        panic!("expected the site to be cleared: {events:?}");
    };
    assert_eq!(*player, P0);
    assert_eq!((loot.gold, loot.mana), (120, 30));
    assert_eq!(loot.items.len(), 1, "the 400 hoard is made into an item");
    assert_eq!(g.player(P0).unwrap().armory.items().count(), 1);
    // The test rules have no heroes to free.
    assert!(!loot.prisoner);
    assert_eq!(
        (g.treasury(P0).unwrap().gold, g.treasury(P0).unwrap().mana),
        (220, 130)
    );
    let map = g.map.as_ref().unwrap();
    assert!(map.lair_at(Plane::Arcanus, 5, 5).is_none());
    assert_eq!(
        map.plane(Plane::Arcanus).feature(5, 5),
        None,
        "the site is gone"
    );
    // Nothing to take the second time.
    let again = g.claim_lair(0, P0);
    assert!(again.is_empty());
}

#[test]
fn the_hoard_can_hold_a_spell_book_a_retort_or_a_spell() {
    let mut g = world(4);
    let mut l = lair(5, 5, vec![]);
    l.gold = 0;
    l.mana = 0;
    l.items.clear();
    l.prisoner = false;
    // A spell: from a realm the wizard has no books in, newly known.
    l.special = 2;
    g.map.as_mut().unwrap().set_lairs(vec![l.clone()]);
    let known_before = g.magic(P0).unwrap().known.len();
    let events = g.claim_lair(0, P0);
    let Some(Event::SiteCleared { loot, .. }) = events.first() else {
        panic!("{events:?}");
    };
    let spell = loot.spell.expect("a spell");
    assert!(g.magic(P0).unwrap().known.contains(&spell));
    assert_eq!(g.magic(P0).unwrap().known.len(), known_before + 1);
    assert_ne!(
        g.content.rules.spells.get(spell).realm,
        "life",
        "a realm without books"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::SpellLearned { .. })));
    // A book of a dungeon's realm (death): refused while the wizard has life books.
    let mut l = lair(6, 6, vec![]);
    l.site = "dungeon".into();
    l.special = 5;
    let mut loot = Loot::default();
    let p = crate::player::find_mut(&mut g.world, P0).unwrap();
    award_special(&mut p.magic, &mut g.rng, &l, 1, &mut loot);
    assert!(
        !loot.books.contains(&"death".to_string()),
        "life and death exclude each other"
    );
    // A temple gives a life book, up to the twelve-book limit.
    let mut l = lair(7, 7, vec![]);
    l.site = "ancient_temple".into();
    let mut books = 0;
    for _ in 0..40 {
        let mut loot = Loot::default();
        let p = crate::player::find_mut(&mut g.world, P0).unwrap();
        award_special(&mut p.magic, &mut g.rng, &l, 1, &mut loot);
        books += loot.books.len();
        assert!(loot.books.iter().all(|b| b == "life"));
    }
    assert!(books > 0);
    assert!(g.magic(P0).unwrap().total_books() <= 12);
}

fn fight_to_the_end(g: &mut Game) {
    use crate::battle::map::distance;
    for _ in 0..300 {
        if !g.in_battle() {
            return;
        }
        let (side, ids): (Side, Vec<usize>) = {
            let b = g.battle().unwrap();
            (b.active, b.active_units(b.active).map(|u| u.id).collect())
        };
        for id in ids {
            if !g.in_battle() {
                return;
            }
            let target = {
                let b = g.battle().unwrap();
                b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            };
            let Some((t, cell)) = target else { break };
            let step = {
                let b = g.battle().unwrap();
                b.reachable(id)
                    .into_iter()
                    .min_by_key(|(p, _)| distance(*p, cell))
                    .map(|(p, _)| p)
            };
            if let Some(to) = step {
                g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
            }
            g.apply(Command::Battle(BattleCommand::Attack {
                unit: id,
                target: t,
            }));
        }
        if g.in_battle() {
            g.apply(Command::Battle(BattleCommand::EndTurn));
        }
    }
    panic!("the battle did not finish");
}

#[test]
fn beating_the_guards_clears_the_site_pays_out_and_moves_in() {
    let mut g = world(5);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 1)])]);
    // Make the guard feeble and the attackers strong.
    let mut units = g.content.rules.units.defs().to_vec();
    for u in &mut units {
        match u.id.as_str() {
            "skeletons" => {
                u.stats.insert(Stat::Melee, 0);
                u.stats.insert(Stat::Defense, 0);
                u.stats.insert(Stat::MaxHp, 1);
                u.stats.insert(Stat::Figures, 1);
            }
            "walker" => {
                u.stats.insert(Stat::Melee, 10);
                u.stats.insert(Stat::Defense, 10);
            }
            _ => {}
        }
    }
    g.content.rules.units = Registry::new(units);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    let events = g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    assert!(matches!(events[0], Event::BattleStarted { defender, .. } if defender == MONSTERS));
    assert!(g.in_battle());
    assert_eq!(
        g.units().units_at(at(5, 5)).len(),
        1,
        "one guard stands on the site"
    );
    fight_to_the_end(&mut g);
    assert!(g.world.contains(w));
    assert!(
        g.map
            .as_ref()
            .unwrap()
            .lair_at(Plane::Arcanus, 5, 5)
            .is_none(),
        "cleared"
    );
    assert_eq!(
        g.map.as_ref().unwrap().plane(Plane::Arcanus).feature(5, 5),
        None
    );
    assert_eq!(g.treasury(P0).unwrap().gold, 220);
    assert_eq!(
        g.units().unit_position(w),
        Some(at(5, 5)),
        "the victors moved in"
    );
    assert!(
        g.units()
            .units_at(at(5, 5))
            .iter()
            .all(|&u| g.units().unit_owner(u) == Some(P0)),
        "no guard is left"
    );
}

#[test]
fn the_building_of_a_site_stands_in_the_middle_of_the_battlefield() {
    use crate::battle::map::layout::Structure;
    let mut g = world(5);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "tower");
    let mut tower = lair(5, 5, vec![("skeletons", 1)]);
    tower.site = "tower".into();
    g.map.as_mut().unwrap().set_lairs(vec![tower]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let active = g.battle.as_ref().expect("a battle is on");
    let look = active.battle.map.look().expect("the field has a look");
    assert_eq!(look.structure, Structure::Tower);
    assert!(!active.battle.map.is_playable(crate::battle::map::STRUCTURE));
}

#[test]
fn a_guard_drained_to_death_joins_the_victors_as_undead() {
    use crate::battle::Status;
    let mut g = world(5);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 2)])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let (drained, slain) = {
        let a = g.battle.as_ref().unwrap();
        let guards: Vec<usize> = a
            .battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender)
            .map(|u| u.id)
            .collect();
        assert_eq!(guards.len(), 2);
        (
            (
                guards[0],
                a.sources[a.battle.units[guards[0]].source.unwrap()],
            ),
            (
                guards[1],
                a.sources[a.battle.units[guards[1]].source.unwrap()],
            ),
        )
    };
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        for (id, kinds) in [(drained.0, [1, 5, 0]), (slain.0, [4, 0, 0])] {
            b.units[id].status = Status::Dead;
            b.units[id].figures = 0;
            b.units[id].damage_kinds = kinds;
        }
        // The test guards are skeletons, creatures of the Death realm, which are not raised.
        b.units[drained.0].realm = None;
    }
    let events = g.apply(Command::Battle(BattleCommand::EndTurn));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::UndeadRaised { player, units: 1 } if *player == P0)));
    assert_eq!(g.units().unit_owner(drained.1), Some(P0));
    assert!(g.units().unit_is_undead(drained.1));
    assert_eq!(g.units().unit_position(drained.1), Some(at(5, 5)));
    assert!(!g.world.contains(slain.1), "the other guard is gone");
    assert_eq!(
        g.units().unit_position(w),
        Some(at(5, 5)),
        "the victors moved in"
    );
}

#[test]
fn zombie_mastery_takes_any_normal_dead_but_not_the_destroyed() {
    use crate::battle::Status;
    let mut g = world(7);
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 2)])]);
    // Make the guards normal units, and give the winner Zombie Mastery.
    let mut units = g.content.rules.units.defs().to_vec();
    for d in units.iter_mut().filter(|d| d.id == "skeletons") {
        d.kind = crate::rules::UnitKind::Normal;
    }
    g.content.rules.units = Registry::new(units);
    let mut spells = g.content.rules.spells.defs().to_vec();
    let mut zm = spells[0].clone();
    zm.id = "zombie_mastery".into();
    spells.push(zm);
    g.content.rules.spells = Registry::new(spells);
    let zm = g.content.rules.spells.id_of("zombie_mastery").unwrap();
    g.magic_mut(P0).unwrap().global.push(zm);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    let guards: Vec<(usize, hecs::Entity)> = {
        let a = g.battle.as_ref().unwrap();
        a.battle
            .units
            .iter()
            .filter(|u| u.side == Side::Defender)
            .map(|u| (u.id, a.sources[u.source.unwrap()]))
            .collect()
    };
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        // One struck down, one destroyed.
        for (i, (id, _)) in guards.iter().enumerate() {
            b.units[*id].status = Status::Dead;
            b.units[*id].figures = 0;
            b.units[*id].damage_kinds = if i == 0 { [4, 0, 0] } else { [0, 0, 1] };
            b.units[*id].realm = None;
        }
    }
    g.apply(Command::Battle(BattleCommand::EndTurn));
    assert_eq!(g.units().unit_owner(guards[0].1), Some(P0));
    assert!(g.units().unit_is_undead(guards[0].1));
    assert!(!g.world.contains(guards[1].1), "the destroyed one is gone");
}

#[test]
fn an_attacker_drained_to_death_by_the_guards_stays_as_a_neutral_undead_unit() {
    use crate::battle::Status;
    let mut g = world(5);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 1)])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let (w, bystander) = (
        g.spawn_unit(P0, at(4, 5), walker),
        g.spawn_unit(P0, at(4, 5), walker),
    );
    g.apply(Command::EnterSite {
        units: vec![w, bystander],
        to: at(5, 5),
    });
    {
        let b = &mut g.battle.as_mut().unwrap().battle;
        for u in b.units.iter_mut().filter(|u| u.side == Side::Attacker) {
            u.status = Status::Dead;
            u.figures = 0;
            u.damage_kinds = if u.source == Some(0) {
                [1, 5, 0]
            } else {
                [4, 0, 0]
            };
            u.realm = None;
        }
    }
    let events = g.apply(Command::Battle(BattleCommand::EndTurn));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::UndeadRaised { player, units: 1 } if *player == MONSTERS)));
    // The drained unit stays on the site for the monsters; the guard made for the fight and
    // the other attacker are gone.
    assert!(g.world.contains(w) && !g.world.contains(bystander));
    assert_eq!(g.units().unit_owner(w), Some(MONSTERS));
    assert!(g.units().unit_is_undead(w));
    assert_eq!(g.units().units_at(at(5, 5)), vec![w]);
    assert!(
        g.economy().lair_at(at(5, 5)).is_some(),
        "the lair is not cleared"
    );
    // It lasts, and it is an enemy to anyone who walks in.
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_owner(w), Some(MONSTERS));
    assert!(g.economy().hostile_at(at(5, 5), P0).is_some());
}

#[test]
fn losing_or_fleeing_leaves_the_site_intact_and_resets_the_guards() {
    let mut g = world(6);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 3)])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(4, 5), walker);
    g.apply(Command::EnterSite {
        units: vec![w],
        to: at(5, 5),
    });
    assert_eq!(g.units().units_at(at(5, 5)).len(), 3);
    g.apply(Command::Battle(BattleCommand::Flee));
    assert!(!g.in_battle());
    assert!(g
        .map
        .as_ref()
        .unwrap()
        .lair_at(Plane::Arcanus, 5, 5)
        .is_some());
    assert!(
        g.units().units_at(at(5, 5)).is_empty(),
        "the guards are made anew on each visit"
    );
    assert_eq!(g.treasury(P0).unwrap().gold, 100, "no treasure");
    assert_eq!(g.units().unit_position(w), Some(at(4, 5)));
}

#[test]
fn only_a_guarded_site_next_to_the_stack_can_be_entered() {
    let mut g = world(7);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "lair");
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 1)]), lair(9, 9, vec![])]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let w = g.spawn_unit(P0, at(2, 2), walker);
    let reject = |g: &mut Game, to| g.apply(Command::EnterSite { units: vec![w], to });
    assert_eq!(
        reject(&mut g, at(5, 5)),
        vec![Event::Rejected(crate::game::Rejection::Site(
            SiteError::Move(MoveError::NotAdjacent)
        ))]
    );
    assert_eq!(
        reject(&mut g, at(3, 3)),
        vec![Event::Rejected(crate::game::Rejection::Site(
            SiteError::NoSite
        ))]
    );
    assert!(!g.in_battle());
}

#[test]
fn clearing_a_tower_clears_it_on_both_planes() {
    let mut g = world(8);
    put_feature(&mut g, Plane::Arcanus, 5, 5, "tower");
    put_feature(&mut g, Plane::Myrror, 5, 5, "tower");
    let mut a = lair(5, 5, vec![]);
    a.site = "tower".into();
    let b = Lair {
        plane: Plane::Myrror,
        ..a.clone()
    };
    g.map.as_mut().unwrap().set_lairs(vec![b, a]);
    g.claim_lair(1, P0);
    let map = g.map.as_ref().unwrap();
    assert!(map.lairs().iter().all(|l| !l.intact));
    assert!(
        map.plane(Plane::Arcanus).feature(5, 5).is_some(),
        "the tower itself stays"
    );
}

#[test]
fn lairs_survive_a_save() {
    let mut g = world(9);
    g.map
        .as_mut()
        .unwrap()
        .set_lairs(vec![lair(5, 5, vec![("skeletons", 2), ("ghouls", 1)])]);
    let snap = g.snapshot();
    let back = snap.restore(g.content.clone()).unwrap();
    assert_eq!(
        back.map.as_ref().unwrap().lairs(),
        g.map.as_ref().unwrap().lairs()
    );
}

/// A map and content with a lair on it and no game.
fn bare(lairs: Vec<Lair>) -> (Map, Content) {
    let c = content();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut map = Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass));
    map.set_lairs(lairs);
    (map, c)
}

#[test]
fn clearing_a_site_takes_its_treasure_once_and_needs_no_game() {
    let (mut map, c) = bare(vec![lair(5, 5, vec![("walker", 1)])]);
    let cleared = clear_site(&mut map, &c, 0).expect("an intact lair");
    assert_eq!(
        (cleared.gold, cleared.mana, cleared.items),
        (120, 30, vec![400])
    );
    assert!(map.lair_at(Plane::Arcanus, 5, 5).is_none());
    assert!(
        clear_site(&mut map, &c, 0).is_none(),
        "nothing the second time"
    );
    assert!(clear_site(&mut map, &c, 7).is_none(), "no such site");
}

#[test]
fn clearing_a_tower_clears_its_twin_on_the_other_plane() {
    let mut a = lair(3, 3, vec![]);
    a.site = "tower".into();
    let mut b = a.clone();
    b.plane = Plane::Myrror;
    let mut other = lair(8, 8, vec![]);
    other.site = "tower".into();
    let (mut map, c) = bare(vec![a, b, other]);
    clear_site(&mut map, &c, 0).unwrap();
    assert!(!map.lairs()[0].intact && !map.lairs()[1].intact);
    assert!(map.lairs()[2].intact, "another tower stays");
}

#[test]
fn a_hoard_of_books_obeys_the_realm_and_the_limit_without_a_game() {
    let mut magic = crate::wizard::magic::Magic::new(&[("life", 11)], &[]);
    let mut rng = crate::game::rng::GameRng::new(5);
    let mut l = lair(1, 1, vec![]);
    l.site = "ruins".into();
    for _ in 0..30 {
        let mut loot = Loot::default();
        award_special(&mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.books.is_empty(), "death clashes with life");
    }
    l.site = "ancient_temple".into();
    for _ in 0..60 {
        award_special(&mut magic, &mut rng, &l, 2, &mut Loot::default());
    }
    assert!(magic.total_books() <= 12);
}

#[test]
fn the_limits_of_six_retorts_and_twelve_books_decide_between_a_book_and_a_retort() {
    use crate::wizard::magic::Retort::*;
    let mut l = lair(1, 1, vec![]);
    l.site = "nature_node".into();
    // Six retorts: only books can come.
    let mut magic = crate::wizard::magic::Magic::new(
        &[("nature", 3)],
        &[Alchemy, Warlord, Charismatic, Famous, Artificer, Conjurer],
    );
    let mut rng = crate::game::rng::GameRng::new(9);
    for _ in 0..40 {
        let mut loot = Loot::default();
        award_special(&mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.retorts.is_empty());
    }
    assert_eq!(magic.retorts.len(), 6);
    // Twelve books: only retorts can come.
    let mut magic = crate::wizard::magic::Magic::new(&[("nature", 6), ("sorcery", 6)], &[]);
    for _ in 0..40 {
        let mut loot = Loot::default();
        award_special(&mut magic, &mut rng, &l, 1, &mut loot);
        assert!(loot.books.is_empty());
    }
    assert_eq!(magic.total_books(), 12);
}

#[test]
fn a_spell_award_teaches_a_spell_from_a_realm_without_books() {
    let c = content();
    let mut magic = crate::wizard::magic::Magic::new(&[("life", 2)], &[]);
    let mut rng = crate::game::rng::GameRng::new(9);
    let spell = award_spell(&mut magic, &c, &mut rng, 1).expect("a spell");
    assert!(magic.known.contains(&spell));
    assert_ne!(c.rules.spells.get(spell).realm, "life");
}
