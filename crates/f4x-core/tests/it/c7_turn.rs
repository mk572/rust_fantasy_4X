//! Class 7: the turn does its arithmetic over everything, so it is checked where the numbers are big
//! or many: a tally against the sum of its parts, the players taken in another order, a treasury at the
//! end of its range, a city at its size cap, a game played far past its usual end.

use crate::support::games;
use crate::support::{text, worlds};
use f4x_core::check::world_problems;
use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};
use f4x_def::half_int::HalfInt;
use f4x_master::turn_players;

/// The players that keep a treasury: the wizards.
fn wizards(w: &World) -> Vec<PlayerId> {
    w.player_ids()
        .into_iter()
        .filter(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
        .collect()
}

/// No city has anything in hand, so that no item is finished and changes the gold at the turn end.
fn idle_cities(w: &mut World) {
    for (city, _, _) in w.economy().cities() {
        w.scenario_clear_production(city);
    }
}

fn gold(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().gold
}

fn mana(w: &World, p: PlayerId) -> HalfInt {
    w.player(p).expect("a player").treasury().mana
}

#[test]
fn the_budget_is_the_sum_of_what_the_cities_yield() {
    for (name, mut w) in games() {
        for round in 0..12 {
            for p in wizards(&w) {
                let (mut gold, mut upkeep, mut surplus) =
                    (HalfInt::ZERO, HalfInt::ZERO, HalfInt::ZERO);
                for (city, _) in w.economy().cities_of(p) {
                    let eco = w.economy().city_economy(city).expect("a city's economy");
                    gold += eco.gold;
                    upkeep += eco.gold_upkeep;
                    surplus +=
                        (eco.food - HalfInt::from_int(eco.figures as i32)).max(HalfInt::ZERO);
                }
                let b = w.budget(p);
                let at = format!("{name}, round {round}, player {p:?}");
                assert_eq!(b.city_gold, gold, "{at}: gold");
                assert_eq!(b.building_upkeep, upkeep, "{at}: building upkeep");
                assert_eq!(b.food_surplus, surplus, "{at}: food surplus");
                assert_eq!(
                    b.net_gold(),
                    b.city_gold - b.building_upkeep + b.noble_gold + b.food_gold - b.unit_upkeep,
                    "{at}: net gold"
                );
                assert!(
                    b.food_gold >= HalfInt::ZERO,
                    "{at}: food sold for less than nothing"
                );
                assert!(b.unit_upkeep >= HalfInt::ZERO, "{at}: upkeep below nothing");
            }
            w.play_round();
        }
    }
}

/// The turn of one player as the master runs it: the player's start, then the player's end.
fn play_turn_of(w: &mut World, p: PlayerId) -> Vec<Event> {
    let mut events = w.start_player_turn(p, w.turn);
    events.extend(w.end_player_turn(p));
    events
}

#[test]
fn the_treasury_after_a_turn_is_what_the_budget_said_once() {
    let mut checked = 0;
    for (name, mut w) in worlds() {
        idle_cities(&mut w);
        for round in 0..10 {
            w.start_round(w.turn + 1);
            for p in turn_players(&w) {
                let mut events = w.start_player_turn(p, w.turn);
                if w.is_neutral_player(p) || w.wizard(p).is_none() {
                    events.extend(w.end_player_turn(p));
                    continue;
                }
                let before = gold(&w, p);
                let net = w.budget(p).net_gold();
                events.extend(w.end_player_turn(p));
                let let_go = events
                    .iter()
                    .any(|e| matches!(e, Event::Shortage { .. } | Event::UnitsDisbanded { .. }));
                // A shortage lets units go and changes the sum; the plain case is the one to hold.
                if !let_go {
                    let wanted = (before + net).max(HalfInt::ZERO);
                    assert_eq!(
                        gold(&w, p),
                        wanted,
                        "{name}, round {round}, {p:?}: {before:?} and a net of {net:?}"
                    );
                    checked += 1;
                }
            }
            w.end_round();
        }
    }
    assert!(
        checked > 20,
        "only {checked} turns without a shortage were checked"
    );
}

#[test]
fn the_order_of_the_players_does_not_change_the_treasuries() {
    for (name, mut w) in games() {
        idle_cities(&mut w);
        for _ in 0..4 {
            w.play_round();
        }
        idle_cities(&mut w);
        let saved = w.snapshot();
        let mut forward = saved
            .clone()
            .restore(crate::support::content())
            .expect("restores");
        let mut backward = saved.restore(crate::support::content()).expect("restores");
        let mut order = turn_players(&forward);
        for p in order.clone() {
            play_turn_of(&mut forward, p);
        }
        order.reverse();
        for p in order {
            play_turn_of(&mut backward, p);
        }
        for p in wizards(&forward) {
            assert_eq!(
                gold(&forward, p),
                gold(&backward, p),
                "{name}: gold of {p:?}"
            );
            assert_eq!(
                mana(&forward, p),
                mana(&backward, p),
                "{name}: mana of {p:?}"
            );
        }
    }
}

#[test]
fn a_treasury_at_the_end_of_its_range_does_not_wrap() {
    for (name, mut w) in games().into_iter().step_by(2) {
        for p in wizards(&w) {
            let t = w.treasury_mut(p).expect("a treasury");
            t.gold = HalfInt::from_raw(i32::MAX - 3);
            t.mana = HalfInt::from_raw(i32::MAX - 3);
        }
        for _ in 0..3 {
            w.play_round();
        }
        for p in wizards(&w) {
            assert!(
                gold(&w, p) >= HalfInt::ZERO,
                "{name}: gold of {p:?} wrapped"
            );
            assert!(
                mana(&w, p) >= HalfInt::ZERO,
                "{name}: mana of {p:?} wrapped"
            );
        }
        let bad = world_problems(&w);
        assert!(bad.is_empty(), "{name}: {bad:#?}");
    }
}

#[test]
fn an_empty_treasury_stays_empty_and_never_goes_below() {
    for (name, mut w) in games().into_iter().step_by(2) {
        for _ in 0..6 {
            for p in wizards(&w) {
                let t = w.treasury_mut(p).expect("a treasury");
                t.gold = HalfInt::ZERO;
                t.mana = HalfInt::ZERO;
            }
            w.play_round();
            for p in wizards(&w) {
                assert!(gold(&w, p) >= HalfInt::ZERO, "{name}: gold of {p:?}");
                assert!(mana(&w, p) >= HalfInt::ZERO, "{name}: mana of {p:?}");
            }
        }
    }
}

#[test]
fn cities_at_the_size_cap_stay_there_and_hold_together() {
    for (name, mut w) in games().into_iter().step_by(2) {
        let cap = w.economy().max_city_size();
        for (city, _, _) in w.economy().cities() {
            w.scenario_set_citizens(city, cap);
        }
        for turn in 1..=8 {
            w.play_round();
            for (city, _, _) in w.economy().cities() {
                let size = w.city_population(city).figures;
                assert!(
                    size <= cap,
                    "{name}, turn {turn}: a city of {size} over the cap {cap}"
                );
            }
            let bad = world_problems(&w);
            assert!(bad.is_empty(), "{name}, after turn {turn}: {bad:#?}");
        }
    }
}

#[test]
fn a_game_played_far_past_its_usual_end_holds_together() {
    let (name, mut w) = games().remove(0);
    let start = w.turn;
    for turn in 1..=400 {
        w.play_round();
        if turn % 50 == 0 {
            let bad = world_problems(&w);
            assert!(bad.is_empty(), "{name}, after turn {turn}: {bad:#?}");
        }
    }
    assert!(
        w.turn >= start + 400 || w.game_over.is_some(),
        "the turns were not counted"
    );
    // What 400 turns left still saves and loads as itself.
    let again = w
        .snapshot()
        .restore(crate::support::content())
        .expect("restores");
    assert_eq!(text(&again), text(&w));
}
