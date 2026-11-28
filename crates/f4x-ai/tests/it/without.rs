//! The AIs in games without heroes and items and without lairs (settings `heroes_enabled` and
//! `lairs_enabled`): whole turns are played and neither turns up.

use f4x_ai::{play_round, Ai};
use f4x_core::world::Event;
use f4x_def::testdir::{sample, shipped, TestResult};
use f4x_master::Game;
use f4x_newgame::setup::{new_game, NewGame};

#[test]
fn without_heroes_and_lairs_the_ais_play_on_and_neither_turns_up() -> TestResult {
    let mut spec = NewGame {
        seed: 3,
        all_ai: true,
        ..NewGame::with_wizards(4)
    };
    spec.settings.world.heroes_enabled = false;
    spec.settings.world.lairs_enabled = false;
    // Fame that brings offers early.
    spec.settings.wizards.starting_gold.ai = 2000;
    let mut game = Game::from(new_game(shipped(), &spec)?);
    let wizards: Vec<_> = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.is_wizard(p))
        .collect();
    let mut ais: Vec<_> = wizards.iter().map(|&p| Ai::new(p)).collect();
    for _ in 0..sample(200, 60) {
        let events = play_round(&mut game, &mut ais);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::MonstersSettled { .. })),
            "round {}",
            game.round()
        );
        for &p in &wizards {
            assert!(
                game.units().hero_units(p).is_empty(),
                "round {}",
                game.round()
            );
        }
        assert!(
            game.items().held(|_| true).is_empty(),
            "round {}",
            game.round()
        );
    }
    Ok(())
}
