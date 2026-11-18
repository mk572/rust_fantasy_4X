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
fn every_level_of_the_file_is_a_valid_level_of_itself() {
    for d in 0..=MAX_DIFFICULTY {
        let s = at(d);
        assert_eq!(s.basic.difficulty, d, "level {d}");
        assert!(s.is_level(&settings()), "level {d}");
        assert!(s.validate().is_empty(), "level {d}: {:?}", s.validate());
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
fn the_most_books_in_a_realm_is_checked_between_five_and_thirteen() {
    let mut s = at(2);
    for ok in [5, 11, 13] {
        s.wizards.max_books_per_realm = ok;
        assert!(s.validate().is_empty(), "{ok}");
    }
    for bad in [0, 4, 14] {
        s.wizards.max_books_per_realm = bad;
        assert_eq!(s.validate().len(), 1, "{bad}");
        assert!(s.normalised().validate().is_empty(), "{bad}");
    }
}

#[test]
fn the_most_books_in_all_realms_is_checked_between_twelve_and_sixty_five() {
    let mut s = at(2);
    for ok in [12, 13, 65] {
        s.wizards.max_books_total = ok;
        assert!(s.validate().is_empty(), "{ok}");
    }
    for bad in [0, 11, 66] {
        s.wizards.max_books_total = bad;
        assert_eq!(s.validate().len(), 1, "{bad}");
        assert!(s.normalised().validate().is_empty(), "{bad}");
    }
}

#[test]
fn the_units_per_square_stay_between_a_battle_side_and_the_ceiling() {
    let mut s = at(2);
    assert!(s.validate().is_empty());
    for bad in [s.rules.max_battle_side - 1, MAX_TILE_LIMIT + 1] {
        s.rules.max_tile = bad;
        assert_eq!(s.validate().len(), 1, "{bad}");
    }
}

#[test]
fn a_battle_side_is_one_to_the_ceiling_and_a_square_holds_at_least_a_side() {
    let mut s = at(2);
    for bad in [0, MAX_STACK + 1] {
        s.rules.max_battle_side = bad;
        assert!(!s.validate().is_empty(), "{bad}");
    }
    s.rules.max_battle_side = 5;
    s.rules.max_tile = 5;
    assert!(s.validate().is_empty(), "{:?}", s.validate());
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
fn out_of_range_values_are_named_and_a_level_past_the_last_is_the_last() {
    assert_eq!(at(9).basic.difficulty, 5);
    // Extreme sits strictly between Hard and Impossible wherever those two differ.
    let (hard, extreme, impossible) = (at(3), at(EXTREME), at(5));
    let between = |h: i32, e: i32, i: i32| (h.min(i)..=h.max(i)).contains(&e);
    assert!(between(
        hard.ai.production_percent.into(),
        extreme.ai.production_percent.into(),
        impossible.ai.production_percent.into()
    ));
    assert!(between(
        hard.scoring.score_percent.into(),
        extreme.scoring.score_percent.into(),
        impossible.scoring.score_percent.into()
    ));
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
        (5, 1, 99, 2)
    );
}

#[test]
fn every_rule_set_gives_a_valid_game_and_the_shipped_ships_carry_what_the_default_says() {
    let def = settings();
    let work = at(2);
    let content = shipped();
    for set in [RuleSet::V131, RuleSet::V160] {
        let applied = set.applied(work, &def);
        assert!(applied.validate().is_empty(), "{:?}", applied.validate());
    }
    // The default rule set's ship capacities are the shipped units' own.
    let v131 = RuleSet::V131.applied(work, &def);
    let carries = |num| {
        content
            .rules
            .units
            .by_num(num)
            .map(|u| u.stats[&crate::rules::Stat::Transport])
    };
    assert_eq!(
        carries(content.rules.config.targets.trireme_unit),
        Some(HalfInt::from_int(v131.rules.trireme_capacity.into()))
    );
    assert_eq!(
        carries(content.rules.config.targets.galley_unit),
        Some(HalfInt::from_int(v131.rules.galley_capacity.into()))
    );
}

#[test]
fn ship_capacity_and_monster_turn_are_kept_in_range() {
    let mut s = at(2);
    s.rules.trireme_capacity = 1;
    s.rules.galley_capacity = 9;
    s.world.monster_start_turn = 20;
    assert_eq!(s.validate().len(), 3, "{:?}", s.validate());
    let fixed = s.normalised();
    assert_eq!(
        (fixed.rules.trireme_capacity, fixed.rules.galley_capacity),
        (2, 8)
    );
    assert_eq!(fixed.world.monster_start_turn, 30);
}

#[test]
fn a_setting_a_file_does_not_name_takes_the_fallback_and_else_the_built_in_value() {
    let def = settings();
    let full = at(2);
    let text = ron::to_string(&full).expect("settings serialize");
    let without = text.replace(",limit_ai_races:false", "");
    assert!(!without.contains("limit_ai_races"), "{text}");
    let plain: GameSettings = ron::from_str(&without).expect("parses without the field");
    assert!(!plain.rules.limit_ai_races, "built-in: off");
    let mut chosen = RuleSet::V160.applied(full, &def);
    chosen.rules.limit_ai_races = true;
    chosen.rules.walls_cover_city = true;
    let loaded: GameSettings = with_fallback(chosen, || ron::from_str(&without)).expect("parses");
    assert!(loaded.rules.limit_ai_races, "the fallback's value");
    let back: GameSettings = ron::from_str(&without).expect("parses");
    assert!(
        !back.rules.limit_ai_races,
        "the fallback is gone after the load"
    );
}

#[test]
fn the_default_game_settings_come_back_from_the_file_they_were_saved_to() {
    let def = settings();
    let dir = crate::testdir::TestDir::new("default-game");
    let path = dir.join("default_game.ron");
    assert_eq!(DefaultGame::load(&path), None);
    let mut rec = DefaultGame::factory(&def);
    assert!(!rec.settings.rules.limit_ai_races);
    rec.settings.rules.limit_ai_races = true;
    rec.save(&path).expect("saves");
    assert_eq!(DefaultGame::load(&path), Some(rec));
}
