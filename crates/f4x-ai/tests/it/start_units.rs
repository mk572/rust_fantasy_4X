//! The AI with the extra starting units (settings `starting_units`): it founds a city with the
//! settlers and sends the Magic Spirit and the Trireme away from home.

use f4x_ai::{play_round, Ai};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::world::setting::StartingUnits;
use f4x_def::id::Serial;
use f4x_def::testdir::{shipped, TestResult};
use f4x_master::Game;
use f4x_newgame::setup::{new_game, NewGame};

/// Long enough for a settler to walk to a site, short enough that none can be built first.
const TURNS: u32 = 12;

#[test]
fn the_ai_uses_its_starting_units() -> TestResult {
    let rules = &shipped().rules;
    let (spirit, ship) = (
        rules.config.wizard.start_spirit,
        rules.config.wizard.start_ship,
    );
    let mut spec = NewGame {
        seed: 1,
        all_ai: true,
        ..NewGame::default()
    };
    spec.settings.starting_units = StartingUnits {
        settler: true,
        magic_spirit: true,
        trireme: true,
    };
    let mut game = Game::from(new_game(shipped(), &spec)?);
    let players = [PlayerId(0), PlayerId(1)];
    // The starting units by serial number, with where they started.
    let tracked = |game: &Game, p: PlayerId| -> Vec<(u16, Serial, Position)> {
        let units = game.units();
        units
            .player_units(p)
            .into_iter()
            .filter_map(|u| {
                let def = units.unit_def(u)?;
                (def.num == spirit || def.num == ship || def.is_settler()).then_some((
                    def.num,
                    units.serial_of(u),
                    units.unit_position(u)?,
                ))
            })
            .collect()
    };
    let before: Vec<_> = players.iter().map(|&p| tracked(&game, p)).collect();
    let mut ais: Vec<_> = players.iter().map(|&p| Ai::new(p)).collect();
    let mut moved = std::collections::BTreeSet::new();
    for _ in 0..TURNS {
        play_round(&mut game, &mut ais);
        for (&p, start) in players.iter().zip(&before) {
            for &(num, serial, from) in start {
                // A unit that is gone has founded a city or melded with a node.
                let gone_or_away = game
                    .units()
                    .entity_of_serial(serial)
                    .and_then(|u| game.units().unit_position(u))
                    .is_none_or(|at| at != from);
                if gone_or_away {
                    moved.insert((p, num, serial));
                }
            }
        }
    }
    for (&p, start) in players.iter().zip(&before) {
        assert!(start.len() >= 2, "{p:?}: {start:?}");
        for &(num, serial, _) in start {
            assert!(
                moved.contains(&(p, num, serial)),
                "{p:?}: unit {num} never left"
            );
        }
        let cities = game
            .economy()
            .cities()
            .into_iter()
            .filter(|(_, c, _)| c.owner == p)
            .count();
        assert!(cities >= 2, "{p:?}: {cities} cities");
    }
    Ok(())
}
