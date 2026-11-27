//! A new game takes its turns: whatever it was made from, rounds pass without a refusal or a panic.
//! The start itself is checked in `f4x-newgame` (class 4).

use f4x_core::world::Event;
use f4x_def::settings::{BOOKS_PER_REALM_RANGE, BOOKS_TOTAL_RANGE};
use f4x_def::testdir::{shipped, TestResult};
use f4x_master::Game;
use f4x_newgame::setup::{new_game, NewGame};
use f4x_newgame::testutil::{most_wizards, sizes, standard, WorldCase, VARIANTS};

/// The most picks the settings of the content's new game accept, for the computer wizards.
fn most_picks() -> u8 {
    let rules = &shipped().settings;
    let game = rules.default_game().expect("a default game");
    (game.wizards.picks.ai..=u8::MAX)
        .take_while(|&picks| {
            let mut with = game;
            with.wizards.picks.human = picks;
            with.wizards.picks.ai = picks;
            with.validate().is_empty()
        })
        .last()
        .expect("the picks of the default game are accepted")
}

#[test]
fn wizards_with_more_books_in_a_realm_than_the_default_allows_play_on() -> TestResult {
    use f4x_def::settings::DEFAULT_MAX_BOOKS_PER_REALM;
    let mut most_seen = 0;
    for seed in 1..40 {
        let mut spec = NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        };
        spec.settings.basic.opponents = 5;
        spec.settings.wizards.max_books_per_realm = *BOOKS_PER_REALM_RANGE.end();
        spec.settings.wizards.max_books_total = *BOOKS_TOTAL_RANGE.end();
        spec.settings.wizards.picks.ai = most_picks();
        let mut w = Game::from(new_game(shipped(), &spec)?);
        for p in w.player_ids() {
            if let Some(wizard) = w.wizard(p) {
                most_seen = most_seen.max(wizard.books().values().copied().max().unwrap_or(0));
            }
        }
        let refused: Vec<_> = (0..3)
            .flat_map(|_| w.play_round())
            .filter(|e| matches!(e, Event::Rejected(_)))
            .collect();
        assert!(refused.is_empty(), "seed {seed}: {refused:?}");
    }
    assert!(
        most_seen > DEFAULT_MAX_BOOKS_PER_REALM,
        "no wizard went over the default most of {DEFAULT_MAX_BOOKS_PER_REALM} books in a realm in 39 games: {most_seen}"
    );
    Ok(())
}

/// Ten turns of the computer's play on a game, with every refusal listed.
fn played_refusals(case: WorldCase, turns: usize) -> Vec<String> {
    let mut w = Game::from(new_game(shipped(), &case.spec()).expect("a new game"));
    let mut refused = Vec::new();
    for _ in 0..turns {
        for e in w.play_round() {
            if matches!(e, Event::Rejected(_)) {
                refused.push(format!("{e:?}"));
            }
        }
    }
    refused
}

#[test]
fn turns_pass_on_a_cut_of_the_worlds_without_a_refusal_or_a_panic() {
    let std = standard();
    let mut worlds = vec![std];
    for &variant in VARIANTS {
        worlds.push(WorldCase {
            variant,
            seed: 3,
            ..std
        });
    }
    for (width, height) in sizes() {
        worlds.push(WorldCase {
            width,
            height,
            seed: 6,
            ..std
        });
    }
    for w in [2, most_wizards()] {
        worlds.push(WorldCase {
            wizards: w,
            seed: 8,
            ..std
        });
    }
    for case in worlds {
        let refused = played_refusals(case, 10);
        assert!(refused.is_empty(), "{case:?}: {refused:?}");
    }
}
