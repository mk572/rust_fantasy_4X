use super::*;
use crate::testdir::shipped;

/// The shipped `rules_settings.ron`.
fn settings() -> SettingsDef {
    shipped().settings.clone()
}

/// The shipped level `d`.
fn at(d: u8) -> GameSettings {
    GameSettings::for_difficulty(&settings(), d)
}

#[test]
fn the_new_game_choices_default_to_an_average_game() {
    let s = at(2);
    assert_eq!(s.basic, settings().new_game);
    assert_eq!(s.basic.difficulty, 2);
    assert_eq!(s.basic.magic_strength, 1);
    assert_eq!(s.basic.land_percent, LAND_LARGE);
    assert!(s.validate().is_empty());
}

#[test]
fn the_levels_carry_the_originals_figures() {
    let gold: Vec<(i32, i32)> = (0..5)
        .map(|d| {
            (
                at(d).wizards.starting_gold.human,
                at(d).wizards.starting_gold.ai,
            )
        })
        .collect();
    assert_eq!(
        gold,
        [(125, 150), (100, 150), (75, 150), (50, 150), (25, 150)]
    );
    let picks: Vec<u8> = (0..5).map(|d| at(d).wizards.picks.ai).collect();
    assert_eq!(picks, [11, 11, 11, 13, 15]);
    assert_eq!(at(4).ai.production_percent, 400);
    assert_eq!(at(3).ai.research_percent, 110);
    assert_eq!(at(4).ai.maintenance_percent, 30);
    assert_eq!(at(4).ai.unit_upkeep_percent, 50);
    assert_eq!(at(3).ai.wizards.erasure_dice, (2, 3));
    assert_eq!(at(2).scoring.score_percent, 100);
    assert_eq!(at(4).scoring.score_percent, 300);
    assert!(!at(0).events.random_events && at(1).events.random_events);
    assert!(at(1).rules.safe_flee.human && !at(2).rules.safe_flee.ai);
    // The lair figures are the old formulas: `(d + 1) / 4` of the guards, `4 / (d + 1)` of
    // the treasure (none at Impossible).
    for d in 0..=4u8 {
        assert_eq!(at(d).world.lair_guard_percent, (u16::from(d) + 1) * 25);
    }
    assert_eq!(at(0).world.lair_treasure_percent, 400);
    assert_eq!(at(3).world.lair_treasure_percent, 100);
    // The raiders are `troops * d / 6` of the original.
    for d in 0..=4u8 {
        assert_eq!(
            at(d).world.raider_percent,
            [0, 17, 33, 50, 67][usize::from(d)]
        );
        assert!(at(d).world.raiders_enabled && at(d).world.monsters_enabled);
    }
    for d in 0..=4 {
        assert!(
            at(d).is_level(&settings()) && at(d).validate().is_empty(),
            "level {d}"
        );
    }
}

#[test]
fn the_default_game_is_the_average_level_and_a_changed_value_shows_as_custom() {
    let default = GameSettings::default();
    assert_eq!(default, at(2));
    assert!(default.is_level(&settings()));
    assert!(default.validate().is_empty());
    let mut s = at(2);
    s.wizards.starting_gold.human = 200;
    assert!(!s.is_level(&settings()));
    s.wizards.starting_gold.human = 75;
    assert!(s.is_level(&settings()));
    // Extras do not make a game Custom.
    s.basic.opponents = 3;
    assert!(s.is_level(&settings()));
}

#[test]
fn without_levels_a_difficulty_is_the_default_game_at_that_level() {
    let none = SettingsDef::default();
    let s = GameSettings::for_difficulty(&none, 3);
    assert_eq!(s.basic.difficulty, 3);
    assert_eq!(
        GameSettings {
            basic: BasicSettings::default(),
            ..s
        },
        GameSettings::default()
    );
}

#[test]
fn the_units_per_square_stay_between_an_army_and_the_ceiling() {
    let mut s = at(2);
    assert_eq!(s.rules.max_tile, MAX_TILE_LIMIT);
    assert!(s.validate().is_empty());
    for bad in [MAX_STACK - 1, MAX_TILE_LIMIT + 1] {
        s.rules.max_tile = bad;
        assert_eq!(s.validate().len(), 1, "{bad}");
    }
}

#[test]
fn a_settings_file_is_checked_for_its_level_count_and_ranges() {
    assert!(settings().validate().is_empty());
    let mut def = settings();
    def.levels.pop();
    assert_eq!(def.validate().len(), 1, "{:?}", def.validate());
    let mut def = settings();
    def.max_opponents = 10;
    def.land_sizes.clear();
    assert_eq!(def.validate().len(), 2, "{:?}", def.validate());
    let mut def = settings();
    def.new_game.opponents = 3;
    def.max_opponents = 2;
    def.levels[0].world.start_reveal_radius = crate::half_int::HalfInt::from_int(11);
    def.levels[1].events.pressure_percent = 0;
    assert_eq!(def.validate().len(), 3, "{:?}", def.validate());
    let mut def = settings();
    def.levels[3].ai.gold_percent = 5;
    def.new_game.opponents = 10;
    let problems = def.validate();
    assert!(problems.iter().any(|p| p.starts_with("level 3:")));
    assert!(problems.iter().any(|p| p.starts_with("new_game:")));
}

#[test]
fn every_level_is_a_valid_record_and_out_of_range_values_are_named() {
    for level in 0..=4 {
        let s = at(level);
        assert_eq!(s.basic.difficulty, level);
        assert!(s.validate().is_empty());
    }
    assert_eq!(at(9).basic.difficulty, 4);
    let mut s = at(2);
    s.basic.difficulty = 7;
    s.basic.opponents = 0;
    s.basic.land_percent = 100;
    s.basic.magic_strength = 3;
    assert_eq!(s.validate().len(), 4, "{:?}", s.validate());
    s.ai.gold_percent = 5;
    s.scoring.score_percent = 900;
    assert_eq!(s.validate().len(), 6, "{:?}", s.validate());
    let fixed = s.normalised();
    assert!(fixed.validate().is_empty());
    assert_eq!(
        (
            fixed.basic.difficulty,
            fixed.basic.opponents,
            fixed.basic.land_percent,
            fixed.basic.magic_strength
        ),
        (4, 1, 99, 2)
    );
}
