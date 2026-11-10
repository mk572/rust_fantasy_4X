//! The settings a game is played with (the data half; `f4x-core` reads them in `game/setting.rs`): chosen once when the game is made, saved and loaded with
//! it, and read by the rules through `Game::settings`
//! (`docs/game-settings.md`).
//!
//! The figures of the difficulty levels and the defaults of the settings screens are
//! data, `crates/f4x-def/content/rules_settings.ron` ([`SettingsDef`]); only the default game (the Average level, read from that file) and the
//! fallback [`BasicSettings::default`] for content without that file are in the code.

use crate::half_int::HalfInt;
use serde::{Deserialize, Serialize};

use crate::rules::{Id, RealmId, RetortDef, SpellId};

/// A retort, by number.
pub type Retort = Id<RetortDef>;

/// The area the original's counts are for: 60 by 40 squares.
pub const REFERENCE_AREA: u32 = 60 * 40;

/// `base` per [`REFERENCE_AREA`] squares, for a map of `width` by `height`, rounded.
pub fn scaled_count(base: u32, width: u32, height: u32) -> u32 {
    (base * width * height + REFERENCE_AREA / 2) / REFERENCE_AREA
}

/// Towers of wisdom for each [`REFERENCE_AREA`] (the original's).
pub const TOWERS_PER_REFERENCE_AREA: u32 = 6;

/// Neutral cities on a plane of [`REFERENCE_AREA`] squares.
pub const NEUTRAL_CITIES_PER_PLANE: u32 = 15;

/// The most units a game may let stand on one tile: the ceiling of the `max_tile` setting and its
/// default. Read the game's own limit with `Game::max_tile()`.
pub const MAX_TILE_LIMIT: usize = 20;

/// Picks the wizard spends on books and retorts.
pub const PICKS: u8 = 11;

/// Most units in an army: the group that moves, attacks and fights together. A battle side is
/// at most this many too (`battle::casting::MAX_SIDE_UNITS`).
pub const MAX_STACK: usize = 9;

/// Land as a share of each plane's tiles for the original's three sizes.
pub const LAND_SMALL: u8 = 15;
pub const LAND_MEDIUM: u8 = 20;
pub const LAND_LARGE: u8 = 30;

/// The difficulty a game gets when nothing says otherwise ("Average").
pub const DEFAULT_DIFFICULTY: u8 = 2;
/// The map a game gets when nothing says otherwise: the original's 60 by 40 squares.
pub const MAP_WIDTH: u16 = 60;
pub const MAP_HEIGHT: u16 = 40;
/// The sizes the map generator and the rest of the game are tested on: from 45 by 30
/// to 90 by 60 squares.
pub const MAP_WIDTHS: std::ops::RangeInclusive<u16> = 45..=90;
pub const MAP_HEIGHTS: std::ops::RangeInclusive<u16> = 30..=60;
/// The node setting a game gets when nothing says otherwise ("Normal").
pub const DEFAULT_MAGIC_STRENGTH: u8 = 1;

/// The original's options step: difficulty, opponents, land size and magic power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicSettings {
    /// 0 intro, 1 easy, 2 average, 3 hard, 4 extreme, 5 impossible.
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
    /// Towers of wisdom, on both planes at once. `None` follows the area (6 for each 60 by 40
    /// squares, the original's); the options step sets the number of the chosen map size.
    #[serde(default)]
    pub towers: Option<u8>,
    /// The most neutral cities on each plane. `None` follows the area (15 for each 60 by 40
    /// squares, the original's); the options step sets the number of the chosen map size.
    #[serde(default)]
    pub neutral_cities: Option<u8>,
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
            towers: None,
            neutral_cities: None,
            diagonal_extra_cost: false,
            original_map_generator: false,
        }
    }
}

impl BasicSettings {
    /// The towers of wisdom of the map: the setting, or 6 for every 60 by 40 squares.
    pub fn tower_count(&self) -> u8 {
        self.towers.unwrap_or_else(|| {
            scaled_count(
                TOWERS_PER_REFERENCE_AREA,
                self.map_width.into(),
                self.map_height.into(),
            )
            .min(255) as u8
        })
    }

    /// The most neutral cities on each plane: the setting, or 15 for every 60 by 40 squares.
    pub fn neutral_city_count(&self) -> u32 {
        self.neutral_cities.map_or_else(
            || {
                scaled_count(
                    NEUTRAL_CITIES_PER_PLANE,
                    self.map_width.into(),
                    self.map_height.into(),
                )
            },
            u32::from,
        )
    }

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
    /// The turn from which monsters leave their lairs on Average and harder: 30, 40 or 50 (the
    /// original: 50; patch 1.40 sets 40). Intro and Easy stay at 50.
    #[serde(default = "default_monster_start")]
    pub monster_start_turn: u8,
    /// How far each wizard sees around their capital when the game starts, in squares: a step
    /// along a row or column counts 1, a diagonal step 1.5 (1 without `diagonal_extra_cost`;
    /// `map::fog::in_sight`), so 2.5 is a 5 by 5 block, without its corners when a diagonal step
    /// costs extra.
    pub start_reveal_radius: HalfInt,
}

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
    /// Most units on one square, from the nine of an army up to `MAX_TILE_LIMIT`. An army and a
    /// battle side stay at nine; units above that on a defended square are a reserve.
    #[serde(default = "default_max_tile")]
    pub max_tile: usize,
    /// Half the food and gold upkeep for the basic troops (`SettingsDef::half_upkeep` lists
    /// them). A new game takes the content's choice; it is saved with the game.
    #[serde(default)]
    pub half_upkeep: bool,
    /// Patch 1.40 of the original: at most this many arcane spells on offer at once while another
    /// realm has spells to research. 0 is off, as in the original game. A new game takes the
    /// content's choice; it is saved with the game.
    #[serde(default)]
    pub arcane_candidate_cap: u8,
    /// How many land units a Trireme carries, from the original's 2 up to `MAX_SHIP_CAPACITY`
    /// (patch 1.40 sets 8). Copied into the game's own content as the unit's Transport stat.
    #[serde(default = "default_trireme")]
    pub trireme_capacity: u8,
    /// How many land units a Galley carries, from the original's 5 up to `MAX_SHIP_CAPACITY`
    /// (patch 1.40 sets 8).
    #[serde(default = "default_galley")]
    pub galley_capacity: u8,
    /// Patch 1.40 (IW082): in a walled city, a unit anywhere inside the walls gets the +3 defence
    /// the wall sections give, not just the +1 of the original, against attackers outside.
    #[serde(default)]
    pub walls_cover_city: bool,
    /// Patch 1.40 (IW114): an enemy's Suppress Magic does not counter the Spell of Return.
    #[serde(default)]
    pub suppress_spares_return: bool,
}

/// The original's Trireme capacity, and the unit number of the Trireme.
pub const TRIREME_CAPACITY: u8 = 2;
/// The original's Galley capacity.
pub const GALLEY_CAPACITY: u8 = 5;
/// The most a Trireme or Galley may carry.
pub const MAX_SHIP_CAPACITY: u8 = 8;
/// The unit numbers (`units.ron`) of the Trireme and the Galley.
pub const TRIREME_UNIT: u16 = 35;
pub const GALLEY_UNIT: u16 = 36;

/// The original's first turn for monsters leaving their lairs.
pub const MONSTER_START_TURN: u8 = 50;
/// The earliest turn a game may be set to start the monsters.
pub const MONSTER_START_EARLIEST: u8 = 30;

fn default_monster_start() -> u8 {
    MONSTER_START_TURN
}

fn default_trireme() -> u8 {
    TRIREME_CAPACITY
}

fn default_galley() -> u8 {
    GALLEY_CAPACITY
}

fn default_max_tile() -> usize {
    MAX_TILE_LIMIT
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

/// How often the random events fire, as a multiplier on the original's chance.
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

/// One difficulty level of `crates/f4x-def/content/rules_settings.ron`: what choosing it fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelDef {
    pub wizards: WizardSettings,
    pub ai: AiSettings,
    pub world: WorldSettings,
    pub rules: RuleSettings,
    pub events: EventSettings,
    pub scoring: ScoreSettings,
}

/// The rule sets a new game can start from (the Custom screen's dropdown, the soak's `--rules`).
/// Picking one fills in its settings; `Custom` is every setting by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RuleSet {
    /// The original's last official patch: the rules as the original and ReMoM have them, which
    /// `rules_settings.ron` holds for each difficulty level.
    #[default]
    V131,
    /// The community patch's rule changes.
    V160,
    /// Every setting by hand.
    Custom,
}

impl RuleSet {
    pub const ALL: [RuleSet; 3] = [RuleSet::V131, RuleSet::V160, RuleSet::Custom];

    /// `work` with this rule set's rules: the difficulty level's figures for everything but the
    /// basic choices (the map, the opponents), which stay. `Custom` changes nothing.
    pub fn applied(self, work: GameSettings, def: &SettingsDef) -> GameSettings {
        let level = GameSettings::for_difficulty(def, work.basic.difficulty);
        let base = GameSettings {
            basic: BasicSettings {
                diagonal_extra_cost: level.basic.diagonal_extra_cost,
                ..work.basic
            },
            ..level
        };
        match self {
            RuleSet::V131 => base,
            // The patch's changes, as far as they are defined (task 530, from IW018 on still to
            // check): ships carry eight (IW003).
            RuleSet::V160 => GameSettings {
                rules: RuleSettings {
                    trireme_capacity: MAX_SHIP_CAPACITY,
                    galley_capacity: MAX_SHIP_CAPACITY,
                    walls_cover_city: true,
                    suppress_spares_return: true,
                    ..base.rules
                },
                ..base
            },
            RuleSet::Custom => work,
        }
    }
}

/// How many difficulty levels the file has: Intro, Easy, Average, Hard, Extreme, Impossible.
pub const LEVELS: usize = 6;
/// The last level, Impossible.
pub const MAX_DIFFICULTY: u8 = 5;
/// Extreme (patch 1.40), a level between Hard and Impossible. It is a level like any other:
/// choosing it changes no rule set.
pub const EXTREME: u8 = 4;

/// One land size on the new-game options step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandSize {
    /// The size's id (`small`); its words are the text keys `options.land_<id>`.
    pub id: String,
    /// Land as a share of each plane's tiles.
    pub land_percent: u8,
}

/// One map size on the new-game options step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSize {
    /// The size's id (`small`); its words are the text keys `options.map_<id>`.
    pub id: String,
    /// Squares across and down, each plane (see [`MAP_WIDTHS`]).
    pub width: u16,
    pub height: u16,
    /// Towers of wisdom on this size of map (both planes share them).
    pub towers: u8,
    /// The most neutral cities on each plane of this size.
    pub neutral_cities: u8,
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
    /// The units, by their id without the race (`spearmen` for `high_elf_spearmen`), so the
    /// setting does not depend on the language.
    pub units: Vec<String>,
}

impl Default for HalfUpkeep {
    fn default() -> Self {
        Self {
            enabled: false,
            units: vec!["spearmen".into(), "swordsmen".into()],
        }
    }
}

/// `crates/f4x-def/content/rules_settings.ron`: the difficulty levels, the defaults of the new-game choices, the
/// map and land sizes and the spell library. A file without it (the test content) leaves `levels`
/// empty, and a game then plays the default game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsDef {
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
    /// Intro, Easy, Average, Hard, Extreme, Impossible; empty or exactly [`LEVELS`].
    pub levels: Vec<LevelDef>,
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

/// `rules_settings.ron`, `spell_library`: what a new wizard starts with. The tables are indexed by the
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
    pub always_known: Vec<SpellId>,
    /// Spells a retort gives at the start, by the retort's number (`retorts.ron`).
    pub retort_known: std::collections::BTreeMap<Retort, Vec<SpellId>>,
    /// Spells of the arcane realm nobody may research at the start (they come by other ways).
    pub arcane_closed: Vec<SpellId>,
    /// A computer wizard also knows this spell of its main realm (the realm with most books;
    /// the first of the realms in file order on a tie), by realm number. The original's table is
    /// off by one spell from what its code names (`INITGAME.c`, `OGBUG`); these are the spells
    /// it really gives, and the ones the wiki lists.
    pub ai_main_realm_known: std::collections::BTreeMap<RealmId, SpellId>,
    /// From this difficulty on (3 is Hard) a computer wizard may also research this spell of its
    /// main realm, by realm number.
    pub ai_main_realm_researchable: std::collections::BTreeMap<RealmId, SpellId>,
    pub ai_main_realm_researchable_from: u8,
    /// Patch 1.40 of the original: while another realm has spells to research, at most this many
    /// arcane spells are on offer at once (4 in the patch). 0 is off, as in the original game.
    pub arcane_candidate_cap: u8,
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
        const COMMON: &[u8] = &[0, 3, 5, 6, 7, 8, 9, 10, 10, 10, 10, 99];
        const UNCOMMON: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 8, 99, 99, 99, 99];
        const RARE: &[u8] = &[0, 0, 1, 2, 3, 4, 5, 6, 7, 9, 99, 99];
        const VERY_RARE: &[u8] = &[0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 99, 99];
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
            always_known: vec![
                SpellId::new(201), /* magic_spirit */
                SpellId::new(214), /* spell_of_return */
            ],
            retort_known: [(
                Retort::new(17), /* artificer */
                vec![
                    SpellId::new(207), /* enchant_item */
                    SpellId::new(211), /* create_artifact */
                ],
            )]
            .into(),
            arcane_closed: vec![
                SpellId::new(213), /* spell_of_mastery */
                SpellId::new(214), /* spell_of_return */
            ],
            ai_main_realm_known: [
                (RealmId::new(0), SpellId::new(9)),   // nature: sprites
                (RealmId::new(1), SpellId::new(49)),  // sorcery: nagas
                (RealmId::new(2), SpellId::new(84)),  // chaos: hell_hounds
                (RealmId::new(3), SpellId::new(129)), // life: guardian_spirit
                (RealmId::new(4), SpellId::new(166)), // death: ghouls
            ]
            .into(),
            ai_main_realm_researchable: [
                (RealmId::new(0), SpellId::new(20)),  // nature: basilisk
                (RealmId::new(1), SpellId::new(60)),  // sorcery: phantom_beast
                (RealmId::new(2), SpellId::new(100)), // chaos: chimeras
                (RealmId::new(3), SpellId::new(136)), // life: unicorns
                (RealmId::new(4), SpellId::new(180)), // death: shadow_demons
            ]
            .into(),
            ai_main_realm_researchable_from: 3,
            arcane_candidate_cap: 0,
        }
    }
}

impl Default for SettingsDef {
    /// Content without a `rules_settings.ron`: the original's three land sizes and four opponents.
    fn default() -> Self {
        let size = |id: &str, land_percent| LandSize {
            id: id.into(),
            land_percent,
        };
        Self {
            new_game: BasicSettings::default(),
            max_opponents: MAX_OPPONENTS,
            half_upkeep: HalfUpkeep::default(),
            land_sizes: vec![
                size("small", LAND_SMALL),
                size("medium", LAND_MEDIUM),
                size("large", LAND_LARGE),
            ],
            map_sizes: [
                ("small", 45, 30, 3, 8),
                ("medium", 60, 40, 6, 15),
                ("large", 75, 50, 9, 23),
                ("huge", 90, 60, 14, 34),
            ]
            .into_iter()
            .map(|(id, width, height, towers, neutral_cities)| MapSize {
                id: id.into(),
                width,
                height,
                towers,
                neutral_cities,
            })
            .collect(),
            levels: Vec::new(),
            spell_library: SpellLibraryDef::default(),
        }
    }
}

impl SettingsDef {
    /// The level `difficulty` (above Impossible counts as Impossible), if the file has levels.
    pub fn level(&self, difficulty: u8) -> Option<&LevelDef> {
        self.levels.get(usize::from(difficulty.min(MAX_DIFFICULTY)))
    }

    /// Every problem with the file, in words (empty when it is fine): the number of levels, and
    /// each level and the new-game choices within range.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.levels.is_empty() && self.levels.len() != LEVELS {
            problems.push(format!(
                "rules_settings.ron has {} levels, not {LEVELS}",
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
            if size.id.trim().is_empty() || !(1..=99).contains(&size.land_percent) {
                problems.push(format!(
                    "land size {:?}: {}% is not 1 to 99 or the id is empty",
                    size.id, size.land_percent
                ));
            }
        }
        if self.map_sizes.is_empty() {
            problems.push("map_sizes is empty".into());
        }
        for size in &self.map_sizes {
            if size.id.trim().is_empty()
                || !MAP_WIDTHS.contains(&size.width)
                || !MAP_HEIGHTS.contains(&size.height)
            {
                problems.push(format!(
                    "map size {:?}: {} by {} is not {:?} by {:?} or the id is empty",
                    size.id, size.width, size.height, MAP_WIDTHS, MAP_HEIGHTS
                ));
            }
        }
        let choices = GameSettings {
            basic: self.new_game,
            ..GameSettings::default()
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
    /// choices at their defaults. Without levels in the file it is the default game.
    pub fn for_difficulty(def: &SettingsDef, difficulty: u8) -> Self {
        let d = difficulty.min(MAX_DIFFICULTY);
        let Some(level) = def.level(d) else {
            let mut game = Self {
                basic: BasicSettings {
                    difficulty: d,
                    ..def.new_game
                },
                ..Self::default()
            };
            game.rules.half_upkeep = def.half_upkeep.enabled;
            game.rules.arcane_candidate_cap = def.spell_library.arcane_candidate_cap;
            return game;
        };
        Self {
            basic: BasicSettings {
                difficulty: d,
                ..def.new_game
            },
            wizards: level.wizards,
            ai: level.ai,
            world: level.world,
            rules: RuleSettings {
                half_upkeep: def.half_upkeep.enabled,
                arcane_candidate_cap: def.spell_library.arcane_candidate_cap,
                ..level.rules
            },
            events: level.events,
            scoring: level.scoring,
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
            b.difficulty <= MAX_DIFFICULTY,
            format!("difficulty {} is not 0 to {MAX_DIFFICULTY}", b.difficulty),
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
        check(
            (MAX_STACK..=MAX_TILE_LIMIT).contains(&self.rules.max_tile),
            format!(
                "{} units on a square is not {} to {}",
                self.rules.max_tile, MAX_STACK, MAX_TILE_LIMIT
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
        let radius = self.world.start_reveal_radius;
        check(
            (HalfInt::ZERO..=HalfInt::from_int(10)).contains(&radius),
            format!("start reveal radius {radius} is not 0 to 10"),
        );
        check(
            self.rules.arcane_candidate_cap <= 20,
            format!(
                "arcane spell cap {} is over 20",
                self.rules.arcane_candidate_cap
            ),
        );
        check(
            (MONSTER_START_EARLIEST..=MONSTER_START_TURN).contains(&self.world.monster_start_turn),
            format!(
                "monster start turn {} is not {MONSTER_START_EARLIEST} to {MONSTER_START_TURN}",
                self.world.monster_start_turn
            ),
        );
        check(
            (TRIREME_CAPACITY..=MAX_SHIP_CAPACITY).contains(&self.rules.trireme_capacity),
            format!(
                "trireme capacity {} is not {TRIREME_CAPACITY} to {MAX_SHIP_CAPACITY}",
                self.rules.trireme_capacity
            ),
        );
        check(
            (GALLEY_CAPACITY..=MAX_SHIP_CAPACITY).contains(&self.rules.galley_capacity),
            format!(
                "galley capacity {} is not {GALLEY_CAPACITY} to {MAX_SHIP_CAPACITY}",
                self.rules.galley_capacity
            ),
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
        b.difficulty = b.difficulty.min(MAX_DIFFICULTY);
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
        self.world.start_reveal_radius = self
            .world
            .start_reveal_radius
            .clamp(HalfInt::ZERO, HalfInt::from_int(10));
        self.rules.arcane_candidate_cap = self.rules.arcane_candidate_cap.min(20);
        self.world.monster_start_turn = self
            .world
            .monster_start_turn
            .clamp(MONSTER_START_EARLIEST, MONSTER_START_TURN);
        self.rules.trireme_capacity = self
            .rules
            .trireme_capacity
            .clamp(TRIREME_CAPACITY, MAX_SHIP_CAPACITY);
        self.rules.galley_capacity = self
            .rules
            .galley_capacity
            .clamp(GALLEY_CAPACITY, MAX_SHIP_CAPACITY);
        self.events.pressure_percent = self.events.pressure_percent.clamp(10, 200);
        self.scoring.score_percent = self.scoring.score_percent.clamp(10, 500);
        self
    }
}

#[cfg(test)]
mod tests;

/// The game as the shipped rules play it: the Average level of `crates/f4x-def/content/rules_settings.ron`
/// (read at build time, so there is one set of figures). A game made for play picks its level in
/// `game::setup::new_game`; a bare `Game::new` and a `NewGame::default()` start from this.
impl Default for GameSettings {
    fn default() -> Self {
        static AVERAGE: std::sync::OnceLock<GameSettings> = std::sync::OnceLock::new();
        *AVERAGE.get_or_init(|| {
            let def: SettingsDef = ron::from_str(include_str!("../content/rules_settings.ron"))
                .expect("crates/f4x-def/content/rules_settings.ron parses");
            GameSettings::for_difficulty(&def, 2)
        })
    }
}
