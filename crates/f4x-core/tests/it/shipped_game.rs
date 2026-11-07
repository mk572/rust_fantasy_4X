//! A whole game on the shipped content (`content/`), through the public interface only.

use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::{Command, Event, Game};
use f4x_def::content::Content;
use f4x_def::testdir::TestResult;

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
    end_turns(&mut g, 10);
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
