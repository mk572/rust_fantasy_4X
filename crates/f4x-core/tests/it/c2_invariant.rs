//! Class 2: what must hold in every world, however it got there. The same turn after turn is the
//! game's (`f4x-master/tests/it/invariant.rs`).

use crate::support::worlds;
use f4x_core::check::world_problems;

#[test]
fn a_fresh_world_holds_together() {
    for (name, w) in worlds() {
        assert_eq!(world_problems(&w), Vec::<String>::new(), "{name}");
    }
}
