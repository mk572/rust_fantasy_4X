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
    // The original's five levels are 0 to 3 and Impossible, which sits at 5 since Extreme (4) was
    // put between Hard and Impossible.
    const ORIGINAL: [(u8, u8); 5] = [(0, 0), (1, 1), (2, 2), (3, 3), (5, 4)];
    let gold: Vec<(i32, i32)> = ORIGINAL
        .iter()
        .map(|&(d, _)| {
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
    let picks: Vec<u8> = ORIGINAL
        .iter()
        .map(|&(d, _)| at(d).wizards.picks.ai)
        .collect();
    assert_eq!(picks, [11, 11, 11, 13, 15]);
    assert_eq!(at(5).ai.production_percent, 400);
    assert_eq!(at(3).ai.research_percent, 110);
    assert_eq!(at(5).ai.maintenance_percent, 30);
    assert_eq!(at(5).ai.unit_upkeep_percent, 50);
    assert_eq!(at(3).ai.wizards.erasure_dice, (2, 3));
    assert_eq!(at(2).scoring.score_percent, 100);
    assert_eq!(at(5).scoring.score_percent, 300);
    assert!(!at(0).events.random_events && at(1).events.random_events);
    assert!(at(1).rules.safe_flee.human && !at(2).rules.safe_flee.ai);
    // The lair figures are the old formulas: `(d + 1) / 4` of the guards, `4 / (d + 1)` of
    // the treasure (none at Impossible), d being the original's level.
    for (level, d) in ORIGINAL {
        assert_eq!(at(level).world.lair_guard_percent, (u16::from(d) + 1) * 25);
    }
    assert_eq!(at(0).world.lair_treasure_percent, 400);
    assert_eq!(at(3).world.lair_treasure_percent, 100);
    // The raiders are `troops * d / 6` of the original.
    for (level, d) in ORIGINAL {
        assert_eq!(
            at(level).world.raider_percent,
            [0, 17, 33, 50, 67][usize::from(d)]
        );
        assert!(at(level).world.raiders_enabled && at(level).world.monsters_enabled);
    }
    for d in 0..=5 {
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
    assert_eq!(s.rules.max_tile, 12, "v1.31 allows twelve on a square");
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
    for level in 0..=5 {
        let s = at(level);
        assert_eq!(s.basic.difficulty, level);
        assert!(s.validate().is_empty());
    }
    assert_eq!(at(9).basic.difficulty, 5);
    // Extreme (4): the patch's AI bonuses, the other figures between Hard and Impossible.
    let (hard, extreme, impossible) = (at(3), at(EXTREME), at(5));
    assert_eq!(
        (
            extreme.ai.growth_percent,
            extreme.ai.production_percent,
            extreme.ai.gold_percent,
            extreme.ai.food_percent,
            extreme.ai.research_percent,
            extreme.ai.maintenance_percent,
            extreme.ai.unit_upkeep_percent,
        ),
        (300, 300, 300, 300, 130, 40, 60)
    );
    assert_eq!(
        (
            extreme.wizards.starting_gold.human,
            extreme.wizards.picks.ai,
            extreme.ai.wizards.erasure_dice,
            extreme.world.lair_guard_percent,
            extreme.world.raider_percent,
            extreme.events.pressure_percent,
            extreme.scoring.score_percent,
        ),
        (40, 14, (2, 4), 110, 60, 90, 250)
    );
    // Strictly between Hard and Impossible wherever those two differ.
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
fn v131_has_the_originals_ships_and_monster_turn_and_v160_has_eight_carrying_ships() {
    let def = settings();
    let work = at(2);
    let v131 = RuleSet::V131.applied(work, &def);
    assert_eq!(
        (v131.rules.trireme_capacity, v131.rules.galley_capacity),
        (2, 5)
    );
    assert_eq!(v131.world.monster_start_turn, 50);
    let v160 = RuleSet::V160.applied(work, &def);
    assert_eq!(
        (v160.rules.trireme_capacity, v160.rules.galley_capacity),
        (8, 8)
    );
    assert!(v160.validate().is_empty(), "{:?}", v160.validate());
    assert!(v160.rules.walls_cover_city && v160.rules.suppress_spares_return);
    assert!(!v131.rules.walls_cover_city && !v131.rules.suppress_spares_return);
    assert!(v160.rules.limit_ai_races && v131.rules.limit_ai_races);
    assert!(v160.rules.ban_life_and_death && v131.rules.ban_life_and_death);
    // The shipped units hold the originals' numbers for the default.
    let content = shipped();
    let carries = |num| {
        content
            .rules
            .units
            .by_num(num)
            .map(|u| u.stats[&crate::rules::Stat::Transport])
    };
    assert_eq!(carries(TRIREME_UNIT), Some(HalfInt::from_int(2)));
    assert_eq!(carries(GALLEY_UNIT), Some(HalfInt::from_int(5)));
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
    let without = text.replace(",limit_ai_races:true", "");
    assert!(!without.contains("limit_ai_races"), "{text}");
    let plain: GameSettings = ron::from_str(&without).expect("parses without the field");
    assert!(plain.rules.limit_ai_races, "built-in: on");
    let mut chosen = RuleSet::V160.applied(full, &def);
    chosen.rules.limit_ai_races = false;
    chosen.rules.walls_cover_city = true;
    let loaded: GameSettings = with_fallback(chosen, || ron::from_str(&without)).expect("parses");
    assert!(!loaded.rules.limit_ai_races, "the fallback's value");
    let back: GameSettings = ron::from_str(&without).expect("parses");
    assert!(
        back.rules.limit_ai_races,
        "the fallback is gone after the load"
    );
}

#[test]
fn the_default_game_settings_start_as_v131_and_a_file_gives_them_back() {
    let def = settings();
    let dir = crate::testdir::TestDir::new("default-game");
    let path = dir.join("default_game.ron");
    assert_eq!(DefaultGame::load(&path), None);
    let mut rec = DefaultGame::factory(&def);
    assert_eq!(rec.preset, RuleSet::V131);
    assert!(rec.settings.rules.limit_ai_races);
    rec.settings.rules.limit_ai_races = false;
    rec.save(&path).expect("saves");
    assert_eq!(DefaultGame::load(&path), Some(rec));
}
