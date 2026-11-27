//! The sanity limit of a square, over commands and turns (`c6_tile.rs` in core has the rules of one
//! command).

use f4x_core::map::Position;
use f4x_core::unit::TILE_SANITY_LIMIT;
use f4x_core::world::testutil::sweep::commands;
use f4x_core::world::Command;

use crate::support::{busy_round, games};

fn wizard_and_city(w: &f4x_core::world::World) -> (f4x_core::player::PlayerId, Position) {
    let wizard = w
        .player_ids()
        .into_iter()
        .find(|&p| w.wizard(p).is_some())
        .expect("a wizard");
    let at = w
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner == wizard)
        .expect("a city")
        .2;
    (wizard, at)
}

/// The sanity limit looks only at squares where a unit was just put, so every way of putting one there
/// must be watched. Whatever the commands of the sweep do, no square is left over the limit (the game
/// itself asserts the same after each command in a debug build).
#[test]
fn no_command_leaves_a_square_over_the_sanity_limit() {
    for (name, mut w) in games().into_iter().step_by(2) {
        // Crowd a square close to the limit, so that what the commands put there tips it over.
        let (wizard, at) = wizard_and_city(&w);
        let unit = f4x_core::rules::UnitId::new(w.content().rules.units.defs()[0].num);
        while w.units().units_at(at).len() < TILE_SANITY_LIMIT {
            w.command(Command::CreateUnit {
                owner: wizard,
                pos: at,
                unit,
            });
        }
        for command in commands(&w) {
            let label = format!("{command:?}");
            w.command(command);
            assert!(
                w.crowded_anywhere().is_empty(),
                "{name}: after {label}: {:?}",
                w.crowded_anywhere()
            );
        }
        for _ in 0..5 {
            busy_round(&mut w, 200);
            assert!(w.crowded_anywhere().is_empty(), "{name}");
        }
    }
}
