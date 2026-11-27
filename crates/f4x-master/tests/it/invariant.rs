//! Class 2: what must hold in every world holds turn after turn. The fresh worlds are core's
//! (`f4x-core/tests/it/c2_invariant.rs`).

use crate::support::games;
use f4x_core::check::world_problems;

#[test]
fn the_world_holds_together_turn_after_turn() {
    for (name, mut w) in games() {
        for turn in 1..=30 {
            w.play_round();
            let bad = world_problems(&w);
            assert!(bad.is_empty(), "{name}, after turn {turn}: {bad:#?}");
        }
    }
}
