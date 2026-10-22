//! The settings a game is played with: chosen once when the game is made, saved and loaded with
//! it, and read by the rules through [`Game::settings`](crate::game::Game::settings) (task 246,
//! `docs/game-settings.md`).
//!
//! Stage 1 holds the original's four new-game choices. The groups the design lists for later
//! stages (wizards, AI, world, rules, victory, events, scoring) are added as fields with
//! `#[serde(default)]` when they get a rule to read them, so no save needs a migration for them.

use serde::{Deserialize, Serialize};

use crate::player::PlayerId;

/// Land as a share of each plane's tiles for the original's three sizes.
pub const LAND_SMALL: u8 = 15;
pub const LAND_MEDIUM: u8 = 20;
pub const LAND_LARGE: u8 = 30;

/// The difficulty a game gets when nothing says otherwise ("Average").
pub const DEFAULT_DIFFICULTY: u8 = 2;
/// The node setting a game gets when nothing says otherwise ("Normal").
pub const DEFAULT_MAGIC_STRENGTH: u8 = 1;

/// The original's options step: difficulty, opponents, land size and magic power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicSettings {
    /// 0 intro, 1 easy, 2 average, 3 hard, 4 impossible.
    #[serde(default = "default_difficulty")]
    pub difficulty: u8,
    /// Computer wizards, 1 to 4. `setup::new_game` sets it from the number of wizards it makes.
    #[serde(default = "default_opponents")]
    pub opponents: u8,
    /// Land as a share of each plane's tiles, 1 to 99 (see [`LAND_SMALL`]).
    #[serde(default = "default_land")]
    pub land_percent: u8,
    /// Node strength: 0 weak, 1 normal, 2 powerful.
    #[serde(default = "default_magic")]
    pub magic_strength: u8,
}

fn default_difficulty() -> u8 {
    DEFAULT_DIFFICULTY
}

fn default_opponents() -> u8 {
    1
}

fn default_land() -> u8 {
    LAND_LARGE
}

fn default_magic() -> u8 {
    DEFAULT_MAGIC_STRENGTH
}

impl Default for BasicSettings {
    fn default() -> Self {
        Self {
            difficulty: DEFAULT_DIFFICULTY,
            opponents: 1,
            land_percent: LAND_LARGE,
            magic_strength: DEFAULT_MAGIC_STRENGTH,
        }
    }
}

/// A value that can differ between the human wizard and the computer wizards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split<T> {
    pub human: T,
    pub ai: T,
}

impl<T: Copy> Split<T> {
    pub fn both(value: T) -> Self {
        Self {
            human: value,
            ai: value,
        }
    }

    /// The value for a human or for a computer wizard.
    pub fn of(&self, human: bool) -> T {
        if human {
            self.human
        } else {
            self.ai
        }
    }
}

/// What a wizard starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardSettings {
    /// Gold at the start: the original gives the human `(5 - difficulty) * 25` and every
    /// computer wizard 150.
    pub starting_gold: Split<i32>,
    /// Picks for books and retorts. The human's creation screen still spends 11 (it reads this
    /// in stage 3); the computer wizards' is read when they are made.
    pub picks: Split<u8>,
}

/// How the computer wizards are made (`docs/game-settings.md` 4.8, from the original's
/// `Init_Computer_Players_Wizard_Profile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiWizardSettings {
    /// Draw ready-made wizards at random and fill their picks up to `picks.ai`; off takes the
    /// next free ready-made wizard as it is.
    pub randomise: bool,
    /// Chance (percent) that an AI wizard's opposed realms are merged (Death with Chaos, Nature
    /// or Sorcery; Life with Chaos, Nature or Sorcery): the original does it above Normal.
    pub consolidate_percent: u8,
    /// "Erasure dice": the number of dice and their sides. Each pip rolled takes a book or the
    /// retorts away from an AI wizard.
    pub erasure_dice: (u8, u8),
    /// With four or more wizards, one or two AI wizards are made Myrran (three books become
    /// the retort) when none is: the original does it above Normal.
    pub myrran_rule: bool,
}

/// The original's difficulty bonuses for the computer wizards (`difficulty_modifiers_table`),
/// as percents of the figure a human would get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    pub growth_percent: u16,
    pub outpost_growth_percent: u16,
    pub production_percent: u16,
    pub gold_percent: u16,
    pub mana_percent: u16,
    pub food_percent: u16,
    pub research_percent: u16,
    /// Building maintenance.
    pub maintenance_percent: u16,
    /// Gold upkeep of units.
    pub unit_upkeep_percent: u16,
    pub wizards: AiWizardSettings,
}

/// The neutral world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSettings {
    /// Guards of lairs, towers and nodes, as a percent of the budget.
    pub lair_guard_percent: u16,
    /// Treasure of lairs, as a percent.
    pub lair_treasure_percent: u16,
    /// Added to the starting size of a neutral city.
    pub neutral_city_size_bonus: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSettings {
    /// A fleeing side loses nobody (the original: the player on turn, Intro and Easy).
    pub safe_flee: Split<bool>,
}

/// How often the random events (task 68) fire, as a multiplier on the original's chance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EventFrequency {
    Half,
    #[default]
    Normal,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSettings {
    /// The original switches events off on Intro.
    pub random_events: bool,
    #[serde(default)]
    pub frequency: EventFrequency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreSettings {
    /// The final score as a percent: 50, 75, 100, 200, 300 by level in the original.
    pub score_percent: u16,
}

/// Everything a game is played with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    #[serde(default)]
    pub basic: BasicSettings,
    #[serde(default)]
    pub wizards: WizardSettings,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub world: WorldSettings,
    #[serde(default)]
    pub rules: RuleSettings,
    #[serde(default)]
    pub events: EventSettings,
    #[serde(default)]
    pub scoring: ScoreSettings,
}

/// The original's figure for each level (Intro to Impossible).
fn by_level<T: Copy>(table: [T; 5], difficulty: u8) -> T {
    table[usize::from(difficulty.min(4))]
}

impl WizardSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            starting_gold: Split {
                human: (5 - i32::from(d.min(4))) * 25,
                ai: 150,
            },
            picks: Split {
                human: 11,
                ai: by_level([11, 11, 11, 13, 15], d),
            },
        }
    }
}

impl AiSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            growth_percent: by_level([100, 100, 150, 200, 400], d),
            outpost_growth_percent: by_level([100, 100, 150, 200, 400], d),
            production_percent: by_level([100, 125, 150, 200, 400], d),
            gold_percent: by_level([100, 125, 150, 200, 400], d),
            mana_percent: by_level([100, 125, 150, 200, 400], d),
            food_percent: by_level([100, 125, 150, 200, 400], d),
            research_percent: by_level([100, 100, 100, 110, 150], d),
            maintenance_percent: by_level([100, 90, 75, 60, 30], d),
            unit_upkeep_percent: by_level([100, 100, 100, 75, 50], d),
            wizards: AiWizardSettings {
                randomise: true,
                consolidate_percent: by_level([0, 0, 0, 20, 20], d),
                erasure_dice: by_level([(0, 0), (0, 0), (1, 3), (2, 3), (2, 5)], d),
                myrran_rule: d > 2,
            },
        }
    }
}

impl WorldSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            // `(d + 1) / 4` of the guards' budget and `4 / (d + 1)` of the treasure, none at
            // Impossible (`MAPGEN.c`).
            lair_guard_percent: by_level([25, 50, 75, 100, 125], d),
            lair_treasure_percent: by_level([400, 200, 133, 100, 100], d),
            neutral_city_size_bonus: by_level([0, 0, 1, 1, 1], d),
        }
    }
}

impl RuleSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            safe_flee: Split::both(d <= 1),
        }
    }
}

impl EventSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            random_events: d != 0,
            frequency: EventFrequency::Normal,
        }
    }
}

impl ScoreSettings {
    fn for_difficulty(d: u8) -> Self {
        Self {
            score_percent: by_level([50, 75, 100, 200, 300], d),
        }
    }
}

macro_rules! group_default {
    ($($group:ident),*) => {$(
        impl Default for $group {
            fn default() -> Self {
                Self::for_difficulty(DEFAULT_DIFFICULTY)
            }
        }
    )*};
}
group_default!(
    WizardSettings,
    AiSettings,
    WorldSettings,
    RuleSettings,
    EventSettings,
    ScoreSettings
);

impl Default for GameSettings {
    fn default() -> Self {
        Self::for_difficulty(DEFAULT_DIFFICULTY)
    }
}

impl GameSettings {
    /// The settings of a difficulty level: the original's figures for it, with the other basic
    /// choices at their defaults.
    pub fn for_difficulty(difficulty: u8) -> Self {
        let d = difficulty.min(4);
        Self {
            basic: BasicSettings {
                difficulty: d,
                ..BasicSettings::default()
            },
            wizards: WizardSettings::for_difficulty(d),
            ai: AiSettings::for_difficulty(d),
            world: WorldSettings::for_difficulty(d),
            rules: RuleSettings::for_difficulty(d),
            events: EventSettings::for_difficulty(d),
            scoring: ScoreSettings::for_difficulty(d),
        }
    }

    /// The symmetric game the project played before the original's difficulty effects: the same
    /// gold for everyone, no bonuses for the computer wizards, the next free ready-made wizard
    /// for each, no safe flee, no score scale. **Internal**: for the soak, the learning AI and
    /// the tests, never offered on a screen.
    pub fn flat() -> Self {
        let mut s = Self::for_difficulty(DEFAULT_DIFFICULTY);
        s.wizards = WizardSettings {
            starting_gold: Split::both(100),
            picks: Split::both(11),
        };
        s.ai = AiSettings {
            growth_percent: 100,
            outpost_growth_percent: 100,
            production_percent: 100,
            gold_percent: 100,
            mana_percent: 100,
            food_percent: 100,
            research_percent: 100,
            maintenance_percent: 100,
            unit_upkeep_percent: 100,
            wizards: AiWizardSettings {
                randomise: false,
                consolidate_percent: 0,
                erasure_dice: (0, 0),
                myrran_rule: false,
            },
        };
        s.rules.safe_flee = Split::both(false);
        s
    }

    /// The same game with `n` opponents.
    pub fn with_opponents(mut self, n: u8) -> Self {
        self.basic.opponents = n;
        self
    }

    /// Whether every value the original's difficulty fills in is the level's own (the advanced
    /// screen shows "Custom" when it is not).
    pub fn is_level(&self) -> bool {
        let level = Self::for_difficulty(self.basic.difficulty);
        self.wizards == level.wizards
            && self.ai == level.ai
            && self.world == level.world
            && self.rules == level.rules
            && self.events.random_events == level.events.random_events
            && self.scoring == level.scoring
    }

    /// Every value out of its range, in words (empty when the record is fine).
    pub fn validate(&self) -> Vec<String> {
        let b = &self.basic;
        let mut problems = Vec::new();
        let mut check = |ok: bool, what: String| {
            if !ok {
                problems.push(what);
            }
        };
        check(
            b.difficulty <= 4,
            format!("difficulty {} is not 0 to 4", b.difficulty),
        );
        check(
            (1..=4).contains(&b.opponents),
            format!("{} opponents is not 1 to 4", b.opponents),
        );
        check(
            (1..=99).contains(&b.land_percent),
            format!("land {}% is not 1 to 99", b.land_percent),
        );
        check(
            b.magic_strength <= 2,
            format!("magic power {} is not 0 to 2", b.magic_strength),
        );
        let w = &self.wizards;
        for (who, gold) in [
            ("your", w.starting_gold.human),
            ("the AI's", w.starting_gold.ai),
        ] {
            check(
                (0..=2000).contains(&gold),
                format!("{who} starting gold {gold} is not 0 to 2000"),
            );
        }
        for (who, picks) in [("your", w.picks.human), ("the AI's", w.picks.ai)] {
            check(
                (5..=20).contains(&picks),
                format!("{who} picks {picks} is not 5 to 20"),
            );
        }
        let a = &self.ai;
        for (what, v) in [
            ("growth", a.growth_percent),
            ("outpost growth", a.outpost_growth_percent),
            ("production", a.production_percent),
            ("gold", a.gold_percent),
            ("mana", a.mana_percent),
            ("food", a.food_percent),
            ("research", a.research_percent),
            ("maintenance", a.maintenance_percent),
            ("unit upkeep", a.unit_upkeep_percent),
        ] {
            check(
                (10..=800).contains(&v),
                format!("AI {what} {v}% is not 10 to 800"),
            );
        }
        let aw = &a.wizards;
        check(
            aw.consolidate_percent <= 100,
            format!("merge chance {}% is over 100", aw.consolidate_percent),
        );
        check(
            aw.erasure_dice.0 <= 10 && aw.erasure_dice.1 <= 20,
            format!("erasure dice {:?} are out of range", aw.erasure_dice),
        );
        check(
            self.world.lair_guard_percent <= 300,
            format!("lair guards {}% is over 300", self.world.lair_guard_percent),
        );
        check(
            self.world.lair_treasure_percent <= 500,
            format!(
                "lair treasure {}% is over 500",
                self.world.lair_treasure_percent
            ),
        );
        check(
            self.world.neutral_city_size_bonus <= 6,
            format!(
                "neutral city bonus {} is over 6",
                self.world.neutral_city_size_bonus
            ),
        );
        check(
            (10..=500).contains(&self.scoring.score_percent),
            format!("score {}% is not 10 to 500", self.scoring.score_percent),
        );
        problems
    }

    /// The record with every value brought into range (what a load does).
    pub fn normalised(mut self) -> Self {
        let b = &mut self.basic;
        b.difficulty = b.difficulty.min(4);
        b.opponents = b.opponents.clamp(1, 4);
        b.land_percent = b.land_percent.clamp(1, 99);
        b.magic_strength = b.magic_strength.min(2);
        let w = &mut self.wizards;
        w.starting_gold.human = w.starting_gold.human.clamp(0, 2000);
        w.starting_gold.ai = w.starting_gold.ai.clamp(0, 2000);
        w.picks.human = w.picks.human.clamp(5, 20);
        w.picks.ai = w.picks.ai.clamp(5, 20);
        let a = &mut self.ai;
        for v in [
            &mut a.growth_percent,
            &mut a.outpost_growth_percent,
            &mut a.production_percent,
            &mut a.gold_percent,
            &mut a.mana_percent,
            &mut a.food_percent,
            &mut a.research_percent,
            &mut a.maintenance_percent,
            &mut a.unit_upkeep_percent,
        ] {
            *v = (*v).clamp(10, 800);
        }
        a.wizards.consolidate_percent = a.wizards.consolidate_percent.min(100);
        a.wizards.erasure_dice = (
            a.wizards.erasure_dice.0.min(10),
            a.wizards.erasure_dice.1.min(20),
        );
        self.world.lair_guard_percent = self.world.lair_guard_percent.min(300);
        self.world.lair_treasure_percent = self.world.lair_treasure_percent.min(500);
        self.world.neutral_city_size_bonus = self.world.neutral_city_size_bonus.min(6);
        self.scoring.score_percent = self.scoring.score_percent.clamp(10, 500);
        self
    }
}

impl crate::game::Game {
    /// Give each computer wizard its difficulty bonuses from the settings (a human wizard and
    /// the neutral players get none). Called when a game is made and when one is loaded, after
    /// the human is known.
    pub(crate) fn apply_wizard_settings(&mut self) {
        let ai = self.settings.ai;
        let human = self.human();
        for (_, (&id, bonus, neutral)) in self.world.query_mut::<(
            &PlayerId,
            &mut crate::player::Bonus,
            Option<&crate::player::Neutral>,
        )>() {
            let computer = neutral.is_none() && Some(id) != human;
            *bonus = if computer {
                crate::player::Bonus {
                    growth: i32::from(ai.growth_percent),
                    outpost_growth: i32::from(ai.outpost_growth_percent),
                    production: i32::from(ai.production_percent),
                    gold: i32::from(ai.gold_percent),
                    mana: i32::from(ai.mana_percent),
                    food: i32::from(ai.food_percent),
                    research: i32::from(ai.research_percent),
                    maintenance: i32::from(ai.maintenance_percent),
                    unit_upkeep: i32::from(ai.unit_upkeep_percent),
                }
            } else {
                crate::player::Bonus::default()
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_an_average_game() {
        let s = GameSettings::default();
        assert_eq!(s.basic.difficulty, 2);
        assert_eq!(s.basic.magic_strength, 1);
        assert_eq!(s.basic.land_percent, LAND_LARGE);
        assert!(s.validate().is_empty());
    }

    #[test]
    fn the_levels_carry_the_originals_figures() {
        let at = GameSettings::for_difficulty;
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
        for d in 0..=4 {
            assert!(at(d).is_level() && at(d).validate().is_empty(), "level {d}");
        }
    }

    #[test]
    fn flat_is_the_symmetric_game_and_shows_as_custom() {
        let flat = GameSettings::flat();
        assert_eq!(flat.wizards.starting_gold, Split::both(100));
        assert_eq!(flat.ai.production_percent, 100);
        assert!(!flat.ai.wizards.randomise);
        assert!(!flat.is_level(), "it is not Average");
        assert!(flat.validate().is_empty());
        let mut s = GameSettings::default();
        s.wizards.starting_gold.human = 200;
        assert!(!s.is_level());
        s.wizards.starting_gold.human = 75;
        assert!(s.is_level());
        // Extras do not make a game Custom.
        s.basic.opponents = 3;
        assert!(s.is_level());
    }

    #[test]
    fn every_level_is_a_valid_record_and_out_of_range_values_are_named() {
        for level in 0..=4 {
            let s = GameSettings::for_difficulty(level);
            assert_eq!(s.basic.difficulty, level);
            assert!(s.validate().is_empty());
        }
        assert_eq!(GameSettings::for_difficulty(9).basic.difficulty, 4);
        let mut s = GameSettings::default();
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

    #[test]
    fn a_record_with_missing_fields_reads_with_the_defaults() {
        let s: GameSettings = ron::from_str("(basic: (difficulty: 3))").unwrap();
        assert_eq!(s.basic.difficulty, 3);
        assert_eq!(s.basic.land_percent, LAND_LARGE);
        let s: GameSettings = ron::from_str("()").unwrap();
        assert_eq!(s, GameSettings::default());
    }
}
