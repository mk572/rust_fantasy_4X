//! The treasuries through the turn order: each player's turn changes its own by what its budget
//! said, and the order in which the players take their turns does not matter to them.

use f4x_core::player::PlayerId;
use f4x_core::world::save::ContentCheck;
use f4x_core::world::Event;
use f4x_def::half_int::HalfInt;
use f4x_master::{turn_players, Game, SaveFile};

use crate::support::{content, games, gold, idle_cities, mana, wizards};

/// The turn of one player as the master runs it: the player's start, then the player's end.
fn play_turn_of(w: &mut Game, p: PlayerId) -> Vec<Event> {
    let mut events = w.start_player_turn(p);
    events.extend(w.end_player_turn(p));
    events
}

#[test]
fn the_treasury_after_a_turn_is_what_the_budget_said_once() {
    let mut checked = 0;
    for (name, mut w) in games() {
        idle_cities(&mut w);
        for round in 0..10 {
            w.start_round();
            for p in turn_players(&w) {
                let mut events = w.start_player_turn(p);
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
        let saved = w.save_file(Vec::new());
        let again = |saved: SaveFile| {
            saved
                .restore(content(), ContentCheck::Strict)
                .expect("restores")
                .game
        };
        let mut forward = again(saved.clone());
        let mut backward = again(saved);
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
