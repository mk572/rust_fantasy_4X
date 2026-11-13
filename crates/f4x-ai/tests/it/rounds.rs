//! The AIs under the master: sequential turns, one round counter step per round.

use f4x_ai::{play_round, Ai};
use f4x_core::game::Event;
use f4x_core::player::PlayerId;
use f4x_newgame::setup::{new_game, NewGame};

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

#[test]
fn a_game_loaded_with_the_ais_plans_plays_on_as_the_saved_one_would_have() {
    use f4x_core::game::save::Snapshot;
    let mut g = new_game(
        f4x_def::testdir::shipped(),
        &NewGame {
            seed: 15,
            all_ai: true,
            ..NewGame::default()
        },
    )
    .expect("a new game");
    let mut ais = vec![Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    for _ in 0..40 {
        play_round(&mut g, &mut ais);
    }
    Ai::remember(&mut g, &ais);
    let text = g.snapshot().to_text().expect("saves");
    let mut loaded = Snapshot::from_text(&text)
        .expect("reads")
        .restore(g.content().clone())
        .expect("loads");
    let mut fresh = vec![Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    Ai::recall(&loaded, &mut fresh);
    for (a, b) in fresh.iter().zip(&ais) {
        assert_eq!(a.to_text().ok(), b.to_text().ok(), "the plans come back");
    }
    for _ in 0..40 {
        play_round(&mut g, &mut ais);
        play_round(&mut loaded, &mut fresh);
    }
    assert_eq!(
        g.snapshot().to_text().ok(),
        loaded.snapshot().to_text().ok(),
        "the loaded game with its plans matches the straight run"
    );
}

#[test]
fn the_master_hands_the_ais_plans_to_the_game_at_every_round_start() {
    use f4x_ai::AiSeat;
    use f4x_master::{Master, Seat, Stopped, Until};
    let mut g = new_game(
        f4x_def::testdir::shipped(),
        &NewGame {
            seed: 3,
            all_ai: true,
            ..NewGame::default()
        },
    )
    .expect("a new game");
    let mut ais = vec![Ai::new(PlayerId(0)), Ai::new(PlayerId(1))];
    let [a, b] = &mut ais[..] else { unreachable!() };
    let seats = vec![
        Seat::new(vec![PlayerId(0)], Box::new(AiSeat(a))),
        Seat::new(vec![PlayerId(1)], Box::new(AiSeat(b))),
        Seat::neutrals(Box::new(f4x_ai::NeutralSeat::default())),
    ];
    let mut master = Master::new(seats).checkpoints();
    let progress = master.run(&mut g, Until::Rounds(5));
    assert_eq!(progress.stopped, Stopped::RoundStart);
    drop(master);
    let saved: Vec<_> = g.plans().iter().map(|(p, _)| *p).collect();
    assert_eq!(&saved[..2], &[PlayerId(0), PlayerId(1)]);
    for ((_, text), ai) in g.plans().iter().zip(&ais) {
        assert_eq!(Some(text.clone()), ai.to_text().ok(), "the AI as it stands");
    }
    // The neutral players keep plans too, like any AI.
    let neutral = &saved[2..];
    assert!(!neutral.is_empty());
    assert!(neutral.iter().all(|&p| g.race_of_player(p).is_some()));
}
