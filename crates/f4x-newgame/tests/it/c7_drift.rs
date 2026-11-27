//! Class 7 (`docs/f4x-newgame-test-design.md`): the world does not drift without anyone meaning it.
//!
//! The fresh games that `f4x-core`'s tests load (`fixtures`) are pinned to files; the three played
//! scenarios are in `f4x-master` (`tests/it/regression.rs`). A change of the generator or the setup
//! moves them; moving them is done on purpose with the environment variables named in the failure. The
//! tests below show that the comparison can fail.

mod fixtures;

use crate::support::*;

#[test]
fn a_game_of_another_seed_is_not_the_pinned_fresh_game() {
    // The fixture of seed 7 with three wizards, against the same game made on seed 8: the
    // comparison of `fixtures` is on the text, so the texts must differ.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../f4x-core/tests/saves/fresh_w3_s7.save");
    let pinned = std::fs::read_to_string(path).expect("the pinned game");
    let other = WorldCase {
        seed: 8,
        wizards: 3,
        ..standard()
    };
    let text = fresh_game(other).snapshot().to_text().expect("saves");
    assert_ne!(pinned, text);
}

#[test]
fn the_pinned_games_have_the_seeds_and_numbers_of_wizards_core_asks_for() {
    // A fixture file nobody loads is dead weight, and a missing one is a failure in core's tests.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-core/tests/saves");
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("the fixtures folder")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.starts_with("fresh_"))
        .collect();
    files.sort();
    let mut wanted: Vec<String> = fixtures::FIXTURES
        .iter()
        .flat_map(|&(w, s)| ["rng", "save"].map(|kind| format!("fresh_w{w}_s{s}.{kind}")))
        .collect();
    wanted.sort();
    assert_eq!(files, wanted);
}
