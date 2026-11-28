//! Class 9, the threats of the neutral world: the raiding parties of the independent cities and
//! from the sea, and the monsters that leave their lairs, settle and sack cities. The clocks, the
//! parties and the budgets are the content's (`threat` of `rules.ron`) and the world settings'.

use crate::support::{around, content, text, worlds, WORLDS};
use f4x_battle::battle::{BattleCommand, Side};
use f4x_core::feature::def::FeatureKind;
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Plane, Position};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::{Stat, UnitId, UnitKind};
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::{fresh, Table};
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::{deep, sample};
use std::collections::BTreeSet;

const SPEARMEN: u16 = 104; // high_men_spearmen
const SETTLERS: u16 = 44; // barbarian_settlers
const GHOULS: u16 = 166; // ghouls
const DEMON: u16 = 169; // demon
const MAGIC_SPIRIT: u16 = 154; // magic_spirit

/// A round starts as the game starts it: the world is tidied up after, as after a command.
fn start_round(w: &mut Table, round: u32) -> Vec<Event> {
    let (world, rng) = w.world_and_rng();
    world.advance(|world| world.start_round(round, rng))
}

/// The worlds with the threats and events a test does not ask for switched off.
fn quiet(raiders: bool, monsters: bool) -> Vec<(String, Table)> {
    worlds()
        .into_iter()
        .map(|(name, mut w)| {
            w.scenario_change_settings(|g| {
                g.events.random_events = false;
                g.world.raiders_enabled = raiders;
                g.world.monsters_enabled = monsters;
                g.world.sea_raids = false;
            });
            (name, w)
        })
        .collect()
}

fn create(w: &mut Table, owner: PlayerId, pos: Position, unit: u16) -> hecs::Entity {
    let before = w.units().units_at(pos);
    let events = w.apply(Command::CreateUnit {
        owner,
        pos,
        unit: UnitId::new(unit),
    });
    let made = w
        .units()
        .units_at(pos)
        .into_iter()
        .find(|u| !before.contains(u));
    made.unwrap_or_else(|| panic!("no unit was made: {events:?}"))
}

fn new_units(w: &World, before: &BTreeSet<hecs::Entity>) -> Vec<hecs::Entity> {
    w.units()
        .all_units()
        .into_iter()
        .filter(|u| !before.contains(u))
        .collect()
}

fn all_units(w: &World) -> BTreeSet<hecs::Entity> {
    w.units().all_units().into_iter().collect()
}

/// A raid of one round: what was raised and by whom.
struct Raid {
    round: u32,
    city: hecs::Entity,
    owner: PlayerId,
    units: Vec<hecs::Entity>,
}

/// Run rounds from `from` until a city raises a party.
fn next_raid(w: &mut Table, from: u32, name: &str) -> Raid {
    for round in from..from + 200 {
        let before = all_units(w);
        let events = start_round(w, round);
        let raised = events.iter().find_map(|e| match e {
            Event::RaidersRaised { city, owner, count } => Some((*city, *owner, *count)),
            _ => None,
        });
        if let Some((city, owner, count)) = raised {
            let units = new_units(w, &before);
            assert_eq!(units.len() as u32, count, "{name}: round {round}");
            return Raid {
                round,
                city,
                owner,
                units,
            };
        }
    }
    panic!("{name}: no raid in 200 rounds");
}

// ---- raiders ----

#[test]
fn the_raider_clock_runs_to_its_limit_and_starts_again() {
    for (name, mut w) in quiet(true, false) {
        let limit = w.content().rules.config.threat.raider_limit;
        let most = u32::from(w.difficulty()) + 1;
        let mut raids = 0;
        for round in 2..2 + sample(600, 150) as u32 {
            let before = w.threat().raider_clock;
            let events = start_round(&mut w, round);
            let now = w.threat().raider_clock;
            if now == 0 {
                assert!(before + most >= limit, "{name}: {before} at round {round}");
                raids += events
                    .iter()
                    .filter(|e| matches!(e, Event::RaidersRaised { .. }))
                    .count();
            } else {
                assert!(
                    now > before && now - before <= most && now < limit,
                    "{name}: {before} to {now}"
                );
                assert!(
                    !events
                        .iter()
                        .any(|e| matches!(e, Event::RaidersRaised { .. })),
                    "{name}: a raid below the limit"
                );
            }
        }
        assert!(raids > 0, "{name}: no raid");
    }
}

#[test]
fn a_raiding_party_is_new_free_troops_of_its_citys_own_kinds() {
    for (name, mut w) in quiet(true, false) {
        let mut from = 2;
        for _ in 0..sample(6, 3) {
            let upkeep =
                |w: &World, city| w.economy().city_economy(city).expect("a city").gold_upkeep;
            // What each neutral city has before the party comes.
            let before: Vec<(hecs::Entity, BTreeSet<UnitId>, HalfInt)> = w
                .economy()
                .cities()
                .into_iter()
                .filter(|c| w.is_neutral_player(c.1.owner))
                .map(|(c, _, at)| {
                    let kinds = w
                        .units()
                        .units_at(at)
                        .into_iter()
                        .filter_map(|u| w.units().unit_kind_id(u))
                        .collect();
                    (c, kinds, upkeep(&w, c))
                })
                .collect();
            let raid = next_raid(&mut w, from, &name);
            from = raid.round + 1;
            let (_, kinds, paid) = before
                .iter()
                .find(|c| c.0 == raid.city)
                .unwrap_or_else(|| panic!("{name}: not a neutral city"));
            assert!(w.is_neutral_player(raid.owner), "{name}");
            assert_ne!(raid.owner, MONSTERS, "{name}");
            assert_eq!(
                upkeep(&w, raid.city),
                *paid,
                "{name}: the party is paid for"
            );
            let home = w
                .economy()
                .cities()
                .into_iter()
                .find(|c| c.0 == raid.city)
                .expect("the city")
                .2;
            assert!(!raid.units.is_empty(), "{name}");
            for &u in &raid.units {
                assert_eq!(w.units().unit_owner(u), Some(raid.owner), "{name}");
                assert_eq!(w.units().unit_position(u), Some(home), "{name}");
                let kind = w.units().unit_kind_id(u).expect("a kind");
                assert!(kinds.contains(&kind), "{name}: {kind:?} was not there");
                let d = w.units().unit_def(u).expect("a unit");
                assert_eq!(d.kind, UnitKind::Normal, "{name}");
                assert!(!d.is_settler(), "{name}");
                assert_eq!(w.units().unit_stat(u, Stat::Transport), 0, "{name}");
            }
        }
    }
}

#[test]
fn the_raider_setting_sizes_the_party() {
    // The same draws in the same world: only the setting differs.
    let size = |percent: u16| {
        quiet(true, false)
            .into_iter()
            .map(|(name, mut w)| {
                w.scenario_change_settings(|g| g.world.raider_percent = percent);
                next_raid(&mut w, 2, &name).units.len()
            })
            .collect::<Vec<_>>()
    };
    let (none, usual, large) = (size(0), size(100), size(300));
    for ((n, u), l) in none.iter().zip(&usual).zip(&large) {
        assert_eq!(*n, 1, "a party is never empty");
        assert!(n <= u && u <= l, "{none:?} {usual:?} {large:?}");
    }
    assert!(
        large.iter().sum::<usize>() > usual.iter().sum::<usize>(),
        "{usual:?} {large:?}"
    );
}

#[test]
fn later_raiders_are_the_more_experienced() {
    let experience = |round: u32| -> u32 {
        quiet(true, false)
            .into_iter()
            .map(|(name, mut w)| {
                let raid = next_raid(&mut w, round, &name);
                raid.units
                    .iter()
                    .map(|&u| w.units().unit_experience(u))
                    .sum::<u32>()
                    / raid.units.len() as u32
            })
            .sum()
    };
    let by_age: Vec<u32> = [2, 60, 150, 300].into_iter().map(experience).collect();
    assert!(
        by_age.windows(2).all(|w| w[0] <= w[1]) && by_age[0] < by_age[3],
        "{by_age:?}"
    );
}

#[test]
fn cities_with_nobody_fit_to_raid_raise_no_party_and_feed_the_monsters_clock() {
    for (name, mut w) in quiet(true, false) {
        // The neutral cities hold settlers and nothing else.
        let neutral: Vec<(PlayerId, Position)> = w
            .economy()
            .cities()
            .into_iter()
            .filter(|c| w.is_neutral_player(c.1.owner))
            .map(|(_, c, at)| (c.owner, at))
            .collect();
        for &(owner, at) in &neutral {
            for u in w.units().units_at(at) {
                w.cheat_remove_unit(u);
            }
            create(&mut w, owner, at, SETTLERS);
        }
        let start = w.threat().monster_clock;
        for round in 2..200 {
            let events = start_round(&mut w, round);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::RaidersRaised { .. })),
                "{name}: round {round}"
            );
        }
        assert!(w.threat().monster_clock > start, "{name}");
    }
}

#[test]
fn a_threat_switched_off_does_not_run_its_clock() {
    for (name, mut w) in quiet(false, false) {
        let before = w.threat();
        let units = all_units(&w);
        for round in 2..sample(500, 150) as u32 {
            start_round(&mut w, round);
        }
        assert_eq!(w.threat(), before, "{name}");
        assert_eq!(all_units(&w), units, "{name}");
    }
}

#[test]
fn the_clocks_and_the_free_troops_survive_a_save() -> Result<(), Box<dyn std::error::Error>> {
    for (name, mut w) in quiet(true, true) {
        let raid = next_raid(&mut w, 2, &name);
        start_round(&mut w, raid.round + 1);
        let saved = w.snapshot().to_text()?;
        let back = Snapshot::from_text(&saved)?.restore(content())?;
        assert_eq!(back.threat(), w.threat(), "{name}");
        assert_eq!(text(&back), saved, "{name}");
    }
    Ok(())
}

/// Whether a wizard has a city on the landmass of `at`, on its plane.
fn player_on_land(w: &World, at: Position) -> bool {
    let mass = w.landmass_of(at);
    w.economy().cities().into_iter().any(|(_, c, p)| {
        !w.is_neutral_player(c.owner) && p.plane == at.plane && w.landmass_of(p) == mass
    })
}

#[test]
fn a_party_is_raised_only_on_a_landmass_with_a_players_city() {
    for (name, mut w) in quiet(true, false) {
        let mut from = 2;
        for _ in 0..100 {
            let raid = next_raid(&mut w, from, &name);
            from = raid.round + 1;
            let home = w.units().unit_position(raid.units[0]).expect("a square");
            assert!(player_on_land(&w, home), "{name}: {home:?}");
            for u in raid.units {
                w.cheat_remove_unit(u);
            }
        }
    }
}

// ---- raiders from the sea ----

/// A landing of one round.
struct Landing {
    round: u32,
    owner: PlayerId,
    at: Position,
    count: u32,
    units: Vec<hecs::Entity>,
}

fn landings(skip: usize, from: u32, most: usize) -> (Table, Vec<Landing>) {
    let (wizards, seed) = WORLDS[0];
    let mut w = fresh(wizards, seed).expect("a fresh game");
    w.scenario_change_settings(|g| {
        g.events.random_events = false;
        g.world.monsters_enabled = false;
        g.world.sea_raids = true;
    });
    for _ in 0..skip {
        w.next_u64();
    }
    let mut found = Vec::new();
    for round in from..from + 400 {
        let before = all_units(&w);
        for e in start_round(&mut w, round) {
            if let Event::SeaRaidLanded { owner, at, count } = e {
                found.push(Landing {
                    round,
                    owner,
                    at,
                    count,
                    units: new_units(&w, &before),
                });
            }
        }
        if found.len() == most {
            break;
        }
    }
    (w, found)
}

#[test]
fn a_sea_raid_lands_fighters_and_a_settler_with_a_ship_beside_them_all_free() {
    // Early, middle and late: each step of the content's parties.
    let mut planes = BTreeSet::new();
    for from in [2, 130, 300] {
        for skip in 0..sample(30, 6) {
            let (w, found) = landings(skip, from, 2);
            assert!(!found.is_empty(), "round {from}: no landing");
            for l in &found {
                let what = format!("draw {skip}, round {}", l.round);
                let rules = &w.content().rules;
                let raid = &rules.config.threat.sea_raid;
                let &(_, base, sides, pool) = raid
                    .parties
                    .iter()
                    .rfind(|p| l.round >= p.0)
                    .expect("a party for the round");
                assert!((base + 1..=base + sides).contains(&l.count), "{what}");
                assert_eq!(l.units.len() as u32, l.count + 2, "{what}");
                planes.insert(l.at.plane.index());
                assert!(w.is_neutral_player(l.owner), "{what}");
                let race = w.race_of_player(l.owner).expect("a race");
                let ship = raid
                    .ships
                    .iter()
                    .rfind(|s| l.round >= s.0)
                    .expect("a ship for the round")
                    .1;
                let (mut ships, mut settlers) = (0, 0);
                for &u in &l.units {
                    assert_eq!(w.units().unit_owner(u), Some(l.owner), "{what}");
                    let at = w.units().unit_position(u).expect("a square");
                    let kind = w.units().unit_kind_id(u).expect("a kind");
                    let d = rules.units.get(kind);
                    if d.num == ship {
                        ships += 1;
                        assert!(around(&w, l.at).contains(&at), "{what}: the ship");
                        assert_eq!(
                            w.economy().terrain_class_at(at),
                            Some(TerrainClass::Ocean),
                            "{what}"
                        );
                        continue;
                    }
                    assert_eq!(at, l.at, "{what}");
                    assert!(rules.races.get(race).units.contains(&kind), "{what}");
                    if d.is_settler() {
                        settlers += 1;
                        continue;
                    }
                    // Among the cheapest of the race's fighters the step allows.
                    let mut fighters: Vec<u32> = rules
                        .races
                        .get(race)
                        .units
                        .iter()
                        .map(|&u| rules.units.get(u))
                        .filter(|d| {
                            d.kind == UnitKind::Normal
                                && !d.is_settler()
                                && d.stat(Stat::Transport) == 0.0
                                && d.stat(Stat::Melee).max(d.stat(Stat::Ranged)) > 0.0
                        })
                        .map(|d| d.cost)
                        .collect();
                    fighters.sort_unstable();
                    if pool > 0 {
                        let dearest = fighters[pool.min(fighters.len()) - 1];
                        assert!(d.cost <= dearest, "{what}: {}", d.id);
                    }
                }
                assert_eq!((ships, settlers), (1, 1), "{what}");
                assert_eq!(
                    w.economy()
                        .terrain_class_at(l.at)
                        .map(|c| c != TerrainClass::Ocean),
                    Some(true),
                    "{what}"
                );
                assert!(w.economy().city_at(l.at).is_none(), "{what}");
                assert!(w.economy().feature_entity_at(l.at).is_none(), "{what}");
            }
        }
    }
    assert_eq!(planes.len(), Plane::all().count(), "both planes are raided");
}

#[test]
fn with_sea_raids_off_none_lands() {
    for (name, mut w) in quiet(true, false) {
        for round in 2..sample(600, 200) as u32 {
            let events = start_round(&mut w, round);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::SeaRaidLanded { .. })),
                "{name}"
            );
        }
    }
}

// ---- monsters ----

/// Every unit of `side` falls, and the next command lets the battle see it.
fn wipe_out(w: &mut Table, side: Side) -> Vec<Event> {
    w.scenario_edit_battle(|b| {
        for u in b.units.iter_mut().filter(|u| u.side == side) {
            u.kill();
            u.status = f4x_battle::unit::Status::Dead;
        }
    });
    w.apply(Command::Battle(BattleCommand::EndTurn))
}

/// The turn from which monsters move in a world.
fn first_turn(w: &World) -> u32 {
    let setting = w.settings().world.monster_start_turn;
    // On the easier levels the original's turn holds, whatever the setting says.
    if w.difficulty() >= 2 {
        u32::from(setting)
    } else {
        u32::from(f4x_def::settings::MONSTER_START_TURN)
    }
}

#[test]
fn no_monsters_move_before_their_first_turn() {
    for (name, mut w) in quiet(false, true) {
        let first = first_turn(&w);
        let mut reset = false;
        for round in 1..first {
            let before = w.threat().monster_clock;
            let events = start_round(&mut w, round);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::MonstersSettled { .. })),
                "{name}: round {round} of {first}"
            );
            reset |= w.threat().monster_clock < before;
        }
        assert!(reset || first < 20, "{name}: the clock never ran out");
    }
}

#[test]
fn monsters_take_over_a_lair_near_a_players_city_and_drive_out_its_guards() {
    for (name, mut w) in quiet(false, true) {
        let first = first_turn(&w);
        let mut taken = 0;
        for round in first..first + sample(2000, 600) as u32 {
            let lairs = w.economy().lairs();
            let units = all_units(&w);
            let events = start_round(&mut w, round);
            let Some((pos, count, evicted)) = events.iter().find_map(|e| match e {
                Event::MonstersSettled {
                    pos,
                    count,
                    evicted,
                    ..
                } => Some((*pos, *count, *evicted)),
                _ => None,
            }) else {
                continue;
            };
            taken += 1;
            let what = format!("{name}: round {round} at {pos:?}");
            let rules = &w.content().rules;
            let old = lairs.iter().find(|l| l.pos == pos).expect("a lair there");
            let new = w.economy().lair_at(pos).expect("a lair there");
            let realm = |u: UnitId| rules.units.get(u).realm;
            let was = realm(old.guards[0].0).expect("a realm");
            assert!(
                !rules.config.threat.spared_realms.contains(&was.num()),
                "{what}: its monsters stay home"
            );
            assert_eq!(new.guard_count(), count, "{what}");
            assert!(count > 0 && count as usize <= w.max_battle_side(), "{what}");
            assert!(new.guards.iter().all(|g| realm(g.0) == Some(was)), "{what}");
            assert!(new.fought_by.is_empty(), "{what}");
            assert_eq!(evicted, old.guard_count(), "{what}");
            let out = new_units(&w, &units);
            assert_eq!(out.len() as u32, evicted, "{what}");
            for u in out {
                assert_eq!(w.units().unit_owner(u), Some(MONSTERS), "{what}");
                let at = w.units().unit_position(u).expect("a square");
                assert!(around(&w, pos).contains(&at), "{what}");
            }
        }
        assert!(taken > 0, "{name}: no monsters moved");
    }
}

fn all_squares(w: &World) -> impl Iterator<Item = Position> {
    let map = w.map().expect("a map");
    let (width, height) = (map.width(), map.height());
    (0..height).flat_map(move |y| {
        (0..width).map(move |x| Position {
            x,
            y,
            plane: Plane::Arcanus,
        })
    })
}

/// Dry land with nothing on it.
fn open_land(w: &World, q: Position) -> bool {
    w.economy()
        .terrain_class_at(q)
        .is_some_and(|c| c != TerrainClass::Ocean)
        && w.units().units_at(q).is_empty()
        && w.economy().city_at(q).is_none()
        && w.economy().feature_entity_at(q).is_none()
}

#[test]
fn monsters_settle_only_on_a_landmass_with_a_players_city() {
    for (name, mut w) in quiet(false, true) {
        let first = first_turn(&w);
        for round in first..first + 2000 {
            for e in start_round(&mut w, round) {
                if let Event::MonstersSettled { pos, .. } = e {
                    assert!(player_on_land(&w, pos), "{name}: round {round}, {pos:?}");
                }
            }
        }
    }
}

/// Units of the monsters beside a square, ready to walk, and the square they stand on.
fn monsters_beside(w: &mut Table, to: Position, kinds: &[u16]) -> Option<Vec<hecs::Entity>> {
    let from = around(w, to).into_iter().find(|&q| open_land(w, q))?;
    Some(
        kinds
            .iter()
            .map(|&k| {
                let u = create(w, MONSTERS, from, k);
                w.scenario_set_moves_left(u, HalfInt::from_int(20));
                u
            })
            .collect(),
    )
}

/// The monsters walk onto `to`; what they settled there, if they did.
fn walk_on(w: &mut Table, to: Position, name: &str) -> Option<u32> {
    let monsters = monsters_beside(w, to, &[GHOULS, GHOULS, DEMON])
        .unwrap_or_else(|| panic!("{name}: no room"));
    let events = w.apply(Command::MoveStack {
        units: monsters.clone(),
        to,
    });
    let settled = events.iter().find_map(|e| match e {
        Event::MonstersSettled {
            pos,
            count,
            evicted,
            ..
        } if *pos == to => {
            assert_eq!(*evicted, 0, "{name}");
            Some(*count)
        }
        _ => None,
    });
    if settled.is_some() {
        assert!(monsters.iter().all(|u| !all_units(w).contains(u)), "{name}");
    }
    settled
}

#[test]
fn roaming_monsters_settle_in_a_lair_that_stands_empty() {
    for (name, mut w) in quiet(false, false) {
        // A site made with no guards: nobody holds it.
        w.scenario_change_settings(|g| {
            g.rules.stronger_lairs = false;
            g.world.lair_guard_percent = 0;
        });
        let nodes: Vec<Position> = w.economy().nodes().iter().map(|n| n.pos).collect();
        let site = all_squares(&w)
            .find(|&q| {
                open_land(&w, q)
                    && !nodes.contains(&q)
                    && around(&w, q).into_iter().any(|r| open_land(&w, r))
            })
            .expect("open land");
        let encounter = w
            .content()
            .features
            .first_where(|d| d.kind == FeatureKind::Encounter)
            .expect("an encounter site");
        w.cheat_add_lair(site, encounter, true)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(w.vacant_zone(site).is_some(), "{name}");
        assert_eq!(walk_on(&mut w, site, &name), Some(3), "{name}");
        let lair = w.economy().lair_at(site).expect("guards");
        assert_eq!(lair.guard_count(), 3, "{name}");
        assert_eq!(
            (lair.gold, lair.mana, lair.items.len(), lair.prisoner),
            (0, 0, 0, false),
            "{name}"
        );
        assert_eq!(w.vacant_zone(site), None, "{name}: it is held now");
    }
}

/// A node the wizard's army has taken from its guards and left again.
fn cleared_node(w: &mut Table, p: PlayerId, name: &str) -> Position {
    let node = w
        .economy()
        .nodes()
        .into_iter()
        .find(|n| {
            n.owner.is_none()
                && w.units().units_at(n.pos).is_empty()
                && around(w, n.pos).into_iter().any(|q| open_land(w, q))
                && w.economy()
                    .lair_at(n.pos)
                    .is_some_and(|l| !l.guards.is_empty())
        })
        .unwrap_or_else(|| panic!("{name}: no guarded node beside open land"))
        .pos;
    let beside = around(w, node)
        .into_iter()
        .find(|&q| open_land(w, q))
        .expect("open land");
    let victor = create(w, p, beside, SPEARMEN);
    let events = w.apply(Command::EnterSite {
        units: vec![victor],
        to: node,
    });
    assert!(w.in_battle(), "{name}: {events:?}");
    wipe_out(w, Side::Defender);
    assert_eq!(w.economy().lair_at(node), None, "{name}: cleared");
    for u in w.units().units_at(node) {
        w.cheat_remove_unit(u);
    }
    node
}

#[test]
fn monsters_do_not_settle_in_a_node_a_wizard_holds() {
    for (name, mut w) in quiet(false, false) {
        let p = w.wizards()[0];
        let node = cleared_node(&mut w, p, &name);
        let spirit = create(&mut w, p, node, MAGIC_SPIRIT);
        w.apply(Command::Meld {
            units: vec![spirit],
        });
        assert_eq!(w.vacant_zone(node), None, "{name}");
        assert_eq!(walk_on(&mut w, node, &name), None, "{name}");
        let held_by = w.economy().node_at(node).expect("the node").owner;
        assert_eq!(held_by, Some(p), "{name}");
    }
}

#[test]
#[ignore = "task 829: a cleared node keeps its guard list and never counts as empty"]
fn roaming_monsters_settle_in_a_cleared_node_nobody_holds() {
    for (name, mut w) in quiet(false, false) {
        let p = w.wizards()[0];
        let node = cleared_node(&mut w, p, &name);
        assert!(w.vacant_zone(node).is_some(), "{name}");
        assert_eq!(walk_on(&mut w, node, &name), Some(3), "{name}");
    }
}

/// A city of a computer wizard or a neutral one, emptied, and monsters beside it.
fn sack(
    w: &mut Table,
    fortress: bool,
    name: &str,
) -> (hecs::Entity, Position, PlayerId, Vec<hecs::Entity>) {
    let found = w
        .economy()
        .cities()
        .into_iter()
        .filter(|(c, ..)| w.economy().is_fortress_city(*c) == fortress)
        .map(|(c, info, at)| (c, at, info.owner))
        .collect::<Vec<_>>();
    for (city, at, owner) in found {
        for u in w.units().units_at(at) {
            w.cheat_remove_unit(u);
        }
        w.scenario_set_citizens(city, 8);
        if let Some(monsters) = monsters_beside(w, at, &[GHOULS, DEMON, DEMON, 165]) {
            return (city, at, owner, monsters);
        }
    }
    panic!("{name}: no city to sack");
}

#[test]
fn monsters_that_take_a_city_wreck_it_vanish_and_sometimes_leave_ruins() {
    for fortress in [false, true] {
        let (mut ruined, mut spared) = (0, 0);
        for skip in 0..if deep() { 80 } else { 24 } {
            let (wizards, seed) = WORLDS[0];
            let mut w = fresh(wizards, seed).expect("a fresh game");
            for _ in 0..skip {
                w.next_u64();
            }
            let name = format!("draw {skip}, fortress {fortress}");
            let (city, at, owner, monsters) = sack(&mut w, fortress, &name);
            let before = w.city_population(city).figures;
            let events = w.apply(Command::MoveStack {
                units: monsters.clone(),
                to: at,
            });
            let ruin = events
                .iter()
                .find_map(|e| match e {
                    Event::CityRampaged {
                        pos,
                        ruined,
                        owner: o,
                        ..
                    } => {
                        assert_eq!((*pos, *o), (at, owner), "{name}");
                        Some(*ruined)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name}: {events:?}"));
            assert!(
                monsters.iter().all(|u| !all_units(&w).contains(u)),
                "{name}: they stay"
            );
            if ruin {
                ruined += 1;
                assert!(w.economy().city_at(at).is_none(), "{name}");
                let lair = w.economy().lair_at(at).expect("ruins with guards");
                assert!((1..=2).contains(&lair.guards.len()), "{name}: {lair:?}");
            } else {
                spared += 1;
                assert_eq!(w.economy().city_owner(city), Some(owner), "{name}");
                assert!(w.city_population(city).figures <= before, "{name}");
                assert!(w.units().units_at(at).is_empty(), "{name}");
            }
        }
        if fortress {
            assert_eq!(ruined, 0, "a fortress city is never ruined");
            assert!(spared > 0);
        } else {
            assert!(ruined > 0 && spared > 0, "ruined {ruined}, spared {spared}");
        }
    }
}

#[test]
fn monsters_that_beat_the_defenders_sack_the_city() {
    for (name, mut w) in quiet(false, false) {
        let p = w.wizards()[0];
        let (city, at) = w.economy().cities_of(p)[0];
        create(&mut w, p, at, SPEARMEN);
        let monsters = monsters_beside(&mut w, at, &[DEMON, DEMON])
            .unwrap_or_else(|| panic!("{name}: no room"));
        let mut events = w.apply(Command::MoveStack {
            units: monsters.clone(),
            to: at,
        });
        let defenders = w.units().units_at(at);
        if !w.in_battle() {
            let started = w.start_battle(&monsters, &defenders);
            events.extend(started.unwrap_or_else(|e| panic!("{name}: {e:?}")));
        }
        assert!(w.in_battle(), "{name}: {events:?}");
        let events = wipe_out(&mut w, Side::Defender);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityRampaged { pos, .. } if *pos == at)),
            "{name}: {events:?}"
        );
        // A fortress city stands, in its wizard's hands.
        assert_eq!(w.economy().city_owner(city), Some(p), "{name}");
        assert!(
            w.units()
                .units_at(at)
                .iter()
                .all(|&u| w.units().unit_owner(u) != Some(MONSTERS)),
            "{name}"
        );
    }
}

// ---- without lairs ----

/// Without lairs no monsters roam, whatever their own setting says.
#[test]
fn without_lairs_no_monsters_leave_a_site() {
    for (name, mut w) in quiet(false, true) {
        w.scenario_change_settings(|g| g.world.lairs_enabled = false);
        assert!(w.settings().world.monsters_enabled, "{name}");
        let first = first_turn(&w);
        for round in first..first + sample(2000, 600) as u32 {
            let events = start_round(&mut w, round);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::MonstersSettled { .. })),
                "{name}: round {round}"
            );
        }
        assert_eq!(w.threat().monster_clock, 0, "{name}");
    }
}

#[test]
fn without_lairs_a_sacked_city_leaves_no_ruins() {
    for skip in 0..if deep() { 80 } else { 24 } {
        let (wizards, seed) = WORLDS[0];
        let mut w = fresh(wizards, seed).expect("a fresh game");
        for _ in 0..skip {
            w.next_u64();
        }
        w.scenario_change_settings(|g| g.world.lairs_enabled = false);
        let name = format!("draw {skip}");
        let (_, at, _, monsters) = sack(&mut w, false, &name);
        let events = w.apply(Command::MoveStack {
            units: monsters,
            to: at,
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityRampaged { ruined: false, .. })),
            "{name}: {events:?}"
        );
        assert!(w.economy().lair_at(at).is_none(), "{name}");
    }
}
