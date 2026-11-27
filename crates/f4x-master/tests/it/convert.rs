//! A save written by game version 0.0.7 (`fixtures/before_0_0_8.f4x`: a game of two wizards in round
//! 30, one war since round 25) is converted when it is read.

use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::endgame::EndKind;
use f4x_core::world::save::{ContentCheck, LoadError, GAME_VERSION, SAVE_VERSION};
use f4x_master::{GameOver, Phase, Restored, SaveFile};

use crate::support::{busy_round, content};

const OLD: &str = include_str!("../fixtures/before_0_0_8.f4x");
const OLD_VERSION: &str = "0.0.7";

/// The old save with the one place that reads `from` changed to `to`.
fn edited(from: &str, to: &str) -> String {
    assert_eq!(OLD.matches(from).count(), 1, "{from} in the old save");
    OLD.replace(from, to)
}

fn restore(text: &str) -> Result<Restored, LoadError> {
    // The content may have moved on since the save was written; its layout is what is tested.
    SaveFile::from_text(text)?.restore(content(), ContentCheck::Ignore)
}

#[test]
fn an_old_save_gives_a_game_in_its_round_with_its_wars_and_plans() {
    let file = SaveFile::from_text(OLD).expect("converts");
    assert_eq!(file.version, SAVE_VERSION);
    assert_eq!(file.game_version, GAME_VERSION);
    assert_eq!(file.game.round, 30);
    assert_eq!(file.game.phase, Some(Phase::TurnStart(PlayerId(0))));
    assert_eq!(file.game.game_over, None);
    assert_eq!(file.world.mastery, None);
    // The war began in round 25; the two other pairs are at peace.
    let ages: Vec<_> = file
        .world
        .diplomacy
        .pairs
        .iter()
        .filter_map(|(a, b, pair)| pair.war_age.map(|age| (*a, *b, age)))
        .collect();
    assert_eq!(ages, [(1, 113, 5)]);
    assert!(file.plans.iter().any(|(p, _)| *p == PlayerId(0)));

    let mut restored = restore(OLD).expect("loads");
    assert_eq!(restored.game.round(), 30);
    // It plays on.
    busy_round(&mut restored.game, 4);
    assert_eq!(restored.game.round(), 31);
}

#[test]
fn a_restored_old_save_has_a_player_for_every_race_and_lists_who_plays() {
    let saved = SaveFile::from_text(OLD).expect("converts").world.players;
    let game = restore(OLD).expect("loads").game;
    let races = game.content().rules.races.iter().count();
    let of_races: Vec<PlayerId> = game
        .player_ids()
        .into_iter()
        .filter(|&p| game.race_of_player(p).is_some())
        .collect();
    assert_eq!(of_races.len(), races);
    // Those the save did not have come with nothing; those it had keep what they had.
    assert!(of_races
        .iter()
        .filter(|p| !saved.iter().any(|s| s.id == p.0))
        .all(|&p| game.is_dormant(p)));
    for s in &saved {
        let gold = game.player(PlayerId(s.id)).map(|p| p.treasury().gold);
        assert_eq!(gold, Some(s.gold), "player {}", s.id);
    }
    assert_eq!(game.turn_players(), f4x_master::turn_players(game.world()));
}

#[test]
fn a_converted_save_is_written_and_read_as_a_save_of_this_version() {
    let file = SaveFile::from_text(OLD).expect("converts");
    let again = SaveFile::from_text(&file.to_text().expect("writes")).expect("reads");
    assert_eq!(again, file);
}

#[test]
fn how_an_old_game_ended_is_carried_over() {
    for (kind, mastery) in [("SpellOfMastery", Some(PlayerId(1))), ("Conquest", None)] {
        let text = edited(
            "game_over: None,",
            &format!("game_over: Some((winner: (1), kind: {kind}, turn: 29)),"),
        );
        let game = restore(&text).expect("loads").game;
        let kind = if mastery.is_some() {
            EndKind::SpellOfMastery
        } else {
            EndKind::Conquest
        };
        assert_eq!(
            game.game_over(),
            Some(GameOver {
                winner: PlayerId(1),
                kind,
                turn: 29,
            })
        );
        assert_eq!(game.mastered_by(), mastery);
    }
}

/// The place in the round of the old save when it names `place`.
fn phase_of(place: &str) -> Option<Phase> {
    let text = edited(
        "flow: Some(\"(phase:TurnStart(0))\"),",
        &format!("flow: Some(\"(phase:{place})\"),"),
    );
    restore(&text).expect("loads").game.phase()
}

#[test]
fn a_seat_of_an_old_save_becomes_the_player_whose_turn_it_was() {
    let file = SaveFile::from_text(OLD).expect("converts");
    // Two wizards had the seats 0 and 1; seat 2 played every other player at once.
    let first_race = file
        .world
        .players
        .iter()
        .filter(|p| p.neutral && p.id != MONSTERS.0)
        .map(|p| PlayerId(p.id))
        .min_by_key(|p| p.0)
        .expect("a race player");
    for (place, phase) in [
        ("Acting(1)", Phase::Acting(PlayerId(1))),
        ("TurnEnd(0)", Phase::TurnEnd(PlayerId(0))),
        ("TurnStart(2)", Phase::TurnStart(first_race)),
        // All their turns had started: none is started again.
        ("Acting(2)", Phase::Acting(MONSTERS)),
        ("TurnEnd(2)", Phase::TurnEnd(MONSTERS)),
        ("RoundStart", Phase::RoundStart),
        ("RoundEnd", Phase::RoundEnd),
    ] {
        assert_eq!(phase_of(place), Some(phase), "{place}");
    }
    assert_eq!(phase_of("Acting(3)"), None, "a seat there was not");
}

#[test]
fn a_place_in_the_round_that_cannot_be_read_is_none() {
    let text = edited(
        "flow: Some(\"(phase:TurnStart(0))\"),",
        "flow: Some(\"garbage\"),",
    );
    assert_eq!(restore(&text).expect("loads").game.phase(), None);
}

#[test]
fn only_the_versions_that_wrote_the_old_layout_are_converted() {
    let of = |version: &str| {
        // The header and the body both name the version.
        assert!(OLD.contains(&format!("game_version: \"{OLD_VERSION}\"")));
        OLD.replace(
            &format!("game_version: \"{OLD_VERSION}\""),
            &format!("game_version: \"{version}\""),
        )
    };
    assert!(SaveFile::from_text(&of("0.0.2")).is_ok());
    for version in ["0.0.1", "0.0.8", "9999.0.0", "banana", ""] {
        assert!(
            matches!(
                SaveFile::from_text(&of(version)),
                Err(LoadError::GameVersion { .. })
            ),
            "game version {version:?} was converted"
        );
    }
}
