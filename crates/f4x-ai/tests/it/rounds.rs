//! The AIs under the master: sequential turns, one round counter step per round.

use f4x_ai::{play_round, Ai};
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::Event;
use f4x_core::player::PlayerId;

#[test]
fn rounds_with_two_ais_advance_the_round_counter_once_each() {
    let mut g = new_game(
        f4x_def::testdir::shipped(),
        &NewGame {
            seed: 5,
            all_ai: true,
            ..NewGame::default()
        },
    )
    .expect("a new game");
    let mut ais = vec![Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    let start = g.turn;
    let mut started = 0;
    for _ in 0..8 {
        let events = play_round(&mut g, &mut ais);
        started += events
            .iter()
            .filter(|e| matches!(e, Event::TurnStarted { .. }))
            .count();
    }
    assert_eq!(g.turn, start + 8);
    assert_eq!(
        started, 8,
        "the round start runs once per round, not once per wizard"
    );
}

#[test]
fn the_second_ai_plays_on_the_board_the_first_left() {
    // Same seed, same AIs: playing in sequence is deterministic.
    let play = || {
        let mut g = new_game(
            f4x_def::testdir::shipped(),
            &NewGame {
                seed: 9,
                all_ai: true,
                ..NewGame::default()
            },
        )
        .expect("a new game");
        let mut ais = vec![Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
        for _ in 0..15 {
            play_round(&mut g, &mut ais);
        }
        (g.turn, g.units().all_units().len(), g.next_u64())
    };
    assert_eq!(play(), play());
}
