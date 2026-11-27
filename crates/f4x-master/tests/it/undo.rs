//! Undo gives back the game as it was, and only what may be taken back.

use f4x_def::names::NameBundle;
use f4x_master::UndoError;

use crate::support::{a_change_that_can_be_taken_back, busy_round, games, text};

#[test]
fn undo_gives_back_the_game_as_it_was() {
    for (name, mut game) in games() {
        game.set_undo(true);
        let before = a_change_that_can_be_taken_back(&mut game).expect("a change");
        assert!(game.can_undo(), "{name}: nothing to take back");
        game.undo().expect("takes it back");
        assert_eq!(text(game.world()), before, "{name}");
    }
}

#[test]
fn the_name_lists_stay_after_an_undo() {
    let (_, mut game) = games().remove(0);
    // A list of one name nothing ships, to know it again.
    let mut names = NameBundle::default();
    names.cities.insert(0, vec![String::from("Kept")]);
    game.world_mut().set_names(names.clone());
    game.set_undo(true);
    a_change_that_can_be_taken_back(&mut game).expect("a change");
    game.undo().expect("takes it back");
    assert_eq!(game.names(), &names);
}

#[test]
fn the_saved_map_locations_stay_after_an_undo() {
    let (_, mut game) = games().remove(0);
    let player = game.player_ids()[0];
    let units = game.units();
    let mine = units.player_units(player)[0];
    let at = units.unit_position(mine).expect("on the map");
    assert!(game.world_mut().save_location(player, 0, at));
    game.set_undo(true);
    a_change_that_can_be_taken_back(&mut game).expect("a change");
    assert!(game.world_mut().save_location(player, 1, at));
    let saved = game.locations(player);
    game.undo().expect("takes it back");
    assert_eq!(game.locations(player), saved);
}

#[test]
fn what_cannot_be_taken_back_clears_the_history() {
    for (name, mut game) in games() {
        game.set_undo(true);
        a_change_that_can_be_taken_back(&mut game).expect("a change");
        assert!(game.can_undo(), "{name}");
        busy_round(&mut game, 3); // ends the turn
        assert!(
            !game.can_undo(),
            "{name}: the turn ended and the history stayed"
        );
        assert_eq!(game.undo(), Err(UndoError::NothingToUndo), "{name}");
    }
}

#[test]
fn a_game_with_undo_off_keeps_no_history() {
    let (_, mut game) = games().remove(0);
    assert!(!game.undo_enabled());
    a_change_that_can_be_taken_back(&mut game).expect("a change");
    assert!(!game.can_undo());
    assert_eq!(game.undo(), Err(UndoError::NothingToUndo));
}

#[test]
fn a_recording_runs_from_its_start_to_its_stop_and_takes_back_what_undo_takes_back() {
    let (_, mut game) = games().remove(0);
    assert!(!game.is_recording());
    game.start_recording();
    assert!(game.is_recording());
    game.set_undo(true);
    assert!(game.undo_enabled());
    assert!(!game.can_undo());
    let recorded = |game: &f4x_master::Game| {
        let replay = game.replay_so_far().expect("recording");
        replay.entries.len()
    };
    let before = recorded(&game);
    a_change_that_can_be_taken_back(&mut game).expect("a change");
    assert_eq!(game.undo_label(), Some("SetTaxRate"));
    assert!(recorded(&game) > before, "the change was not recorded");
    assert_eq!(game.undo().as_deref(), Ok("SetTaxRate"));
    assert_eq!(
        recorded(&game),
        before,
        "what was taken back is still in the recording"
    );
    game.set_undo(false);
    assert!(!game.undo_enabled());
    assert!(!game.can_undo());
    let replay = game.stop_recording().expect("a recording");
    assert_eq!(replay.entries.len(), before);
    assert!(!game.is_recording());
    assert!(game.stop_recording().is_none());
}
