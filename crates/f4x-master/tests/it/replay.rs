//! A recorded game plays again to the same game.

use f4x_core::world::save::GAME_VERSION;
use f4x_core::world::Command;
use f4x_master::{Entry, Replay, ReplayError};

use crate::support::{all_farm, busy_round, content, games, text};

#[test]
fn a_recorded_game_plays_again_to_the_same_game() {
    for (name, mut game) in games() {
        game.start_recording();
        for _ in 0..10 {
            busy_round(&mut game, 6);
        }
        let replay = game.replay_so_far().expect("a recording");
        // Written and read again, as a file would carry it.
        let replay = Replay::from_text(&replay.to_text().expect("writes")).expect("reads");
        let (played, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(played.world()), text(game.world()), "{name}");
    }
}

#[test]
fn balancing_the_farmers_is_replayed_and_can_be_taken_back() {
    for (name, mut game) in games() {
        all_farm(&mut game);
        let player = game.player_ids()[0];
        game.start_recording();
        game.set_undo(true);
        let before = text(game.world());
        game.apply(Command::BalanceFarmers { player });
        let balanced = text(game.world());
        assert_ne!(balanced, before, "{name}: nothing was balanced");
        let replay = game.replay_so_far().expect("a record");
        let (again, _) = replay.play(content(), usize::MAX).expect("plays");
        assert_eq!(text(again.world()), balanced, "{name}");
        game.undo().expect("takes it back");
        assert_eq!(text(game.world()), before, "{name}");
    }
}

#[test]
fn a_replay_stopped_part_way_is_the_game_as_it_was_then() {
    for (name, mut game) in games() {
        game.start_recording();
        let mut states = vec![(0, text(game.world()))];
        for _ in 0..6 {
            busy_round(&mut game, 4);
            let done = game.replay_so_far().expect("a recording").entries.len();
            states.push((done, text(game.world())));
        }
        let replay = game.stop_recording().expect("a recording");
        for (count, state) in states {
            let (part, _) = replay.play(content(), count).expect("plays");
            assert_eq!(text(part.world()), state, "{name}, after {count} commands");
        }
        assert!(!game.is_recording());
    }
}

#[test]
fn a_replay_that_is_no_replay_or_names_nothing_is_refused() {
    assert!(matches!(
        Replay::from_text("nonsense"),
        Err(ReplayError::Load(_))
    ));
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 3);
    let mut replay = game.stop_recording().expect("recorded");
    let unknown = "MoveStack(units: [999999], to: (x: 0, y: 0, plane: Arcanus))";
    replay.entries.insert(0, Entry::Command(unknown.into()));
    assert!(matches!(
        replay.play(content(), usize::MAX),
        Err(ReplayError::Unresolved(0))
    ));
    let (_, untouched) = games().remove(0);
    assert!(matches!(
        untouched.replay_so_far(),
        Err(ReplayError::NotRecording)
    ));
}

#[test]
fn the_units_a_recorded_command_names_are_told_apart() {
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 6);
    let replay = game.stop_recording().expect("a recording");
    let moves: std::collections::BTreeSet<&str> = replay
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Command(c) if c.starts_with("MoveStack") => Some(c.as_str()),
            _ => None,
        })
        .collect();
    assert!(moves.len() >= 3, "{moves:?}");
    let units: std::collections::BTreeSet<&str> = moves
        .iter()
        .filter_map(|c| c.split("units:[").nth(1)?.split(']').next())
        .collect();
    assert!(units.len() >= 3, "the moves name too few units: {units:?}");
}

#[test]
fn a_replay_of_a_game_version_this_game_cannot_read_is_refused() {
    let (_, mut game) = games().remove(0);
    game.start_recording();
    busy_round(&mut game, 2);
    let text = game
        .stop_recording()
        .expect("recorded")
        .to_text()
        .expect("writes");
    assert!(Replay::from_text(&text).is_ok());
    let from = format!("game_version: \"{GAME_VERSION}\"");
    assert!(text.contains(&from), "the replay does not say its version");
    for version in ["9999.0.0", "0.0.0", "banana"] {
        let edited = text.replacen(&from, &format!("game_version: \"{version}\""), 1);
        assert!(
            matches!(Replay::from_text(&edited), Err(ReplayError::Load(_))),
            "{version}"
        );
    }
}
