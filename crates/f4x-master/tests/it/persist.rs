//! Class 4 over played rounds: a game rebuilt from text loses nothing. What a world alone writes and
//! reads is in core (`c4_persist.rs`).

use f4x_core::world::save::{ContentCheck, Snapshot};
use f4x_core::world::World;
use f4x_master::SaveFile;

use crate::support::{busy_round, content, games, shape, state, text};

fn restored(w: &World) -> World {
    Snapshot::from_text(&text(w))
        .expect("the text reads")
        .restore(content())
        .expect("the game restores")
}

#[test]
fn a_game_written_and_read_is_the_same_game() {
    for (name, mut w) in games() {
        for round in 0..12 {
            let back = restored(&w);
            assert_eq!(text(&back), text(&w), "{name}, round {round}");
            busy_round(&mut w, 6);
        }
    }
}

#[test]
fn a_loaded_game_goes_on_as_the_saved_one_would_have() {
    for (name, mut live) in games() {
        for _ in 0..8 {
            busy_round(&mut live, 6);
        }
        let saved = live.save_file(Vec::new()).to_text().expect("saves");
        let mut loaded = SaveFile::from_text(&saved)
            .expect("reads")
            .restore(content(), ContentCheck::Strict)
            .expect("restores")
            .game;
        for round in 0..15 {
            let a = shape(&busy_round(&mut live, 6));
            let b = shape(&busy_round(&mut loaded, 6));
            assert_eq!(a, b, "{name}, round {round}: the events differ");
            assert_eq!(state(&live), state(&loaded), "{name}, round {round}");
        }
    }
}
