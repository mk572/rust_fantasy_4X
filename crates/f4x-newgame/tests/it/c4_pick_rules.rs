//! Class 4 (`docs/f4x-newgame-test-design.md`): the rules of the start that the settings name (the
//! counts of towers and cities, the picks, the book limits), through the public interface only.

use crate::support::{knobs, limits};
use f4x_core::wizard::BuildError;
use f4x_core::world::World;
use f4x_def::content::Content;
use f4x_def::settings::{BOOKS_PER_REALM_RANGE, BOOKS_TOTAL_RANGE};
use f4x_def::testdir::TestResult;
use f4x_newgame::error::NewGameError;
use f4x_newgame::setup::{new_game, NewGame};

fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}

/// A new game on the shipped content with the map-size counts set by hand.
fn game_with_counts(
    seed: u64,
    towers: Option<u8>,
    neutral_cities: Option<u8>,
) -> Result<World, NewGameError> {
    let mut spec = NewGame {
        seed,
        all_ai: true,
        ..Default::default()
    };
    spec.settings.basic.towers = towers;
    spec.settings.basic.neutral_cities = neutral_cities;
    new_game(shipped(), &spec).map(|(world, _)| world)
}

fn towers_in(w: &World) -> usize {
    let economy = w.economy();
    economy
        .lairs()
        .iter()
        .filter(|l| {
            economy
                .feature_at(l.pos)
                .is_some_and(|f| f.kind == f4x_def::feature::FeatureKind::Tower)
        })
        .count()
}

fn neutral_cities_on(w: &World, plane: f4x_core::map::Plane) -> usize {
    let economy = w.economy();
    economy
        .cities()
        .iter()
        .filter(|(c, _, pos)| {
            pos.plane == plane
                && economy
                    .city_owner(*c)
                    .is_some_and(|o| w.is_neutral_player(o))
        })
        .count()
}

#[test]
fn the_settings_say_how_many_towers_a_map_gets() -> TestResult {
    for towers in [0u8, 2, 5] {
        let w = game_with_counts(4, Some(towers), None)?;
        assert!(
            towers_in(&w) <= usize::from(towers),
            "{towers} towers asked, {} made",
            towers_in(&w)
        );
        assert!(
            towers_in(&w) + 2 >= usize::from(towers),
            "{towers} towers asked, {} made",
            towers_in(&w)
        );
    }
    // The default follows the area: six on the normal map.
    assert!(towers_in(&game_with_counts(4, None, None)?) >= 4);
    Ok(())
}

#[test]
fn the_settings_cap_the_neutral_cities_of_each_plane() -> TestResult {
    let w = game_with_counts(4, None, Some(3))?;
    for plane in f4x_core::map::Plane::all() {
        let n = neutral_cities_on(&w, plane);
        assert!(n <= 3, "{plane:?}: {n} neutral cities with a cap of 3");
    }
    let open = game_with_counts(4, None, None)?;
    let total = |w: &World| -> usize {
        f4x_core::map::Plane::all()
            .map(|p| neutral_cities_on(w, p))
            .sum()
    };
    assert!(
        total(&open) > total(&w),
        "without a cap there are more: {} against {}",
        total(&open),
        total(&w)
    );
    Ok(())
}

#[test]
fn a_myrran_wizard_that_fits_only_at_the_option_s_cost_needs_the_option_to_be_made() -> TestResult {
    use f4x_core::rules::RealmId as Realm;
    use f4x_core::wizard::magic::Retort;
    let picks = NewGame::default().settings.wizards.picks.human;
    // The cost the option gives Myrran, read from a game made with it.
    let optional = {
        let mut spec = NewGame {
            seed: 5,
            all_ai: true,
            ..Default::default()
        };
        spec.settings.rules.two_pick_myrran = true;
        f4x_core::wizard::retort_cost(
            &new_game(shipped(), &spec)?.0.content().rules,
            Retort::new(9 /* myrran */),
        )
    };
    let normal = f4x_core::wizard::retort_cost(&shipped().rules, Retort::new(9 /* myrran */));
    assert!(optional < normal, "the option makes Myrran cheaper");
    // Books that use up the picks together with the cheaper Myrran, and overspend with the dearer.
    let books = picks - optional;
    let make = |two_pick: bool| {
        let mut spec = NewGame {
            seed: 5,
            books: [(Realm::new(3 /* life */), books)].into(),
            retorts: [Retort::new(9 /* myrran */)].into(),
            ..Default::default()
        };
        spec.settings.rules.two_pick_myrran = two_pick;
        new_game(shipped(), &spec)
    };
    assert!(make(false).is_err(), "the books and Myrran overspend");
    make(true)?;
    Ok(())
}

/// The most picks a game's settings let a wizard have (what the validator takes).
fn most_picks() -> u8 {
    let picks = knobs()
        .into_iter()
        .find(|k| k.name == "wizards.picks.human")
        .expect("the picks are a knob");
    u8::try_from(limits(&picks).1).expect("picks fit a byte")
}

/// The fewest picks a ready-made wizard spends (the computer's wizards are no poorer than these).
fn fewest_ready_made_picks() -> u8 {
    let rules = &shipped().rules;
    (rules.wizards.defs().iter())
        .filter_map(|d| f4x_core::wizard::preset_build_of(rules.wizards.defs(), d.num, true))
        .map(|b| b.picks_spent(rules))
        .min()
        .expect("the content has ready-made wizards")
}

#[test]
fn no_wizard_starts_with_more_books_in_a_realm_than_the_setting_allows() -> TestResult {
    let (lo, hi) = (*BOOKS_PER_REALM_RANGE.start(), *BOOKS_PER_REALM_RANGE.end());
    let picks = most_picks();
    for most in [lo, (lo + hi) / 2, hi] {
        for randomise in [true, false] {
            for seed in 1..8 {
                let mut spec = NewGame {
                    seed,
                    all_ai: true,
                    ..Default::default()
                };
                spec.settings.basic.opponents = 5;
                spec.settings.wizards.max_books_per_realm = most;
                spec.settings.wizards.max_books_total = *BOOKS_TOTAL_RANGE.end();
                spec.settings.wizards.picks.ai = picks;
                spec.settings.ai.wizards.randomise = randomise;
                let w = new_game(shipped(), &spec)?.0;
                let wizards: Vec<_> = w
                    .player_ids()
                    .into_iter()
                    .filter_map(|p| w.wizard(p))
                    .collect();
                assert_eq!(
                    wizards.len(),
                    usize::from(spec.settings.basic.opponents) + 1,
                    "the opponents and the one who would be you"
                );
                for wizard in wizards {
                    let books = wizard.books();
                    assert!(
                        books.values().all(|&n| n <= most),
                        "seed {seed}, at most {most}, randomise {randomise}: {books:?}"
                    );
                    let spent = books.values().sum::<u8>()
                        + wizard
                            .retorts()
                            .iter()
                            .map(|&r| f4x_core::wizard::retort_cost(&w.content().rules, r))
                            .sum::<u8>();
                    // Whatever the cap took off is spent again: no wizard spends fewer picks
                    // than the poorest ready-made one, nor more than there are.
                    assert!(
                        (fewest_ready_made_picks()..=picks).contains(&spent),
                        "seed {seed}, at most {most}: {books:?} spent {spent}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_ready_made_human_wizard_over_the_most_books_is_asked_to_spend_the_picks() -> TestResult {
    // A ready-made wizard with a realm of many books (a number of the content).
    let rules = &shipped().rules;
    let (preset, books) = rules
        .wizards
        .defs()
        .iter()
        .filter_map(|d| {
            let most = d.books.iter().map(|&(_, n)| n).max()?;
            Some((d.num, most))
        })
        .max_by_key(|&(_, most)| most)
        .ok_or("the content has no ready-made wizard")?;
    let make = |most: u8| {
        let mut spec = NewGame {
            seed: 3,
            preset: Some(preset),
            ..Default::default()
        };
        spec.settings.wizards.max_books_per_realm = most;
        new_game(shipped(), &spec)
    };
    assert!(make(books).is_ok());
    let refused = make(books - 1)
        .err()
        .ok_or("the wizard is made with surplus picks")?;
    assert!(
        matches!(
            refused,
            NewGameError::Wizard(BuildError::UnspentPicks { .. })
        ),
        "{refused:?}"
    );
    Ok(())
}
