//! What the tests share: the fresh games the matrix runs over.

use f4x_core::game::testutil::fresh;
use f4x_core::game::Game;

/// The saved fresh games: (computer wizards, seed). The files are `tests/saves/fresh_w*_s*.save`.
pub const WORLDS: [(u8, u64); 4] = [(2, 7), (3, 3), (3, 7), (3, 11)];

/// Every fresh game of the matrix, with its name for a failure message.
pub fn worlds() -> Vec<(String, Game)> {
    WORLDS
        .iter()
        .map(|&(w, s)| {
            let game = fresh(w, s).unwrap_or_else(|e| panic!("{e}"));
            (format!("{w} wizards, seed {s}"), game)
        })
        .collect()
}
