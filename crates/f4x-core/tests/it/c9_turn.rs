//! Class 9, the turn: the four phases and what each one touches (the round's start and end, one player's
//! start and end), Time Stop, standing orders, standby, and a treasury that cannot pay.

use crate::support::{around, text, worlds};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::unit::movement::MoveError;
use f4x_core::unit::Standby;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_master::Game;

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
fn only_the_start_of_a_round_advances_the_round_counter() {
    for (name, mut w) in worlds() {
        let turn = w.turn;
        let p = wizards(&w)[0];
        w.end_player_turn(p);
        w.start_player_turn(p, w.turn);
        w.end_round();
        assert_eq!(
            w.turn, turn,
            "{name}: a player's turn or the round's end moved it"
        );
        let events = w.start_round(w.turn + 1);
        assert_eq!(w.turn, turn + 1, "{name}");
        assert!(
            events.contains(&Event::TurnStarted { turn: turn + 1 }),
            "{name}: {events:?}"
        );
    }
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
        assert_eq!(w.turn, 1, "{name}: the world is not at its first round");
        w.start_player_turn(p, w.turn);
        assert_eq!(
            people(&w, p),
            first.0,
            "{name}: the first round has no growth"
        );
        w.start_round(w.turn + 1);
        w.start_player_turn(p, w.turn);
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
        w.start_player_turn(o, w.turn);
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
        w.start_round(w.turn + 1);
        w.start_player_turn(p, w.turn);
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
        let before = text(&w);
        w.end_player_turn(MONSTERS);
        assert_eq!(
            text(&w),
            before,
            "{name}: the monsters' turn end changed the game"
        );
    }
}

/// Wizard 0 keeps Time Stop (spell 80, `time_stop`) with mana to pay for it.
fn stopped() -> Vec<(String, World, Vec<PlayerId>)> {
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
fn time_stop_halts_growth_gold_and_mana_for_the_others_and_gold_for_the_keeper() {
    for (name, w, ids) in stopped() {
        let mut w = Game::new(w);
        assert_eq!(w.time_stopper(), Some(ids[0]), "{name}");
        let (keeper, others) = (ids[0], &ids[1..]);
        let before: Vec<_> = ids
            .iter()
            .map(|&p| (gold(&w, p), mana(&w, p), people(&w, p)))
            .collect();
        for _ in 0..3 {
            w.play_round();
        }
        assert_eq!(
            gold(&w, keeper),
            before[0].0,
            "{name}: the keeper earned or paid gold"
        );
        assert!(
            mana(&w, keeper) < before[0].1,
            "{name}: Time Stop cost no mana"
        );
        for (i, &o) in others.iter().enumerate() {
            let at = &before[i + 1];
            assert_eq!(
                (gold(&w, o), mana(&w, o), people(&w, o)),
                (at.0, at.1, at.2.clone()),
                "{name}: time moved for {o:?}"
            );
        }
    }
}

#[test]
fn a_standing_order_does_not_move_a_unit_while_another_wizard_keeps_time_stop() {
    let mut tried = 0;
    for (name, mut w, ids) in stopped() {
        let (keeper, other) = (ids[0], ids[1]);
        let walk = |w: &mut World, p: PlayerId| {
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
        w.start_round(w.turn + 1);
        w.start_player_turn(other, w.turn);
        assert_eq!(
            w.units().unit_position(theirs),
            Some(their_at),
            "{name}: a unit walked in stopped time"
        );
        w.start_player_turn(keeper, w.turn);
        assert_ne!(
            w.units().unit_position(own),
            Some(own_at),
            "{name}: the keeper's unit did not walk"
        );
    }
    assert!(tried > 0, "no world had two walkers with room to go");
}

/// Units are added for `p` on the square of their first city until the treasury cannot pay.
fn overloaded(w: &mut World, p: PlayerId) -> Option<()> {
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
        let before = text(&w);
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
            text(&w),
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
