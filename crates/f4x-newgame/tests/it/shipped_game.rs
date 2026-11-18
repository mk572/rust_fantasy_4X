//! A whole game on the shipped content (`content/`), through the public interface only.

use f4x_core::game::{Command, Event, Game};
use f4x_def::content::Content;
use f4x_def::testdir::TestResult;
use f4x_newgame::setup::{new_game, NewGame};

fn shipped() -> &'static Content {
    f4x_def::testdir::shipped()
}

fn game(seed: u64) -> Result<Game, String> {
    new_game(
        shipped(),
        &NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        },
    )
}

fn end_turns(g: &mut Game, n: usize) -> Vec<Event> {
    (0..n).flat_map(|_| g.apply(Command::EndTurn)).collect()
}

#[test]
fn turns_pass_on_a_new_game_without_a_refusal() -> TestResult {
    let mut g = game(3)?;
    let events = end_turns(&mut g, 20);
    let refused: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::Rejected(_)))
        .collect();
    assert!(refused.is_empty(), "{refused:?}");
    Ok(())
}

#[test]
fn a_saved_game_loads_into_the_same_game() -> TestResult {
    let mut g = game(5)?;
    end_turns(&mut g, 3);
    let text = g.snapshot().to_text()?;
    let loaded = f4x_core::game::save::Snapshot::from_text(&text)?.restore(shipped().clone())?;
    assert_eq!(loaded.snapshot().to_text()?, text);
    Ok(())
}

#[test]
fn the_same_seed_and_commands_give_the_same_game() -> TestResult {
    let play = || -> Result<String, Box<dyn std::error::Error>> {
        let mut g = game(9)?;
        end_turns(&mut g, 15);
        Ok(g.snapshot().to_text()?)
    };
    assert_eq!(play()?, play()?);
    Ok(())
}

/// A new game on the shipped content with the map-size counts set by hand.
fn game_with_counts(
    seed: u64,
    towers: Option<u8>,
    neutral_cities: Option<u8>,
) -> Result<Game, String> {
    let mut spec = NewGame {
        seed,
        all_ai: true,
        ..Default::default()
    };
    spec.settings.basic.towers = towers;
    spec.settings.basic.neutral_cities = neutral_cities;
    new_game(shipped(), &spec)
}

fn towers_in(g: &Game) -> usize {
    let economy = g.economy();
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

fn neutral_cities_on(g: &Game, plane: f4x_core::map::Plane) -> usize {
    let economy = g.economy();
    economy
        .cities()
        .iter()
        .filter(|(c, _, pos)| {
            pos.plane == plane
                && economy
                    .city_owner(*c)
                    .is_some_and(|o| g.is_neutral_player(o))
        })
        .count()
}

#[test]
fn the_settings_say_how_many_towers_a_map_gets() -> TestResult {
    for towers in [0u8, 2, 5] {
        let g = game_with_counts(4, Some(towers), None)?;
        assert!(
            towers_in(&g) <= usize::from(towers),
            "{towers} towers asked, {} made",
            towers_in(&g)
        );
        assert!(
            towers_in(&g) + 2 >= usize::from(towers),
            "{towers} towers asked, {} made",
            towers_in(&g)
        );
    }
    // The default follows the area: six on the normal map.
    assert!(towers_in(&game_with_counts(4, None, None)?) >= 4);
    Ok(())
}

#[test]
fn the_settings_cap_the_neutral_cities_of_each_plane() -> TestResult {
    let g = game_with_counts(4, None, Some(3))?;
    for plane in f4x_core::map::Plane::all() {
        let n = neutral_cities_on(&g, plane);
        assert!(n <= 3, "{plane:?}: {n} neutral cities with a cap of 3");
    }
    let open = game_with_counts(4, None, None)?;
    let total = |g: &Game| -> usize {
        f4x_core::map::Plane::all()
            .map(|p| neutral_cities_on(g, p))
            .sum()
    };
    assert!(
        total(&open) > total(&g),
        "without a cap there are more: {} against {}",
        total(&open),
        total(&g)
    );
    Ok(())
}

#[test]
fn a_myrran_wizard_of_nine_books_needs_the_two_pick_option_to_be_made() -> TestResult {
    use f4x_core::rules::RealmId as Realm;
    use f4x_core::wizard::magic::Retort;
    let make = |two_pick: bool| {
        let mut spec = NewGame {
            seed: 5,
            books: [(Realm::new(3 /* nature */), 9)].into(),
            retorts: [Retort::new(9 /* myrran */)].into(),
            ..Default::default()
        };
        spec.settings.rules.two_pick_myrran = two_pick;
        new_game(shipped(), &spec)
    };
    // The books and the retort at the row's cost must overspend the picks, or the test shows
    // nothing; two picks is what the option is named for.
    let rules = &f4x_def::testdir::shipped().rules;
    let picks = NewGame::default().settings.wizards.picks.human;
    assert!(
        9 + f4x_core::wizard::retort_cost(rules, Retort::new(9 /* myrran */)) > picks,
        "nine books and Myrran must be more than the {picks} picks"
    );
    assert!(9 + 2 <= picks, "nine books and a two-pick Myrran fit");
    assert!(make(false).is_err(), "nine books and Myrran overspend");
    let g = make(true)?;
    assert_eq!(
        f4x_core::wizard::retort_cost(&g.content().rules, Retort::new(9)),
        2
    );
    Ok(())
}

#[test]
fn no_wizard_starts_with_more_books_in_a_realm_than_the_setting_allows() -> TestResult {
    for most in [5u8, 6, 8, 13] {
        for randomise in [true, false] {
            for seed in 1..8 {
                let mut spec = NewGame {
                    seed,
                    all_ai: true,
                    ..Default::default()
                };
                spec.settings.basic.opponents = 5;
                spec.settings.wizards.max_books_per_realm = most;
                spec.settings.wizards.max_books_total = 65;
                spec.settings.wizards.picks.ai = 15;
                spec.settings.ai.wizards.randomise = randomise;
                let g = new_game(shipped(), &spec)?;
                let wizards: Vec<_> = g
                    .player_ids()
                    .into_iter()
                    .filter_map(|p| g.wizard(p))
                    .collect();
                assert_eq!(
                    wizards.len(),
                    6,
                    "five opponents and the one who would be you"
                );
                for w in wizards {
                    let books = w.books();
                    assert!(
                        books.values().all(|&n| n <= most),
                        "seed {seed}, at most {most}, randomise {randomise}: {books:?}"
                    );
                    let spent = books.values().sum::<u8>()
                        + w.retorts()
                            .iter()
                            .map(|&r| f4x_core::wizard::retort_cost(&g.content().rules, r))
                            .sum::<u8>();
                    // Whatever the cap took off is spent again, so no wizard has fewer picks
                    // spent than the ready-made ones (ten or eleven), and none more than there
                    // are.
                    assert!(
                        (10..=15).contains(&spent),
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
    let make = |most: u8| {
        let mut spec = NewGame {
            seed: 3,
            preset: Some(11), // Ariel, ten Life books
            ..Default::default()
        };
        spec.settings.wizards.max_books_per_realm = most;
        new_game(shipped(), &spec)
    };
    assert!(make(11).is_ok());
    let refused = make(8).err().ok_or("Ariel is made with surplus picks")?;
    assert!(refused.contains("UnspentPicks"), "{refused}");
    Ok(())
}

#[test]
fn wizards_with_twelve_or_thirteen_books_in_a_realm_play_on() -> TestResult {
    let mut most_seen = 0;
    for seed in 1..40 {
        let mut spec = NewGame {
            seed,
            all_ai: true,
            ..Default::default()
        };
        spec.settings.basic.opponents = 5;
        spec.settings.wizards.max_books_per_realm = 13;
        spec.settings.wizards.max_books_total = 65;
        spec.settings.wizards.picks.ai = 20;
        let mut g = new_game(shipped(), &spec)?;
        for p in g.player_ids() {
            if let Some(w) = g.wizard(p) {
                most_seen = most_seen.max(w.books().values().copied().max().unwrap_or(0));
            }
        }
        let refused: Vec<_> = end_turns(&mut g, 3)
            .into_iter()
            .filter(|e| matches!(e, Event::Rejected(_)))
            .collect();
        assert!(refused.is_empty(), "seed {seed}: {refused:?}");
    }
    assert!(
        most_seen > 11,
        "no wizard reached 12 books in 39 games: {most_seen}"
    );
    Ok(())
}
