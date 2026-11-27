//! Class 9, the turn: the world's operations at the boundaries of a turn and what each one touches
//! (the round's start and end, one player's start and end), Time Stop, standing orders, standby, and
//! a treasury that cannot pay. The order of the operations and the round counter are the game's
//! (`f4x-master/tests/it`).

use crate::support::{around, content, state, worlds};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::{Position, TerrainId};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::rules::UnitId;
use f4x_core::unit::movement::MoveError;
use f4x_core::unit::Standby;
use f4x_core::world::rng::GameRng;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;

fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

fn gold(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().gold
}

fn mana(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().mana
}

/// No city has anything in hand, so that no item is finished and changes the gold at a turn's end.
fn idle_cities(w: &mut World) {
    for (city, _, _) in w.economy().cities() {
        w.scenario_clear_production(city);
    }
}

/// The population of every city: (figures, tens of growth), by owner.
fn people(w: &World, p: PlayerId) -> Vec<(u32, u32)> {
    w.economy()
        .cities_of(p)
        .into_iter()
        .map(|(c, _)| {
            let pop = w.city_population(c);
            (pop.figures, pop.tens)
        })
        .collect()
}

#[test]
fn the_end_of_a_player_turn_settles_only_that_players_treasury() {
    let mut moved = 0;
    for (name, mut w) in worlds() {
        idle_cities(&mut w);
        let ids = wizards(&w);
        let before: Vec<_> = ids.iter().map(|&p| (gold(&w, p), mana(&w, p))).collect();
        let (p, others) = (ids[0], &ids[1..]);
        let net = w.budget(p).net_gold();
        let events = w.end_player_turn(p);
        let let_go = events
            .iter()
            .any(|e| matches!(e, Event::Shortage { .. } | Event::UnitsDisbanded { .. }));
        if !let_go {
            assert_eq!(
                gold(&w, p),
                (before[0].0 + net).max(HalfInt::ZERO),
                "{name}: the budget said {net:?}"
            );
        }
        moved += usize::from(gold(&w, p) != before[0].0);
        for (i, &o) in others.iter().enumerate() {
            assert_eq!(
                (gold(&w, o), mana(&w, o)),
                before[i + 1],
                "{name}: the end of {p:?}'s turn changed {o:?}"
            );
        }
    }
    assert!(moved > 0, "no turn's end changed any treasury");
}

#[test]
fn the_start_of_a_player_turn_grows_only_their_cities_and_not_in_the_first_round() {
    let mut grew = 0;
    for (name, mut w) in worlds() {
        let ids = wizards(&w);
        let (p, others) = (ids[0], &ids[1..]);
        let first = (
            people(&w, p),
            others.iter().map(|&o| people(&w, o)).collect::<Vec<_>>(),
        );
        w.start_player_turn(p, 1);
        assert_eq!(
            people(&w, p),
            first.0,
            "{name}: the first round has no growth"
        );
        w.start_round(2);
        w.start_player_turn(p, 2);
        grew += usize::from(people(&w, p) != first.0);
        for (i, &o) in others.iter().enumerate() {
            assert_eq!(
                people(&w, o),
                first.1[i],
                "{name}: {p:?}'s start grew a city of {o:?}"
            );
        }
    }
    assert!(grew > 0, "no city grew in any world");
}

#[test]
fn the_start_of_a_player_turn_refills_only_their_units_moves() {
    for (name, mut w) in worlds() {
        let ids = wizards(&w);
        let (p, o) = (ids[0], ids[1]);
        let mine: Vec<_> = w.units().player_units(p);
        let theirs: Vec<_> = w.units().player_units(o);
        for &u in mine.iter().chain(&theirs) {
            w.scenario_set_moves_left(u, HalfInt::ZERO);
        }
        w.start_player_turn(o, 1);
        for &u in &mine {
            assert_eq!(w.units().unit_moves_left(u), HalfInt::ZERO, "{name}");
        }
        for &u in &theirs {
            assert_eq!(
                w.units().unit_moves_left(u),
                w.units().unit_max_moves(u),
                "{name}: a unit of {o:?} was not refilled"
            );
        }
    }
}

#[test]
fn standby_done_lasts_the_turn_and_patrol_stays() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let units = w.units().player_units(p);
        let (done, patrol) = (units[0], *units.last().expect("a unit"));
        if done == patrol {
            continue;
        }
        for (u, s) in [(done, Standby::Done), (patrol, Standby::Patrol)] {
            w.apply(Command::SetStandby {
                units: vec![u],
                standby: Some(s),
            });
            assert_eq!(w.units().unit_standby(u), Some(s), "{name}");
        }
        w.apply(Command::SetStandby {
            units: vec![patrol],
            standby: Some(Standby::Done),
        });
        assert_eq!(
            w.units().unit_standby(patrol),
            Some(Standby::Patrol),
            "{name}: Done ended a patrol"
        );
        w.start_round(2);
        w.start_player_turn(p, 2);
        assert_eq!(
            w.units().unit_standby(done),
            None,
            "{name}: Done outlasted the turn"
        );
        assert_eq!(
            w.units().unit_standby(patrol),
            Some(Standby::Patrol),
            "{name}: Patrol went at the turn's start"
        );
    }
}

#[test]
fn the_player_of_the_monsters_has_no_turn_end() {
    for (name, mut w) in worlds() {
        let before = state(&w);
        w.end_player_turn(MONSTERS);
        assert_eq!(
            state(&w),
            before,
            "{name}: the monsters' turn end changed the game"
        );
    }
}

/// Wizard 0 keeps Time Stop (spell 80, `time_stop`) with mana to pay for it.
fn stopped() -> Vec<(String, Table, Vec<PlayerId>)> {
    worlds()
        .into_iter()
        .map(|(name, mut w)| {
            let ids = wizards(&w);
            for &p in &ids {
                let t = w.treasury_mut(p).expect("a treasury");
                t.gold = HalfInt::from_int(500);
                t.mana = HalfInt::from_int(2000);
            }
            let spell = w.content().rules.spells.id_numbered(80).expect("time_stop");
            w.scenario_add_global(ids[0], spell);
            idle_cities(&mut w);
            (name, w, ids)
        })
        .collect()
}

#[test]
fn a_standing_order_does_not_move_a_unit_while_another_wizard_keeps_time_stop() {
    let mut tried = 0;
    for (name, mut w, ids) in stopped() {
        let (keeper, other) = (ids[0], ids[1]);
        let walk = |w: &mut Table, p: PlayerId| {
            let u = w.units().player_units(p).into_iter().find(|&u| {
                w.units().unit_max_moves(u) > HalfInt::ZERO
                    && w.units()
                        .unit_def(u)
                        .is_some_and(|d| d.movement == f4x_def::rules::MovementType::Walking)
            })?;
            let at = w.units().unit_position(u)?;
            // A square three steps off, so that the order is still running after its first step.
            let ring = |w: &World, from: Vec<_>| -> Vec<_> {
                from.into_iter().flat_map(|q| around(w, q)).collect()
            };
            let two = ring(w, around(w, at));
            let to = ring(w, two).into_iter().find(|&q| {
                w.map()
                    .expect("a map")
                    .plane(q.plane)
                    .chebyshev((q.x, q.y), (at.x, at.y))
                    == 3
                    && w.units().units_at(q).is_empty()
                    && w.economy().city_at(q).is_none()
                    && w.economy().terrain_class_at(q) == Some(TerrainClass::Grassland)
            })?;
            let events = w.apply(Command::GoTo { units: vec![u], to });
            // The order takes its first step when it is given: the test starts from where that left the unit.
            let ordered = events.iter().all(|e| !matches!(e, Event::Rejected(_)));
            ordered.then(|| (u, w.units().unit_position(u).expect("a position"), to))
        };
        let (Some((theirs, their_at, _)), Some((own, own_at, _))) =
            (walk(&mut w, other), walk(&mut w, keeper))
        else {
            continue;
        };
        tried += 1;
        w.start_round(2);
        w.start_player_turn(other, 2);
        w.end_player_turn(other);
        assert_eq!(
            w.units().unit_position(theirs),
            Some(their_at),
            "{name}: a unit walked in stopped time"
        );
        w.start_player_turn(keeper, 2);
        w.end_player_turn(keeper);
        assert_ne!(
            w.units().unit_position(own),
            Some(own_at),
            "{name}: the keeper's unit did not walk"
        );
    }
    assert!(tried > 0, "no world had two walkers with room to go");
}

/// Units are added for `p` on the square of their first city until the treasury cannot pay.
fn overloaded(w: &mut Table, p: PlayerId) -> Option<()> {
    let (_, at) = w.economy().cities_of(p).first().copied()?;
    let kind = w.units().unit_kind_id(w.units().player_units(p)[0])?;
    w.treasury_mut(p)?.gold = HalfInt::ZERO;
    for _ in 0..300 {
        if w.treasury_shortfall(p).is_some() {
            return Some(());
        }
        let made = w.apply(Command::CreateUnit {
            owner: p,
            pos: at,
            unit: kind,
        });
        if made.iter().any(|e| matches!(e, Event::Rejected(_))) {
            return None;
        }
    }
    None
}

#[test]
fn a_treasury_that_cannot_pay_lets_go_of_the_units_the_shortfall_named() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        idle_cities(&mut w);
        let p = wizards(&w)[0];
        if overloaded(&mut w, p).is_none() {
            continue;
        }
        tried += 1;
        let short = w.treasury_shortfall(p).expect("a shortfall");
        assert!(!short.default_release.is_empty(), "{name}: nothing named");
        let kept: Vec<_> = w
            .units()
            .player_units(p)
            .into_iter()
            .filter(|u| !short.default_release.contains(u))
            .collect();
        let events = w.end_player_turn(p);
        assert!(
            events.iter().any(|e| matches!(e, Event::Shortage { .. })),
            "{name}: {events:?}"
        );
        for u in &short.default_release {
            assert!(
                w.units().unit_position(*u).is_none(),
                "{name}: a named unit stayed"
            );
        }
        for u in kept {
            assert!(
                w.units().unit_position(u).is_some(),
                "{name}: an unnamed unit went"
            );
        }
        let told: u32 = events
            .iter()
            .filter_map(|e| match e {
                Event::UnitsDisbanded { player, units } if *player == p => Some(*units),
                _ => None,
            })
            .sum();
        assert_eq!(
            told as usize,
            short.default_release.len(),
            "{name}: {events:?}"
        );
        assert!(gold(&w, p) >= HalfInt::ZERO, "{name}");
    }
    assert!(tried > 0, "no world could be overloaded");
}

#[test]
fn a_player_may_choose_who_goes_and_only_among_their_own_units() {
    let mut tried = 0;
    for (name, mut w) in worlds() {
        idle_cities(&mut w);
        let ids = wizards(&w);
        let (p, o) = (ids[0], ids[1]);
        if overloaded(&mut w, p).is_none() {
            continue;
        }
        tried += 1;
        let theirs = w.units().player_units(o)[0];
        let before = state(&w);
        let events = w.apply(Command::ReleaseTroops {
            player: p,
            units: vec![theirs],
            scrap: Vec::new(),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Rejected(Rejection::Move(MoveError::BadSelection)))),
            "{name}: {events:?}"
        );
        assert_eq!(
            state(&w),
            before,
            "{name}: a refused release changed the game"
        );
        // The newest own unit, which the turn's end would let go of first, stays if the player says so.
        let own = w.units().player_units(p);
        let chosen = own[0];
        let events = w.apply(Command::ReleaseTroops {
            player: p,
            units: vec![chosen],
            scrap: Vec::new(),
        });
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::UnitsDisbanded { player, units: 1 } if *player == p)),
            "{name}: {events:?}"
        );
        assert!(w.units().unit_position(chosen).is_none(), "{name}");
    }
    assert!(tried > 0, "no world could be overloaded");
}

/// An empty field: no unit in the world, and a stretch of grassland far from every city and lair.
/// Returns the square in its middle.
fn open_field(w: &mut Table) -> Position {
    for u in w.units().unsorted_units() {
        w.scenario_remove_unit(u);
    }
    let cities: Vec<Position> = w.economy().cities().into_iter().map(|c| c.2).collect();
    let (width, height) = w
        .map()
        .map(|m| (m.width(), m.height()))
        .expect("a game has a map");
    let plane = cities[0].plane;
    let square = |x: u32, y: u32| Position { x, y, plane };
    let around = |at: Position| {
        (-FIELD..=FIELD).flat_map(move |dx| {
            (-FIELD..=FIELD).map(move |dy| {
                square(
                    (at.x as i32 + dx).rem_euclid(width as i32) as u32,
                    (at.y as i32 + dy) as u32,
                )
            })
        })
    };
    let middle = (FIELD as u32..height - FIELD as u32)
        .flat_map(|y| (0..width).map(move |x| square(x, y)))
        .find(|&at| around(at).all(|p| !cities.contains(&p) && w.economy().lair_at(p).is_none()))
        .expect("a stretch with no city and no lair");
    for at in around(middle) {
        w.scenario_set_terrain(at, TerrainId(4 /* grassland */));
    }
    middle
}

/// How far the field reaches from its middle, in squares: past the sight of the units used here.
const FIELD: i32 = 4;

fn put(w: &mut Table, owner: PlayerId, at: Position, unit: u16) -> hecs::Entity {
    w.apply(Command::CreateUnit {
        owner,
        pos: at,
        unit: UnitId::new(unit),
    })
    .iter()
    .find_map(|e| match e {
        Event::UnitCreated { unit, .. } => Some(*unit),
        _ => None,
    })
    .expect("a unit is made")
}

/// `at` moved by `dx`, `dy`.
fn off(w: &World, at: Position, dx: i32, dy: i32) -> Position {
    let width = w.map().expect("a game has a map").width() as i32;
    Position {
        x: (at.x as i32 + dx).rem_euclid(width) as u32,
        y: (at.y as i32 + dy) as u32,
        plane: at.plane,
    }
}

fn patrol(w: &mut Table, unit: hecs::Entity) {
    w.apply(Command::SetStandby {
        units: vec![unit],
        standby: Some(Standby::Patrol),
    });
}

#[test]
fn patrol_ends_when_another_players_unit_comes_into_sight() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let watcher = put(&mut w, p, at, 104 /* high_men_spearmen */);
        let done = put(&mut w, p, at, 104 /* high_men_spearmen */);
        patrol(&mut w, watcher);
        w.apply(Command::SetStandby {
            units: vec![done],
            standby: Some(Standby::Done),
        });
        let far = w.units().unit_sight(watcher).floor() + 1;

        // One's own unit is company enough to stay, and so is a stranger just out of sight, even
        // when another unit of the owner stands beside it.
        let beyond = off(&w, at, far, 0);
        let square = off(&w, at, 1, 0);
        put(&mut w, p, square, 104 /* high_men_spearmen */);
        let square = off(&w, at, far, 1);
        let scout = put(&mut w, p, square, 104 /* high_men_spearmen */);
        put(&mut w, MONSTERS, beyond, 104 /* high_men_spearmen */);
        assert_eq!(
            w.units().unit_standby(watcher),
            Some(Standby::Patrol),
            "{name}: Patrol ended with no stranger in its sight"
        );
        assert!(w.units().unit_position(scout).is_some(), "{name}");

        let square = off(&w, at, far - 1, 0);
        put(&mut w, MONSTERS, square, 104 /* high_men_spearmen */);
        assert_eq!(
            w.units().unit_standby(watcher),
            None,
            "{name}: Patrol stayed with a stranger in sight"
        );
        assert_eq!(
            w.units().unit_standby(done),
            Some(Standby::Done),
            "{name}: Done is not ended by what is seen"
        );

        // Set again with the stranger still there, it holds: that one is known.
        patrol(&mut w, watcher);
        assert_eq!(
            w.units().unit_standby(watcher),
            Some(Standby::Patrol),
            "{name}: a stranger in view when Patrol was set ended it"
        );
        let text = w.snapshot().to_text().expect("writes");
        let again = Snapshot::from_text(&text)
            .expect("reads")
            .restore(content())
            .expect("restores");
        let mut again = Table::new(again, GameRng::new(1));
        let loaded = again.units().units_at(at);
        let loaded = loaded[0];
        again.apply(Command::ClearOrders { units: Vec::new() });
        assert_eq!(
            again.units().unit_standby(loaded),
            Some(Standby::Patrol),
            "{name}: the loaded game forgot what the patrol knew"
        );
        // Another one ends it.
        let square = off(&w, at, 0, 1);
        put(&mut w, MONSTERS, square, 104 /* high_men_spearmen */);
        assert_eq!(
            w.units().unit_standby(watcher),
            None,
            "{name}: Patrol stayed though a second stranger came into sight"
        );
    }
}

#[test]
fn patrol_ends_for_an_invisible_unit_only_when_it_can_be_made_out() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let watcher = put(&mut w, p, at, 104 /* high_men_spearmen */);
        patrol(&mut w, watcher);
        let square = off(&w, at, 1, 0);
        put(&mut w, MONSTERS, square, 194 /* air_elemental */);
        assert_eq!(
            w.units().unit_standby(watcher),
            Some(Standby::Patrol),
            "{name}: an unseen unit ended Patrol"
        );
        put(&mut w, p, at, 167 /* night_stalker */);
        assert_eq!(
            w.units().unit_standby(watcher),
            None,
            "{name}: Patrol stayed though the owner sees through the invisibility"
        );
    }
}

#[test]
fn patrol_ends_for_a_unit_that_only_passes_by() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let watcher = put(&mut w, p, at, 104 /* high_men_spearmen */);
        let far = w.units().unit_sight(watcher).floor() + 1;
        // A way of two steps, the first into the watcher's sight and the second out of it again,
        // with sea on every other side.
        let way = [
            off(&w, at, far, -1),
            off(&w, at, far - 1, 0),
            off(&w, at, far, 1),
        ];
        for dy in -2..=2 {
            for dx in far - 2..=far + 1 {
                let square = off(&w, at, dx, dy);
                if square != at && !way.contains(&square) {
                    w.scenario_set_terrain(square, TerrainId(8 /* ocean */));
                }
            }
        }
        let passer = put(&mut w, MONSTERS, way[0], 104 /* high_men_spearmen */);
        patrol(&mut w, watcher);
        assert_eq!(
            w.units().unit_standby(watcher),
            Some(Standby::Patrol),
            "{name}"
        );
        w.scenario_set_moves_left(passer, HalfInt::from_int(10));
        w.apply(Command::GoTo {
            units: vec![passer],
            to: way[2],
        });
        assert_eq!(w.units().unit_position(passer), Some(way[2]), "{name}");
        assert_eq!(
            w.units().unit_standby(watcher),
            None,
            "{name}: Patrol stayed though a stranger walked through its sight"
        );
    }
}

/// Whether a go-to for `unit` alone was taken.
fn go_to(w: &mut Table, unit: hecs::Entity, to: Position) -> bool {
    !w.apply(Command::GoTo {
        units: vec![unit],
        to,
    })
    .iter()
    .any(|e| matches!(e, Event::Rejected(_)))
}

#[test]
fn a_go_to_walks_on_at_the_end_of_the_owners_turn_and_not_at_its_start() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let from = off(&w, at, -FIELD, 0);
        let walker = put(&mut w, p, from, 104 /* high_men_spearmen */);
        let to = off(&w, at, FIELD, 0);
        assert!(go_to(&mut w, walker, to), "{name}");
        let first = w.units().unit_position(walker);
        assert!(first != Some(from) && first != Some(to), "{name}");
        w.start_player_turn(p, 2);
        assert_eq!(w.units().unit_position(walker), first, "{name}");
        w.end_player_turn(p);
        assert_ne!(w.units().unit_position(walker), first, "{name}");
        assert_eq!(w.unit_destination(walker), Some(to), "{name}");
    }
}

#[test]
fn a_walk_at_the_end_of_a_turn_never_attacks() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let walker = put(&mut w, p, at, 104 /* high_men_spearmen */);
        let there = off(&w, at, 1, 0);
        put(&mut w, MONSTERS, there, 104 /* high_men_spearmen */);
        w.scenario_set_moves_left(walker, HalfInt::ZERO);
        assert!(go_to(&mut w, walker, there), "{name}");
        assert!(w.unit_has_order(walker), "{name}");
        w.start_player_turn(p, 2);
        let events = w.end_player_turn(p);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::OrderFailed {
                    why: MoveError::WouldFight,
                    ..
                }
            )),
            "{name}: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::AttackRequested { .. })),
            "{name}"
        );
        assert_eq!(w.units().unit_position(walker), Some(at), "{name}");
        assert!(!w.unit_has_order(walker), "{name}");
    }
}

#[test]
fn an_order_ends_when_the_unit_is_moved_by_hand_or_told_to_patrol() {
    for (name, mut w) in worlds() {
        let p = wizards(&w)[0];
        let at = open_field(&mut w);
        let walker = put(&mut w, p, at, 104 /* high_men_spearmen */);
        let to = off(&w, at, FIELD, 0);
        let standby = |w: &mut Table, standby| {
            w.apply(Command::SetStandby {
                units: vec![walker],
                standby: Some(standby),
            });
        };
        assert!(go_to(&mut w, walker, to), "{name}");
        w.start_player_turn(p, 2);
        // A move the game refuses changes nothing.
        w.apply(Command::MoveStack {
            units: vec![walker],
            to,
        });
        assert!(w.unit_has_order(walker), "{name}: refused move");
        standby(&mut w, Standby::Done);
        assert!(w.unit_has_order(walker), "{name}: done");
        standby(&mut w, Standby::Patrol);
        assert!(!w.unit_has_order(walker), "{name}: patrol");

        assert!(go_to(&mut w, walker, to), "{name}");
        w.start_player_turn(p, 3);
        let here = w.units().unit_position(walker).expect("a square");
        let beside = off(&w, here, 0, 1);
        w.apply(Command::MoveStack {
            units: vec![walker],
            to: beside,
        });
        assert_eq!(w.units().unit_position(walker), Some(beside), "{name}");
        assert!(!w.unit_has_order(walker), "{name}: moved by hand");
    }
}
