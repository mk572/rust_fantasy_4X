//! The steps of the turn order, on real games.

use f4x_core::player::MONSTERS;
use f4x_core::world::{Event, Rejection};
use f4x_master::{Entry, Step};

use crate::support::{content, games, text};

#[test]
fn a_whole_round_is_the_steps_for_every_player() {
    for ((name, mut a), (_, mut b)) in games().into_iter().zip(games()) {
        a.play_round();
        let players = b.turn_players().to_vec();
        let round = b.round();
        for &p in &players {
            if !b.is_dormant(p) {
                let (world, rng) = b.world_and_rng();
                world.advance(|w| w.end_player_turn(p, rng));
            }
        }
        let (world, rng) = b.world_and_rng();
        world.advance(|w| w.end_round(rng));
        world.advance(|w| w.start_round(round + 1, rng));
        for &p in &players {
            if !b.is_dormant(p) {
                let (world, rng) = b.world_and_rng();
                world.advance(|w| w.start_player_turn(p, round + 1, rng));
            }
        }
        assert_eq!(text(a.world()), text(b.world()), "{name}");
        assert_eq!(a.draws(), b.draws(), "{name}");
    }
}

#[test]
fn every_player_takes_part_and_the_monsters_come_last() {
    for (name, game) in games() {
        let mut everyone = game.player_ids();
        assert!(!everyone.contains(&MONSTERS), "{name}");
        everyone.push(MONSTERS);
        assert_eq!(game.turn_players(), everyone, "{name}");
    }
}

#[test]
fn a_step_is_recorded_ends_the_undo_history_and_plays_again() {
    for (name, mut game) in games() {
        let player = game.turn_players()[0];
        game.start_recording();
        game.set_undo(true);
        crate::support::a_change_that_can_be_taken_back(&mut game).expect("a change");
        assert!(game.can_undo(), "{name}");
        game.end_player_turn(player);
        assert!(!game.can_undo(), "{name}: a step can be taken back");
        game.end_round();
        game.start_round();
        game.start_player_turn(player);
        game.play_round();
        let replay = game.replay_so_far().expect("a recording");
        let steps: Vec<Step> = replay
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Step(step) => Some(*step),
                Entry::Command(_) => None,
            })
            .collect();
        assert_eq!(
            steps,
            [
                Step::EndTurn(player),
                Step::EndRound,
                Step::StartRound,
                Step::StartTurn(player),
                Step::PlayRound,
            ],
            "{name}"
        );
        let (again, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(again.world()), text(game.world()), "{name}");
    }
}

#[test]
fn a_step_for_no_player_is_refused_and_leaves_no_trace() {
    let (_, mut game) = games().remove(0);
    game.start_recording();
    game.set_undo(true);
    crate::support::a_change_that_can_be_taken_back(&mut game).expect("a change");
    let before = text(game.world());
    let recorded = game.replay_so_far().expect("a recording").entries.len();
    let nobody = (0..MONSTERS.0)
        .map(f4x_core::player::PlayerId)
        .find(|p| !game.turn_players().contains(p))
        .expect("a free player number");
    for events in [game.start_player_turn(nobody), game.end_player_turn(nobody)] {
        assert_eq!(events, [Event::Rejected(Rejection::NoSuchPlayer)]);
    }
    assert_eq!(text(game.world()), before);
    assert_eq!(
        game.replay_so_far().expect("a recording").entries.len(),
        recorded
    );
    assert!(game.can_undo());
}
