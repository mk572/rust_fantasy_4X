//! The saved fresh games that the unit tests of `f4x-core` load instead of making a world (core
//! sits below this crate and cannot): `crates/f4x-core/tests/saves/fresh_w<wizards>_s<seed>.save`.
//! The test checks that each file is the game `new_game` makes today, so a change of the map
//! generator or the setup shows up here and the files are refreshed on purpose:
//! `F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures`, then read the tests of
//! `f4x-core` that load them.

use f4x_def::testdir::{shipped, TestResult};
use f4x_newgame::setup::{new_game, NewGame};

/// The file of the fresh game with `wizards` wizards on `seed` (`f4x_core::game::testutil::fresh`).
fn fresh_file(wizards: u8, seed: u64) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../f4x-core/tests/saves")
        .join(format!("fresh_w{wizards}_s{seed}.save"))
}

/// The games core's tests load: `(wizards, seed)`, every one played by the computer.
const FIXTURES: &[(u8, u64)] = &[(2, 7), (3, 3), (3, 7), (3, 11)];

#[test]
fn the_fresh_games_core_loads_are_what_new_game_makes() -> TestResult {
    let refresh = std::env::var_os("F4X_REFRESH_FIXTURES").is_some();
    let mut stale = Vec::new();
    for &(wizards, seed) in FIXTURES {
        let game = new_game(
            shipped(),
            &NewGame {
                seed,
                all_ai: true,
                ..NewGame::with_wizards(wizards)
            },
        )?;
        let text = game.snapshot().to_text()?;
        let path = fresh_file(wizards, seed);
        if refresh {
            std::fs::create_dir_all(path.parent().ok_or("no folder")?)?;
            std::fs::write(&path, &text)?;
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            stale.push(path.display().to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "stale or missing: {stale:?}; refresh with F4X_REFRESH_FIXTURES=1 cargo test -p f4x-newgame --test it fixtures"
    );
    Ok(())
}
