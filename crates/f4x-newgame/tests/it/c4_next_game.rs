//! A game changes its own copy of the content by its settings; a game started or loaded from that
//! copy must not take the changes along.

use f4x_core::world::World;
use f4x_def::testdir::{shipped, TestResult};
use f4x_newgame::setup::{new_game, NewGame};

/// Every setting that writes into the content, set away from its default.
fn changing() -> NewGame {
    let mut spec = NewGame {
        seed: 3,
        all_ai: true,
        ..NewGame::default()
    };
    let rules = &mut spec.settings.rules;
    rules.half_upkeep = !rules.half_upkeep;
    rules.arcane_candidate_cap += 1;
    rules.adamant_alchemy = true;
    rules.two_pick_myrran = true;
    rules.cavalry_halberdier_abilities = true;
    rules.trireme_capacity += 1;
    rules.galley_capacity += 1;
    spec.settings.world.heroes_enabled = false;
    spec
}

fn plain() -> NewGame {
    NewGame {
        seed: 3,
        all_ai: true,
        ..NewGame::default()
    }
}

#[test]
fn a_game_started_from_another_games_content_takes_none_of_its_settings() -> TestResult {
    let (changed, _) = new_game(shipped(), &changing())?;
    let (fresh, _) = new_game(shipped(), &plain())?;
    assert!(
        changed.content() != fresh.content(),
        "the settings change the content"
    );
    let (next, _) = new_game(changed.content(), &plain())?;
    assert!(next.content() == fresh.content());
    let (again, _) = new_game(changed.content(), &changing())?;
    assert!(again.content() == changed.content());
    Ok(())
}

#[test]
fn a_game_loaded_with_another_games_content_takes_none_of_its_settings() -> TestResult {
    let (changed, _) = new_game(shipped(), &changing())?;
    let (fresh, _) = new_game(shipped(), &plain())?;
    let saved = fresh.snapshot();
    let loaded: World = saved.restore(changed.content().clone())?;
    assert!(loaded.content() == fresh.content());
    Ok(())
}
