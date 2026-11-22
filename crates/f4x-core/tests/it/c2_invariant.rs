//! Class 2: what must hold in every world, however it got there.

use crate::support::worlds;
use f4x_core::check::world_problems;
use f4x_core::game::Command;

#[test]
fn a_fresh_world_holds_together() {
    for (name, g) in worlds() {
        assert_eq!(world_problems(&g), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn the_world_holds_together_turn_after_turn() {
    for (name, mut g) in worlds() {
        for turn in 1..=30 {
            g.apply(Command::EndTurn);
            let bad = world_problems(&g);
            assert!(bad.is_empty(), "{name}, after turn {turn}: {bad:#?}");
        }
    }
}
