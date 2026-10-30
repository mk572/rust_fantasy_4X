//! The settings a game is played with: chosen once when the game is made, saved and loaded with
//! it, and read by the rules through [`Game::settings`](crate::game::Game::settings) (task 246,
//! `docs/game-settings.md`).
//!
//! Stage 1 holds the original's four new-game choices. The groups the design lists for later
//! stages (wizards, AI, world, rules, victory, events, scoring) are added when they get a rule to
//! read them. The figures of the difficulty levels and the defaults of the settings screens are
//! data, `content/settings.ron` ([`SettingsDef`]); only [`GameSettings::flat`] and the
//! fallback [`BasicSettings::default`] for content without that file are in the code.

use serde::{Deserialize, Serialize};

use crate::player::PlayerId;

/// Land as a share of each plane's tiles for the original's three sizes.
pub const LAND_SMALL: u8 = 15;
pub const LAND_MEDIUM: u8 = 20;
pub const LAND_LARGE: u8 = 30;

/// The difficulty a game gets when nothing says otherwise ("Average").
pub const DEFAULT_DIFFICULTY: u8 = 2;
/// The map a game gets when nothing says otherwise: the original's 60 by 40 squares.
pub const MAP_WIDTH: u16 = 60;
pub const MAP_HEIGHT: u16 = 40;
/// The sizes the map generator and the rest of the game are tested on (task 343): from 45 by 30
/// to 90 by 60 squares.
pub const MAP_WIDTHS: std::ops::RangeInclusive<u16> = 45..=90;
pub const MAP_HEIGHTS: std::ops::RangeInclusive<u16> = 30..=60;
/// The node setting a game gets when nothing says otherwise ("Normal").
pub const DEFAULT_MAGIC_STRENGTH: u8 = 1;

/// The original's options step: difficulty, opponents, land size and magic power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicSettings {
    /// 0 intro, 1 easy, 2 average, 3 hard, 4 impossible.
    #[serde(default = "default_difficulty")]
    pub difficulty: u8,
    /// Computer wizards, 1 to [`MAX_OPPONENTS`]. `setup::new_game` sets it from the number of wizards it makes.
    #[serde(default = "default_opponents")]
    pub opponents: u8,
    /// Land as a share of each plane's tiles, 1 to 99 (see [`LAND_SMALL`]).
    #[serde(default = "default_land")]
    pub land_percent: u8,
    /// Node strength: 0 weak, 1 normal, 2 powerful.
    #[serde(default = "default_magic")]
    pub magic_strength: u8,
    /// The size of each plane in squares (see [`MAP_WIDTHS`]); the counts of towers, nodes,
    /// sites and neutral cities follow the area.
    #[serde(default = "default_map_width")]
    pub map_width: u16,
    #[serde(default = "default_map_height")]
    pub map_height: u16,
    /// A diagonal step on the overland map costs half as much again as a straight one (3
    /// half-moves on open ground); off, it costs the same (the original's rule). The same goes for
    /// sight: a square seen along a diagonal costs half as much again to see, or no more.
    #[serde(default)]
    pub diagonal_extra_cost: bool,
    /// Make the land with the original's generator (random-walk continents, `NEWG_CreateLands`)
    /// instead of this game's noise-based one. The features, lairs and nodes are placed the same
    /// way either way. Off by default.
    #[serde(default)]
    pub original_map_generator: bool,
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

fn default_map_width() -> u16 {
    MAP_WIDTH
}

fn default_map_height() -> u16 {
    MAP_HEIGHT
}

impl Default for BasicSettings {
    fn default() -> Self {
        Self {
            difficulty: DEFAULT_DIFFICULTY,
            opponents: 1,
            land_percent: LAND_LARGE,
            magic_strength: DEFAULT_MAGIC_STRENGTH,
            map_width: MAP_WIDTH,
            map_height: MAP_HEIGHT,
            diagonal_extra_cost: false,
            original_map_generator: false,
        }
    }
}

impl BasicSettings {
    /// Squares of both planes for each wizard.
    pub fn squares_per_player(&self) -> u32 {
        2 * u32::from(self.map_width) * u32::from(self.map_height) / (u32::from(self.opponents) + 1)
    }

    /// The squares each wizard has when the game leaves fewer than [`MIN_SQUARES_PER_PLAYER`]:
    /// the game still starts, but the player is warned that there may be too little room to
    /// expand.
    pub fn crowding(&self) -> Option<u32> {
        let each = self.squares_per_player();
        (each < MIN_SQUARES_PER_PLAYER).then_some(each)
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
    /// Mana in the treasury at the start (the original: none).
    pub starting_mana: Split<i32>,
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
    /// An AI wizard knows a common summoning spell of its main realm from the start
    /// (`SpellLibraryDef::ai_main_realm_known`), even when erased books left the realm with
    /// one; the original does it on every level. The human wizard never gets it.
    pub guaranteed_summoning_spell: bool,
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
    /// Whether the independent cities raise raiding parties now and then (`threat/raider.rs`).
    pub raiders_enabled: bool,
    /// Whether monsters now and then leave their lair to settle in another and to attack cities
    /// (`threat/monster.rs`).
    pub monsters_enabled: bool,
    /// How big a party is, as a percent of the troops in the city (the original's `d / 6`).
    pub raider_percent: u16,
    /// How far each wizard sees around their capital when the game starts.
    pub start_reveal_radius: Radius,
}

/// A distance in squares on the map: a step along a row or column counts 1, a diagonal step 1.5
/// (1 without `diagonal_extra_cost`; `map::fog::in_radius`), so 2.5 is a 5 by 5 block, without its
/// corners when a diagonal step costs extra. Always a plain number
/// (`validate` refuses the rest), which makes `Eq` sound.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Radius(pub f32);

impl Eq for Radius {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSettings {
    /// A fleeing side loses nobody (the original: the player on turn, Intro and Easy).
    pub safe_flee: Split<bool>,
    /// Building Settlers costs a citizen. On (the original's rule): a city needs two citizens to
    /// build Settlers and loses one when they are finished. Off: Settlers cost production only.
    #[serde(default = "yes")]
    pub settlers_cost_citizen: bool,
    /// How hard a walking unit hits a flying one in melee.
    #[serde(default)]
    pub melee_vs_fliers: MeleeVsFliers,
}

fn yes() -> bool {
    true
}

/// A walking unit's melee attack against a flying unit (divergence 26). The flier hits back at
/// full strength whichever is chosen; thrown and breath attacks are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeVsFliers {
    /// The walker's blows do nothing (the original's rule).
    #[default]
    Zero,
    /// Half strength, rounded up (a planned mod).
    Half,
    /// Full strength: the rule is off.
    Full,
}

impl MeleeVsFliers {
    /// The melee strength a walker has against a flier, given its strength against anything else.
    pub fn strength(self, full: i32) -> i32 {
        match self {
            MeleeVsFliers::Zero => 0,
            MeleeVsFliers::Half => (full + 1) / 2,
            MeleeVsFliers::Full => full,
        }
    }
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
    /// The percent of the turns waited since the last event that count as pressure: the chance
    /// of an event in a turn is the pressure over 512 (`world_event::scaled_pressure`). The
    /// original's figure by level: 50, 66, 75, 80, 100.
    pub pressure_percent: u16,
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
    pub basic: BasicSettings,
    pub wizards: WizardSettings,
    pub ai: AiSettings,
    pub world: WorldSettings,
    pub rules: RuleSettings,
    pub events: EventSettings,
    pub scoring: ScoreSettings,
}

/// One difficulty level of `content/settings.ron`: what choosing it fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelDef {
    pub wizards: WizardSettings,
    pub ai: AiSettings,
    pub world: WorldSettings,
    pub rules: RuleSettings,
    pub events: EventSettings,
    pub scoring: ScoreSettings,
}

/// How many difficulty levels the file has (Intro to Impossible).
pub const LEVELS: usize = 5;

/// One land size on the new-game options step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandSize {
    pub name: String,
    /// Land as a share of each plane's tiles.
    pub land_percent: u8,
}

/// One map size on the new-game options step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSize {
    pub name: String,
    /// Squares across and down, each plane (see [`MAP_WIDTHS`]).
    pub width: u16,
    pub height: u16,
}

/// How many opponents the game offers at most (the original: four). Nine opponents are ten
/// wizards on a 90 by 60 map, 2.25 times the original's biggest map for twice its wizards.
pub const MAX_OPPONENTS: u8 = 9;

/// The least land a wizard should have to expand into, in squares of both planes together
/// (`crowding`): the original's 4800 squares for five wizards is 960 each.
pub const MIN_SQUARES_PER_PLAYER: u32 = 900;

/// Which troops cost half the upkeep, and whether they do. Upkeep is whole food and gold for
/// each unit, so about every second unit costs nothing: the ones with an even serial number pay
/// (the number is the order the game made the units in, kept for each unit for good).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HalfUpkeep {
    pub enabled: bool,
    /// The names of the units (`Spearmen`, `Swordsmen`), the same for every race.
    pub units: Vec<String>,
}

impl Default for HalfUpkeep {
    fn default() -> Self {
        Self {
            enabled: false,
            units: vec!["Spearmen".into(), "Swordsmen".into()],
        }
    }
}

/// `content/settings.ron`: the difficulty levels, the defaults of the new-game choices and of the
/// client's preference switches. A file without it (the test content) leaves `levels` empty, and
/// a game then plays the symmetric [`GameSettings::flat`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsDef {
    /// The language of the messages: a folder of `content/text/` (two letters, `en`).
    pub language: String,
    /// The new-game choices before the player changes them.
    pub new_game: BasicSettings,
    /// The most computer wizards a new game can have, 1 to [`MAX_OPPONENTS`].
    pub max_opponents: u8,
    /// Half the food and gold upkeep for the basic troops (a divergence inspired by the Caster of
    /// Magic mod). Off when the file has no such entry.
    pub half_upkeep: HalfUpkeep,
    /// The land sizes the options step offers, smallest first.
    pub land_sizes: Vec<LandSize>,
    /// The map sizes the options step offers, smallest first.
    pub map_sizes: Vec<MapSize>,
    /// Intro, Easy, Average, Hard, Impossible; empty or exactly [`LEVELS`].
    pub levels: Vec<LevelDef>,
    /// The Settings screen's switches by name (the client knows the names), on or off.
    pub preferences: std::collections::BTreeMap<String, bool>,
    /// The spells a new wizard knows and may research (the original's spell library set-up).
    pub spell_library: SpellLibraryDef,
}

/// How many spells of each rarity: the columns of the tables in [`SpellLibraryDef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rarities {
    pub common: u8,
    pub uncommon: u8,
    pub rare: u8,
    pub very_rare: u8,
}

/// `settings.ron`, `spell_library`: what a new wizard starts with. The tables are indexed by the
/// number of books in a realm, `0` to [`SpellLibraryDef::MAX_BOOKS`] (so they have 12 rows). A
/// count above what the realm has means all of them. The original's numbers are in
/// `INITGAME.c` (`Init_Computer_Players_Spell_Library`) and `NewGame.c`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpellLibraryDef {
    /// Spells of the realm the wizard knows at the start: the realm's chosen spells first, then
    /// its default list (`realms.ron`) in order.
    pub known: Vec<Rarities>,
    /// Spells of the realm the wizard may research, the known ones included (picked at random).
    pub researchable: Vec<Rarities>,
    /// Spells of the realm the wizard chooses at creation (the rest of `known` are defaults).
    pub chosen: Vec<Rarities>,
    /// Spells every wizard knows at the start.
    pub always_known: Vec<String>,
    /// Spells a retort gives at the start.
    pub retort_known: std::collections::BTreeMap<crate::wizard::magic::Retort, Vec<String>>,
    /// Spells of the arcane realm nobody may research at the start (they come by other ways).
    pub arcane_closed: Vec<String>,
    /// A computer wizard also knows this spell of its main realm (the realm with most books;
    /// the first of the realms in file order on a tie), by realm id. The original's table is
    /// off by one spell from what its code names (`INITGAME.c`, `OGBUG`); these are the spells
    /// it really gives, and the ones the wiki lists.
    pub ai_main_realm_known: std::collections::BTreeMap<String, String>,
    /// From this difficulty on (3 is Hard) a computer wizard may also research this spell of its
    /// main realm, by realm id.
    pub ai_main_realm_researchable: std::collections::BTreeMap<String, String>,
    pub ai_main_realm_researchable_from: u8,
}

impl SpellLibraryDef {
    /// Most books in a realm; the tables have one row more.
    pub const MAX_BOOKS: usize = 11;

    /// The row for a number of books (more than the most counts as the most).
    pub fn row(table: &[Rarities], books: u8) -> Rarities {
        let zero = Rarities {
            common: 0,
            uncommon: 0,
            rare: 0,
            very_rare: 0,
        };
        table
            .get(usize::from(books).min(Self::MAX_BOOKS))
            .copied()
            .unwrap_or(zero)
    }

    /// Every problem in words.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (name, rows) in [
            ("known", &self.known),
            ("researchable", &self.researchable),
            ("chosen", &self.chosen),
        ] {
            if rows.len() != Self::MAX_BOOKS + 1 {
                problems.push(format!(
                    "spell_library.{name} has {} rows, not {}",
                    rows.len(),
                    Self::MAX_BOOKS + 1
                ));
            }
        }
        problems
    }
}

impl Default for SpellLibraryDef {
    /// The original's numbers.
    fn default() -> Self {
        let r = |common, uncommon, rare, very_rare| Rarities {
            common,
            uncommon,
            rare,
            very_rare,
        };
        let books = |f: &dyn Fn(u8) -> Rarities| (0..=Self::MAX_BOOKS as u8).map(f).collect();
        const COMMON: [u8; 12] = [0, 3, 5, 6, 7, 8, 9, 10, 10, 10, 10, 99];
        const UNCOMMON: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 8, 99, 99, 99, 99];
        const RARE: [u8; 12] = [0, 0, 1, 2, 3, 4, 5, 6, 7, 9, 99, 99];
        const VERY_RARE: [u8; 12] = [0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 99, 99];
        Self {
            known: books(&|n| match n {
                0 => r(0, 0, 0, 0),
                11 => r(10, 2, 1, 0),
                n => r(n - 1, 0, 0, 0),
            }),
            researchable: books(&|n| {
                let i = usize::from(n);
                r(COMMON[i], UNCOMMON[i], RARE[i], VERY_RARE[i])
            }),
            chosen: books(&|n| match n {
                0 => r(0, 0, 0, 0),
                11 => r(0, 2, 1, 0),
                n => r(n - 1, 0, 0, 0),
            }),
            always_known: vec!["magic_spirit".into(), "spell_of_return".into()],
            retort_known: [(
                crate::wizard::magic::Retort::Artificer,
                vec!["enchant_item".into(), "create_artifact".into()],
            )]
            .into(),
            arcane_closed: vec!["spell_of_mastery".into(), "spell_of_return".into()],
            ai_main_realm_known: [
                ("nature", "sprites"),
                ("sorcery", "nagas"),
                ("chaos", "hell_hounds"),
                ("life", "guardian_spirit"),
                ("death", "ghouls"),
            ]
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
            ai_main_realm_researchable: [
                ("nature", "basilisk"),
                ("sorcery", "phantom_beast"),
                ("chaos", "chimeras"),
                ("life", "unicorns"),
                ("death", "shadow_demons"),
            ]
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
            ai_main_realm_researchable_from: 3,
        }
    }
}

impl Default for SettingsDef {
    /// Content without a `settings.ron`: the original's three land sizes and four opponents.
    fn default() -> Self {
        let size = |name: &str, land_percent| LandSize {
            name: name.into(),
            land_percent,
        };
        Self {
            language: crate::text::FALLBACK_LANGUAGE.into(),
            new_game: BasicSettings::default(),
            max_opponents: MAX_OPPONENTS,
            half_upkeep: HalfUpkeep::default(),
            land_sizes: vec![
                size("Small", LAND_SMALL),
                size("Medium", LAND_MEDIUM),
                size("Large", LAND_LARGE),
            ],
            map_sizes: [
                ("Small", 45, 30),
                ("Medium", 60, 40),
                ("Large", 75, 50),
                ("Huge", 90, 60),
            ]
            .into_iter()
            .map(|(name, width, height)| MapSize {
                name: name.into(),
                width,
                height,
            })
            .collect(),
            levels: Vec::new(),
            preferences: Default::default(),
            spell_library: SpellLibraryDef::default(),
        }
    }
}

impl SettingsDef {
    /// The level `difficulty` (above Impossible counts as Impossible), if the file has levels.
    pub fn level(&self, difficulty: u8) -> Option<&LevelDef> {
        self.levels.get(usize::from(difficulty.min(4)))
    }

    /// Every problem with the file, in words (empty when it is fine): the number of levels, and
    /// each level and the new-game choices within range.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.levels.is_empty() && self.levels.len() != LEVELS {
            problems.push(format!(
                "settings.ron has {} levels, not {LEVELS}",
                self.levels.len()
            ));
        }
        for d in 0..self.levels.len() as u8 {
            let level = GameSettings::for_difficulty(self, d);
            problems.extend(
                level
                    .validate()
                    .into_iter()
                    .map(|p| format!("level {d}: {p}")),
            );
        }
        problems.extend(self.spell_library.validate());
        if !(1..=MAX_OPPONENTS).contains(&self.max_opponents) {
            problems.push(format!(
                "max_opponents {} is not 1 to {MAX_OPPONENTS}",
                self.max_opponents
            ));
        } else if self.new_game.opponents > self.max_opponents {
            problems.push(format!(
                "new_game: {} opponents is over max_opponents {}",
                self.new_game.opponents, self.max_opponents
            ));
        }
        if self.land_sizes.is_empty() {
            problems.push("land_sizes is empty".into());
        }
        for size in &self.land_sizes {
            if size.name.trim().is_empty() || !(1..=99).contains(&size.land_percent) {
                problems.push(format!(
                    "land size {:?}: {}% is not 1 to 99 or the name is empty",
                    size.name, size.land_percent
                ));
            }
        }
        if self.map_sizes.is_empty() {
            problems.push("map_sizes is empty".into());
        }
        for size in &self.map_sizes {
            if size.name.trim().is_empty()
                || !MAP_WIDTHS.contains(&size.width)
                || !MAP_HEIGHTS.contains(&size.height)
            {
                problems.push(format!(
                    "map size {:?}: {} by {} is not {:?} by {:?} or the name is empty",
                    size.name, size.width, size.height, MAP_WIDTHS, MAP_HEIGHTS
                ));
            }
        }
        let choices = GameSettings {
            basic: self.new_game,
            ..GameSettings::flat()
        };
        problems.extend(
            choices
                .validate()
                .into_iter()
                .map(|p| format!("new_game: {p}")),
        );
        problems
    }
}

impl GameSettings {
    /// The settings of a difficulty level: the file's figures for it, with the other basic
    /// choices at their defaults. Without levels in the file it is [`GameSettings::flat`].
    pub fn for_difficulty(def: &SettingsDef, difficulty: u8) -> Self {
        let d = difficulty.min(4);
        let Some(level) = def.level(d) else {
            return Self {
                basic: BasicSettings {
                    difficulty: d,
                    ..def.new_game
                },
                ..Self::flat()
            };
        };
        Self {
            basic: BasicSettings {
                difficulty: d,
                ..def.new_game
            },
            wizards: level.wizards,
            ai: level.ai,
            world: level.world,
            rules: level.rules,
            events: level.events,
            scoring: level.scoring,
        }
    }

    /// The symmetric game the project played before the original's difficulty effects: the same
    /// gold for everyone, no bonuses for the computer wizards, the next free ready-made wizard
    /// for each, no safe flee, no score scale. **Internal**: for the soak, the learning AI and
    /// the tests, never offered on a screen.
    pub fn flat() -> Self {
        Self {
            basic: BasicSettings::default(),
            wizards: WizardSettings {
                starting_gold: Split::both(100),
                starting_mana: Split::both(0),
                picks: Split::both(11),
            },
            ai: AiSettings {
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
                    guaranteed_summoning_spell: false,
                },
            },
            world: WorldSettings {
                lair_guard_percent: 75,
                lair_treasure_percent: 133,
                neutral_city_size_bonus: 1,
                raiders_enabled: false,
                monsters_enabled: false,
                raider_percent: 33,
                start_reveal_radius: Radius(2.5),
            },
            rules: RuleSettings {
                safe_flee: Split::both(false),
                settlers_cost_citizen: true,
                melee_vs_fliers: MeleeVsFliers::Zero,
            },
            events: EventSettings {
                random_events: true,
                pressure_percent: 75,
                frequency: EventFrequency::Normal,
            },
            scoring: ScoreSettings { score_percent: 100 },
        }
    }

    /// The same game with `n` opponents.
    pub fn with_opponents(mut self, n: u8) -> Self {
        self.basic.opponents = n;
        self
    }

    /// Whether every value the original's difficulty fills in is the level's own (the advanced
    /// screen shows "Custom" when it is not).
    pub fn is_level(&self, def: &SettingsDef) -> bool {
        let level = Self::for_difficulty(def, self.basic.difficulty);
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
            (1..=MAX_OPPONENTS).contains(&b.opponents),
            format!("{} opponents is not 1 to {MAX_OPPONENTS}", b.opponents),
        );
        check(
            (1..=99).contains(&b.land_percent),
            format!("land {}% is not 1 to 99", b.land_percent),
        );
        check(
            b.magic_strength <= 2,
            format!("magic power {} is not 0 to 2", b.magic_strength),
        );
        check(
            MAP_WIDTHS.contains(&b.map_width) && MAP_HEIGHTS.contains(&b.map_height),
            format!(
                "map {} by {} is not {:?} by {:?}",
                b.map_width, b.map_height, MAP_WIDTHS, MAP_HEIGHTS
            ),
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
        for (who, mana) in [
            ("your", w.starting_mana.human),
            ("the AI's", w.starting_mana.ai),
        ] {
            check(
                (0..=2000).contains(&mana),
                format!("{who} starting mana {mana} is not 0 to 2000"),
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
            self.world.raider_percent <= 300,
            format!("raiders {}% is over 300", self.world.raider_percent),
        );
        let radius = self.world.start_reveal_radius.0;
        check(
            radius.is_finite() && (0.0..=10.0).contains(&radius),
            format!("start reveal radius {radius} is not 0 to 10"),
        );
        check(
            (10..=200).contains(&self.events.pressure_percent),
            format!(
                "event pressure {}% is not 10 to 200",
                self.events.pressure_percent
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
        b.opponents = b.opponents.clamp(1, MAX_OPPONENTS);
        b.land_percent = b.land_percent.clamp(1, 99);
        b.magic_strength = b.magic_strength.min(2);
        b.map_width = b.map_width.clamp(*MAP_WIDTHS.start(), *MAP_WIDTHS.end());
        b.map_height = b.map_height.clamp(*MAP_HEIGHTS.start(), *MAP_HEIGHTS.end());
        let w = &mut self.wizards;
        w.starting_gold.human = w.starting_gold.human.clamp(0, 2000);
        w.starting_gold.ai = w.starting_gold.ai.clamp(0, 2000);
        w.starting_mana.human = w.starting_mana.human.clamp(0, 2000);
        w.starting_mana.ai = w.starting_mana.ai.clamp(0, 2000);
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
        self.world.raider_percent = self.world.raider_percent.min(300);
        let radius = self.world.start_reveal_radius.0;
        self.world.start_reveal_radius = Radius(if radius.is_finite() {
            radius.clamp(0.0, 10.0)
        } else {
            2.5
        });
        self.events.pressure_percent = self.events.pressure_percent.clamp(10, 200);
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
    use crate::game::testutil::settings;

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
    fn flat_is_the_symmetric_game_and_shows_as_custom() {
        let flat = GameSettings::flat();
        assert_eq!(flat.wizards.starting_gold, Split::both(100));
        assert_eq!(flat.ai.production_percent, 100);
        assert!(!flat.ai.wizards.randomise);
        assert!(!flat.is_level(&settings()), "it is not Average");
        assert!(flat.validate().is_empty());
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
    fn without_levels_a_difficulty_is_the_flat_game_at_that_level() {
        let none = SettingsDef::default();
        let s = GameSettings::for_difficulty(&none, 3);
        assert_eq!(s.basic.difficulty, 3);
        assert_eq!(
            GameSettings {
                basic: BasicSettings::default(),
                ..s
            },
            GameSettings::flat()
        );
    }

    #[test]
    fn the_shipped_flat_game_is_the_averages_world() {
        // `flat` keeps the Average level's world, events and score, as it always did.
        let (flat, average) = (GameSettings::flat(), at(2));
        // except that nothing raids or roams in it, so tests and soaks stay as they were.
        assert_eq!(
            flat.world,
            WorldSettings {
                raiders_enabled: false,
                monsters_enabled: false,
                ..average.world
            }
        );
        assert_eq!(flat.events, average.events);
        assert_eq!(flat.scoring, average.scoring);
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
        def.levels[0].world.start_reveal_radius = Radius(f32::NAN);
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
}
