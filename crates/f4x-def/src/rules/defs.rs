//! Definition types: static content describing races, units, buildings, spells and
//! so on. Runtime state (damage taken, remaining duration) lives in ECS components.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::damage_type::DamageType;
use super::effects::{Condition, Effect, EffectRule, Trigger};
use super::ids::Id;
use super::registry::{Def, Registry};
use super::stats::{ModOp, Modifier, Source, Stat};
use super::{Ability, BuildingId, RealmId, SpellId};
use crate::half_int::HalfInt;
use crate::map::TerrainClass;

/// A number for each plane.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneWeight {
    pub arcanus: u32,
    pub myrror: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaceDef {
    /// The race number, the id the game passes around (and the row of the racial unrest matrix).
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The race's townsfolk topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    /// Units cities of this race can build.
    pub units: Vec<Id<UnitDef>>,
    /// The two units a new wizard's capital starts with: the race's spearmen and its swordsmen
    /// (the swordsmen twice where the race has no spearmen).
    #[serde(default)]
    pub start_army: Vec<Id<UnitDef>>,
    /// Buildings cities of this race can build (besides race-neutral ones).
    pub buildings: Vec<Id<BuildingDef>>,
    /// Terrain the race's cities prefer; used by AI and map placement.
    #[serde(default)]
    pub home_terrain: Vec<TerrainClass>,
    /// How often the race is rolled for an independent city on each plane when the setting
    /// `mix_neutral_races` is on (every race has the same total across the two planes).
    #[serde(default)]
    pub neutral_weight: PlaneWeight,
    /// Applied to every city of the race.
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Food each farmer of the race grows; absent is the base of the settings (`farmer_food`).
    #[serde(default)]
    pub farmer_food: Option<HalfInt>,
    /// Production per worker of the race, as an amount (2 is two, 1.5 one and a half); absent is
    /// the base of the settings (`worker_production`).
    #[serde(default)]
    pub worker_production: Option<HalfInt>,
    /// Mana per citizen, as an amount (1 is one per citizen, 0.5 half).
    #[serde(default)]
    pub mana_per_citizen: HalfInt,
    /// Doubles taxes and the yield of mines.
    #[serde(default)]
    pub double_mining: bool,
    /// Extra percent of gold from trade.
    #[serde(default)]
    pub trade_percent: i32,
    /// Base chance (percent) per turn that an outpost of the race grows.
    #[serde(default = "ten")]
    pub outpost_growth: u8,
    /// Houses in the city picture: 0 standard, 1 tree houses, 2 mounds.
    #[serde(default)]
    pub house_type: u8,
    /// Neutral cities of this race stand on Myrror; the others on Arcanus.
    #[serde(default)]
    pub myrror: bool,
}

fn yes() -> bool {
    true
}
fn ten() -> u8 {
    10
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitKind {
    /// Trained by a city of its race.
    Normal,
    /// Hired or summoned individual with items and levels.
    Hero,
    /// Conjured by a spell; costs mana upkeep.
    Summoned,
    /// Neutral or realm creature that is not built by a race.
    Fantastic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementType {
    Walking,
    Flying,
    Swimming,
    Sailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upkeep {
    #[serde(default)]
    pub gold: HalfInt,
    /// Food a turn: 1 is one food, 0.5 half a food, 1.5 one and a half, 0 eats nothing.
    #[serde(default)]
    pub food: HalfInt,
    #[serde(default)]
    pub mana: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemSlot {
    Weapon,
    Armor,
    Misc,
}

/// The kind of a hero, which decides what its three item slots take (`Hero_Slot_Types`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeroKind {
    /// Sword, armour, miscellaneous.
    Warrior,
    /// Bow, armour, miscellaneous.
    Bow,
    /// Sword or staff, armour, miscellaneous.
    SwordOrStaff,
    /// Staff, miscellaneous, miscellaneous.
    Mage,
}

/// What a hero's item slot takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlotKind {
    /// Sword, mace or axe.
    Sword,
    /// A bow, or what the sword slot takes.
    Bow,
    /// What the sword slot takes, or a staff or wand.
    SwordOrStaff,
    /// Staff or wand.
    Staff,
    /// Shields and mail.
    Armor,
    Misc,
}

impl HeroKind {
    /// The kinds of the hero's three slots.
    pub fn slots(self) -> [SlotKind; 3] {
        match self {
            Self::Warrior => [SlotKind::Sword, SlotKind::Armor, SlotKind::Misc],
            Self::Bow => [SlotKind::Bow, SlotKind::Armor, SlotKind::Misc],
            Self::SwordOrStaff => [SlotKind::SwordOrStaff, SlotKind::Armor, SlotKind::Misc],
            Self::Mage => [SlotKind::Staff, SlotKind::Misc, SlotKind::Misc],
        }
    }
}

impl SlotKind {
    /// Whether an item of this kind may go in the slot (`ItemScrn.c`, `Move_Item`): an item
    /// fits the slot of its own type; the bow slot also takes the sword slot's items and the
    /// sword-or-staff slot those and the staff slot's.
    pub fn fits(self, item: ItemKind) -> bool {
        use ItemKind as K;
        let sword = matches!(item, K::Sword | K::Mace | K::Axe);
        let staff = matches!(item, K::Staff | K::Wand);
        match self {
            Self::Sword => sword,
            Self::Bow => sword || item == K::Bow,
            Self::SwordOrStaff => sword || staff,
            Self::Staff => staff,
            Self::Armor => matches!(item, K::Shield | K::Chainmail | K::Platemail),
            Self::Misc => item == K::Misc,
        }
    }
}

/// Extra data for [`UnitKind::Hero`] units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroDef {
    /// Decides the hero's three item slots.
    pub kind: HeroKind,
    /// Gold to hire.
    pub hire_cost: u32,
}

impl HeroDef {
    /// The kinds of the hero's three item slots.
    pub fn slots(&self) -> [SlotKind; 3] {
        self.kind.slots()
    }
}

/// Which abilities a hero's random picks may draw (`Init_Heroes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickType {
    Warrior,
    Mage,
    /// Both the warrior and the mage abilities.
    Any,
}

/// What a unit is worth in a fight, in High Men spearmen: the unit combat rating, the fit of
/// `f4x-tools duel ladder` over real battles (task 655, `docs/duel.md`; `unit_ratings.ron`). A
/// recruit High Men spearman is 1, and two units of 50 are worth one of 100: a rough guide to
/// trades, army size and who would win, not a promise (rock-paper-scissors cases of immunities and
/// special attacks overturn it, and spells, terrain and walls swing a real battle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRatings {
    /// What a hero is worth beyond its table row: the rows were fought without the abilities,
    /// spells and items the game rolls for a hero, so a hero's rating is its row times this.
    #[serde(default = "one")]
    pub hero_factor: f32,
    /// What one spearman is in the numbers the AI's thresholds were tuned to (the strength the
    /// formula of `battle::power` gave a unit): the median of that formula over the ratings, so
    /// that a typical unit weighs about what it did and the play starts where it was.
    #[serde(default = "one")]
    pub power_scale: f32,
    /// One row per unit, in the order of the unit numbers.
    #[serde(default)]
    pub units: Vec<UnitRating>,
}

fn one() -> f32 {
    1.0
}

impl Default for UnitRatings {
    /// No rows, and a hero worth its row.
    fn default() -> Self {
        UnitRatings {
            hero_factor: 1.0,
            power_scale: 1.0,
            units: Vec::new(),
        }
    }
}

/// The rating of one unit at its experience levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRating {
    /// The unit's number.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The worth at experience level 1, 2, ... (a unit that does not level up has one number); a
    /// level beyond the last number takes the last.
    pub levels: Vec<f32>,
}

impl UnitRatings {
    /// The row of the unit with this number.
    pub fn row(&self, num: u16) -> Option<&UnitRating> {
        self.units
            .binary_search_by_key(&num, |r| r.num)
            .ok()
            .map(|i| &self.units[i])
    }
}

/// What a unit's rating buys per resource: the unit combat rating over what the unit costs to
/// have (`unit_efficiency.ron`, made from `unit_ratings.ron` and the unit rows by `f4x-tools duel
/// ladder --efficiency FILE`): for choosing garrisons, what to build, what to hire. A rough guide
/// like the ratings it is made from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitEfficiencies {
    /// The turns of upkeep that count beside the base cost (100).
    #[serde(default = "hundred")]
    pub upkeep_turns: u32,
    /// One row per unit with a rating and a cost, in the order of the unit numbers.
    #[serde(default)]
    pub units: Vec<UnitEfficiency>,
}

fn hundred() -> u32 {
    100
}

impl Default for UnitEfficiencies {
    fn default() -> Self {
        UnitEfficiencies {
            upkeep_turns: 100,
            units: Vec::new(),
        }
    }
}

/// The efficiency of one unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitEfficiency {
    /// The unit's number.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The rating, averaged over the levels that were measured (a creature's is flat), a hero's
    /// with its factor.
    pub rating: f32,
    /// The base cost plus `upkeep_turns` turns of upkeep, gold, food and mana counting alike.
    pub cost: f32,
    /// Rating per 100 resources of cost.
    pub per_100: f32,
    /// `per_100` over that of a High Men spearman (1 is as good a buy as a spearman).
    pub relative: f32,
}

impl UnitEfficiencies {
    /// The row of the unit with this number.
    pub fn row(&self, num: u16) -> Option<&UnitEfficiency> {
        self.units
            .binary_search_by_key(&num, |r| r.num)
            .ok()
            .map(|i| &self.units[i])
    }
}

/// How a hero type starts in each wizard's game (`NewGame.c:_hero_premade_table`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroTemplate {
    /// The hero's unit number.
    pub unit: super::UnitId,
    /// Abilities drawn at random for each wizard at the start of the game.
    pub picks: u32,
    pub pick_type: PickType,
    /// Abilities every wizard's copy has: hero abilities of `abilities.ron` (the ones
    /// [`HeroAbilities::ids`](crate::hero::HeroAbilities::ids) can give).
    pub abilities: Vec<Id<AbilityDef>>,
    pub casting_skill: u32,
    /// The innate spells (ids in `spells.ron`), at most four.
    pub spells: Vec<Id<SpellDef>>,
    /// For the messages that name the hero (`docs/text.md`).
    #[serde(default)]
    pub gender: crate::text::Gender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDef {
    /// The unit type number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    pub kind: UnitKind,
    /// Whether a mercenary company can be drawn from this unit. The original's
    /// `Generate_Mercenaries` leaves out the Draconian engineers, which fly and need no roads.
    #[serde(default = "yes")]
    pub hireable: bool,
    /// Required for `Normal` units; heroes and creatures have none.
    #[serde(default)]
    pub race: Option<Id<RaceDef>>,
    /// Required for `Summoned` units.
    #[serde(default)]
    pub realm: Option<Id<RealmDef>>,
    /// Production cost (gold-equivalent) or summoning mana cost.
    pub cost: u32,
    #[serde(default)]
    pub upkeep: Upkeep,
    pub stats: BTreeMap<Stat, HalfInt>,
    pub movement: MovementType,
    #[serde(default)]
    pub abilities: Vec<Ability>,
    /// Buildings the producing city must have.
    #[serde(default)]
    pub requires: Vec<BuildingId>,
    #[serde(default)]
    pub level_path: Option<Id<LevelPathDef>>,
    #[serde(default)]
    pub hero: Option<HeroDef>,
    /// A ship's deck in a sea battle: `(width, length)` in squares, pointed at both ends. The
    /// battle field is a diamond of about 200 squares, so two facing decks need
    /// `width + width + 1 + length <= 20`.
    #[serde(default)]
    pub deck: Option<(u8, u8)>,
}

impl UnitDef {
    /// Whether the unit's normal attacks count as magical by what it is: heroes, fantastic and
    /// summoned creatures, and anything with the `magical_weapon` ability. Ships and catapults are
    /// `Fantastic` in the data, as in the original, but they are not creatures: their blows and
    /// boulders are stopped by weapon immunity (the Unofficial Patch fixes the original, where they
    /// were not). What an item, a weapon quality or a spell adds comes on top.
    pub fn innate_magical_weapon(&self) -> bool {
        (self.kind != UnitKind::Normal
            && !(self.kind == UnitKind::Fantastic
                && (self.movement == MovementType::Sailing
                    || self
                        .abilities
                        .contains(&Ability::new(70 /* wall_crusher */)))))
            || self
                .abilities
                .contains(&Ability::new(41 /* magical_weapon */))
    }

    /// The unit's base value of a stat as a number (0 for a stat it has none of).
    pub fn stat(&self, stat: Stat) -> f32 {
        self.stats.get(&stat).map_or(0.0, |v| v.as_f32())
    }

    /// Whether the unit can found a city.
    pub fn is_settler(&self) -> bool {
        self.abilities.contains(&Id::new(12 /* create_outpost */))
    }

    /// Whether the unit is a settler, an engineer or a melder (a mana spirit): the units that
    /// work and never defend a city.
    pub fn is_worker(&self) -> bool {
        self.is_settler()
            || self.stat(Stat::Construction) > 0.0
            || self.abilities.contains(&Id::new(42 /* meld */))
    }

    /// Whether the unit fights on land: it has an attack and is neither a settler, an engineer
    /// (it walks off to build roads), a ship nor a melder (a spirit that goes to a node; it fights only when a wizard is desperate for the little extra power to clear one early, which the node logic decides). The one
    /// definition of a soldier; what kind it is (normal, hero, fantastic) does not matter here,
    /// where it may stand is another question (only normal units and heroes keep a city quiet).
    pub fn is_fighter(&self) -> bool {
        self.stat(Stat::Melee).max(self.stat(Stat::Ranged)) > 0.0
            && !self.is_worker()
            && self.movement != MovementType::Sailing
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityDef {
    /// The ability's number, the id the game passes around: counted from 1 and frozen.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    #[serde(default)]
    pub description: String,
    /// The ability's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    /// What a hero ability gives per level (`abilities.ron`); none for any other ability.
    #[serde(default)]
    pub per_level: Option<PerLevel>,
    /// What the ability does against damage types: immunities and defence floors.
    #[serde(default)]
    pub defence: Vec<super::damage_type::DefenceRule>,
    /// The damage the ability adds to its unit's attack.
    #[serde(default)]
    pub attack: Option<super::damage_type::AbilityAttack>,
    /// The mana a unit with the ability casts spells with in a battle (a spell caster 20, a major
    /// one 40); 0 for an ability that does not make a caster. A caster also shoots its spell
    /// shots with it.
    #[serde(default)]
    pub caster_mana: i32,
    /// How many times a unit with the ability may summon a demon in a battle (Summon Demon: 3).
    #[serde(default)]
    pub summons_per_battle: u32,
    /// What the ability sets on its unit in a battle.
    #[serde(default)]
    pub flags: Vec<super::unit_flag::UnitFlag>,
    pub effects: Vec<EffectRule>,
}

/// `x * numerator / denominator` with `x = level + level_offset`, rounded down: what a hero
/// ability gives a hero of that level (0 for the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerLevel {
    pub numerator: i32,
    pub denominator: i32,
    pub level_offset: i32,
}

impl PerLevel {
    pub fn at(self, level: usize) -> i32 {
        (level as i32 + self.level_offset) * self.numerator / self.denominator.max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingDef {
    /// The building number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The building's topic in `help.ron` (an index): its description in the city builder and its
    /// help pop-up.
    #[serde(default)]
    pub help: Option<u16>,
    pub cost: u32,
    #[serde(default)]
    pub upkeep_gold: u32,
    #[serde(default)]
    pub requires: Vec<BuildingId>,
    /// Building made redundant by this one. It still counts as built (for requirements
    /// and effects) but costs no upkeep.
    #[serde(default)]
    pub replaces: Option<BuildingId>,
    /// Every race that can build it, complete: a race that cannot build one of the buildings it
    /// `requires` is not listed. An empty list is a building no race builds.
    #[serde(default)]
    pub races: Vec<Id<RaceDef>>,
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Footprint in the city picture: 0 is 2x2, 1 is 2x3, 2 is 3x2, 3 is 3x3.
    #[serde(default)]
    pub shape: u8,
    /// Only a coastal city (one next to the sea) can build it.
    #[serde(default)]
    pub coastal: bool,
}

/// A school of magic. Data, so the set of realms is not hard-coded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealmDef {
    /// The realm number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The realm's spells topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    pub color: (u8, u8, u8),
    /// Spells (ids) a wizard with books in this realm starts with when they choose none, in
    /// the order they are taken: common ones first.
    #[serde(default)]
    pub default_spells: Vec<Id<SpellDef>>,
}

/// The five realms that have spell books, in the original's order (`sbr_Nature` to `sbr_Death`);
/// arcane has none.
pub const BOOK_REALMS: &[RealmId] = &[
    RealmId::new(0), // nature
    RealmId::new(1), // sorcery
    RealmId::new(2), // chaos
    RealmId::new(3), // life
    RealmId::new(4), // death
];

/// What the books must be before a retort may be picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetortNeed {
    /// At least this many books in the realm (by alias).
    Books(Id<RealmDef>, u8),
    /// At least `count` realms with this many books or more each.
    Realms(u8, u8),
}

/// A retort: a special ability of a wizard, picked at the creation screen or won as loot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetortDef {
    /// The retort's place in the wizard record, the id the game passes around and the saved game
    /// writes.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The retort's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    /// Picks it takes at the creation screen.
    pub cost: u8,
    /// What the books must be before it may be picked.
    #[serde(default)]
    pub requires: Vec<RetortNeed>,
    /// What the retort does. Only passive `Modify` rules count: the wizard's stats
    /// (`SpellCostPercent`, `ResearchIncome` as a percent) with a condition on the spell
    /// (`RealmIs`, `SpellIs`).
    #[serde(default)]
    pub effects: Vec<super::effects::EffectRule>,
}

impl RetortDef {
    /// Do these books (per realm) meet every need?
    pub fn available(&self, books: &std::collections::BTreeMap<RealmId, u8>) -> bool {
        self.requires.iter().all(|need| match need {
            RetortNeed::Books(realm, min) => books.get(realm).copied().unwrap_or(0) >= *min,
            RetortNeed::Realms(count, min) => {
                books.values().filter(|&&n| n >= *min).count() >= usize::from(*count)
            }
        })
    }
}

/// A ready-made wizard for the creation screen: a book and retort loadout, and the name and
/// description that the local texts file fills in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WizardDef {
    /// The wizard's place in the list, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The wizard's topic in `help.ron` (an index).
    pub help: u16,
    #[serde(default)]
    pub gender: crate::text::Gender,
    /// Spell books as `(realm id, count)`.
    pub books: Vec<(Id<RealmDef>, u8)>,
    /// The retorts the wizard starts with.
    pub retorts: Vec<Id<RetortDef>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    VeryRare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastContext {
    Overland,
    Battle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellTarget {
    None,
    OwnUnit,
    EnemyUnit,
    AnyUnit,
    OwnCity,
    EnemyCity,
    Tile,
    Battlefield,
    Wizard,
    /// Another wizard, by clicking one of their cities or units (Spell Blast).
    EnemyWizard,
    /// One of another wizard's global enchantments (Disjunction, Spell Binding).
    GlobalEnchantment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    Instant,
    Permanent,
}

/// Which units a spell on a unit may be aimed at, beyond its side (`SpellDef::unit_only`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitOnly {
    /// Any unit.
    #[default]
    Any,
    /// A normal unit or a hero, not a fantastic creature and not undead. A unit that is several
    /// things at once, as Torin is a hero and a Life creature, is a valid target when any one of
    /// them is.
    NormalOrHero,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellDef {
    /// The original's spell number (`SPELLDAT.LBX` record), the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// The spell's topic in `help.ron` (an index).
    #[serde(default)]
    pub help: Option<u16>,
    pub realm: Id<RealmDef>,
    pub rarity: Rarity,
    /// Mana to cast.
    pub cost: u32,
    pub research_cost: u32,
    pub contexts: Vec<CastContext>,
    /// A spell that can also be cast in battle costs five times as much on the map (a spell sold
    /// only for the map, or only for battle, is not dearer).
    #[serde(default)]
    pub overland_x5: bool,
    pub target: SpellTarget,
    pub duration: Duration,
    /// Mana per turn to keep a lasting spell going.
    #[serde(default)]
    pub upkeep: u32,
    /// Strength a dispel must beat.
    #[serde(default)]
    pub dispel_difficulty: u32,
    /// A curse the target can resist when it is cast in a battle.
    #[serde(default)]
    pub save: Option<SpellSave>,
    /// What a spell that harms units without an attack of its own (a battlefield curse, Life
    /// Drain) harms them with: a unit with a defence rule that is immune to one of these types
    /// is not harmed (Wrack, Terror and Black Prayer are death, so death immunity stops them).
    #[serde(default)]
    pub harm_types: super::damage_type::DamageTypes,
    /// The original game's version of this spell ignores the immunity to its death type (Word of
    /// Death, Black Prayer, Terror): the rule setting `death_immunity_exceptions` brings that
    /// back, and without it the spell respects the immunity like every other spell of its kind.
    #[serde(default)]
    pub death_immunity_exception: bool,
    /// The same for magic immunity (the battlefield enchantments and curses the original lets
    /// through): the rule setting `magic_immunity_exceptions` brings that back.
    #[serde(default)]
    pub magic_immunity_exception: bool,
    /// Spells this one is not cumulative with. It cannot be cast on a unit that has one of them
    /// (or, for a global enchantment, on a wizard who keeps it), and while the unit has one its
    /// own bonuses do not count: Stone Skin gives way to Iron Skin.
    #[serde(default)]
    pub not_with: Vec<Id<SpellDef>>,
    /// Abilities a unit has of its own that make this spell pointless: it cannot be cast on a
    /// unit that has one (a unit that flies by nature cannot be given Wind Walking).
    #[serde(default)]
    pub already_able: Vec<Id<AbilityDef>>,
    /// The kinds of unit the spell may be aimed at.
    #[serde(default)]
    pub unit_only: UnitOnly,
    /// What the spell does against some attacks to the unit it lasts on (Elemental Armor, Bless,
    /// Righteousness): the same rules as an ability's `defence`.
    #[serde(default)]
    pub defence: Vec<super::damage_type::DefenceRule>,
    pub effects: Vec<EffectRule>,
}

/// The resistance check against a lasting spell that harms its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellSave {
    /// Added to the target's resistance (negative: harder to resist).
    pub modifier: i32,
    /// A target immune to one of these damage types cannot be affected at all (a curse of
    /// Sorcery is of the illusion type, one of Death of the death type).
    #[serde(default)]
    pub types: Vec<DamageType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    /// The item number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    pub slot: ItemSlot,
    /// The item type; it fixes the slot (see [`ItemKind::slot`]).
    pub kind: ItemKind,
    pub cost: u32,
    /// The plain bonuses: attack, to hit, defence, movement, resistance.
    pub effects: Vec<EffectRule>,
    /// Added to the hero's mana in battle.
    #[serde(default)]
    pub spell_skill: i32,
    /// The size of the resistance penalty put on what the hero's spells hit.
    #[serde(default)]
    pub spell_save: i32,
    /// Special powers (ids in `item_powers.ron`).
    #[serde(default)]
    pub powers: Vec<Id<ItemPowerDef>>,
    /// The spell the hero can cast from the item, and how many times (Spell Charges).
    #[serde(default)]
    pub charges: Option<(Id<SpellDef>, u32)>,
    /// The original's flag byte: set items are left out of the Enchant Item grade, unset ones out
    /// of the Create Artifact grade (`Make_Item`).
    #[serde(default)]
    pub flag: bool,
}

/// The ten item types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Sword,
    Mace,
    Axe,
    Bow,
    Staff,
    Wand,
    Misc,
    Shield,
    Chainmail,
    Platemail,
}

impl ItemKind {
    /// The hero slot the kind goes in.
    pub fn slot(self) -> ItemSlot {
        match self {
            Self::Sword | Self::Mace | Self::Axe | Self::Bow | Self::Staff | Self::Wand => {
                ItemSlot::Weapon
            }
            Self::Misc => ItemSlot::Misc,
            Self::Shield | Self::Chainmail | Self::Platemail => ItemSlot::Armor,
        }
    }

    /// The pictures of `ITEMS.LBX` that belong to the kind, first and last (the crafting
    /// screen's arrows step through them; `docs/heroes-items-design.md` section 6).
    pub fn icon_range(self) -> (u16, u16) {
        match self {
            Self::Sword => (0, 8),
            Self::Mace => (9, 19),
            Self::Axe => (20, 28),
            Self::Bow => (29, 37),
            Self::Staff => (38, 46),
            Self::Chainmail => (47, 54),
            Self::Platemail => (55, 61),
            Self::Shield => (62, 71),
            Self::Misc => (72, 106),
            Self::Wand => (107, 115),
        }
    }
}

/// One entry of the item power table (`ITEMPOW.LBX`): a plain bonus or a special power an item
/// can carry, with what it costs to put on one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPowerDef {
    /// The item power number, the id the game passes around.
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    /// At most one power of a group on an item.
    pub group: u8,
    /// Mana cost of adding the power to an item.
    pub cost: u32,
    /// The item types that may carry it.
    pub kinds: Vec<ItemKind>,
    /// Special powers: the realm and the number of books a wizard needs to add it.
    #[serde(default)]
    pub realm: Option<Id<RealmDef>>,
    #[serde(default)]
    pub books: u32,
    /// Plain bonuses: what it adds (movement in whole moves; spell save is the size of the penalty).
    #[serde(default)]
    pub bonus: Option<(PowerBonus, i32)>,
    /// The lasting spell (an id of `spells.ron`) the power puts on the hero that carries the item.
    #[serde(default)]
    pub spell: Option<Id<SpellDef>>,
    /// The ability the power gives the carrier: a special attack, or Merging.
    #[serde(default)]
    pub attack: Option<Id<AbilityDef>>,
    /// Added to the carrier's melee attack on top of the item's own attack bonus (Flaming 3,
    /// Giant Strength 1).
    #[serde(default)]
    pub melee_bonus: i32,
    /// What the bonus dice of `melee_bonus` and `ranged_bonus` are made of (Flaming: fire).
    #[serde(default)]
    pub bonus_types: Vec<DamageType>,
    /// Added to the carrier's ranged attack on top of the item's own attack bonus, for an item of
    /// one of the `ranged_kinds` (any item that counts for the ranged attack when that is empty).
    #[serde(default)]
    pub ranged_bonus: i32,
    #[serde(default)]
    pub ranged_kinds: Vec<ItemKind>,
    /// The power that puts a spell with charges in the item (a design names the spell and the
    /// charges instead of listing it).
    #[serde(default)]
    pub holds_spell: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerBonus {
    Attack,
    ToHit,
    Defense,
    Moves,
    Resistance,
    SpellSkill,
    SpellSave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelDef {
    /// Experience needed to reach this level.
    pub xp: u32,
    pub effects: Vec<EffectRule>,
}

/// Experience thresholds and what each level grants (units and heroes share the mechanism).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelPathDef {
    /// The level path's number (ours: counted from 1 in the alphabetical order of the ids).
    pub num: u16,
    /// The alias for human editing.
    pub id: String,
    pub levels: Vec<LevelDef>,
    /// Levels above the last one experience reaches: only Warlord, Crusade and the like lift a
    /// unit into them (Ultra-Elite and Champion for normal units).
    #[serde(default)]
    pub bonus: Vec<BonusLevelDef>,
}

/// A level that experience alone never gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BonusLevelDef {
    pub effects: Vec<EffectRule>,
}

impl LevelPathDef {
    /// Levels in all, those experience gives and the bonus levels.
    pub fn level_count(&self) -> usize {
        self.levels.len() + self.bonus.len()
    }

    /// The effects of level `index` (0-based), bonus levels last.
    pub fn level(&self, index: usize) -> Option<&[EffectRule]> {
        match self.levels.get(index) {
            Some(l) => Some(&l.effects),
            None => self
                .bonus
                .get(index - self.levels.len())
                .map(|b| &b.effects[..]),
        }
    }
}

/// Tunable numbers that are not content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct RulesConfig {
    /// Base chance (percent) for each figure's attack to hit.
    pub to_hit_percent: u8,
    /// Chance (percent) for each point of defense to block a hit.
    pub block_percent: u8,
    /// Unrest percent caused by each tax rate; the rate is also half the gold per citizen.
    pub tax_unrest_percent: Vec<u8>,
    /// Unrest (in tens of percent) in a city of one race under a capital of another:
    /// `racial_unrest[capital race][city race]`, indexed by race number. Empty means none.
    pub racial_unrest: Vec<Vec<i8>>,
    /// Building ids in the order the Grand Vizier picks them (our own list).
    pub autobuild_order: Vec<Id<BuildingDef>>,
    /// What a spell cast in battle costs the further it is from the wizard's fortress.
    pub casting_range: CastingRange,
    /// What wizards ask for goods in a trade.
    pub trade: TradeRules,
    /// How the combat forecast words how the sides compare.
    pub forecast: ForecastRules,
    /// Relations, treaties and negotiations.
    pub diplomacy: DiplomacyRules,
    /// The offers of mercenaries and heroes.
    pub offer: OfferRules,
    /// The growth of an outpost.
    pub outpost: OutpostRules,
    /// Raiders, monsters and the big events of the world.
    pub threat: ThreatRules,
    /// The fewest defenders a city wants: `(from figures, defenders)` pairs in rising order of
    /// figures; a city below the first has none.
    pub garrison_minimum: Vec<(u32, u32)>,
    /// Where a city's size class starts: the citizens at which class 1, class 2 and so on begin, in
    /// rising order; a city below the first (an outpost) is class 0, so a city's class is the number
    /// of entries it has reached. The words for the classes are in the text files (`city_size.*`).
    pub city_size_from: Vec<u32>,
    /// What a neutral city's population gives it: the building comes with `population - 2` at
    /// or above the number (a building its race cannot build is left out).
    pub starting_buildings: Vec<(u32, Id<BuildingDef>)>,
    /// The Arcanus races (race numbers) a computer wizard may have above Normal difficulty when
    /// `RuleSettings::limit_ai_races` is on.
    pub limited_ai_races: Vec<u16>,
    /// The minerals (feature numbers) a world event can put on the map.
    pub event_ores: Vec<u16>,
    /// The wizard: the start, the costs of spells, the limits.
    pub wizard: WizardRules,
    /// Making, holding and moving magic items.
    pub item: ItemRules,
    /// The abilities the rules treat as a group, by number.
    pub abilities: AbilityRules,
    /// The numbers of the overland spells and curses.
    pub spell_effects: SpellEffectRules,
    /// Experience, fame and the level Heroism gives.
    pub renown: RenownRules,
    /// The rows of the content that a setting of the game changes.
    pub targets: SettingTargets,
}

/// The rules of the battle (`battle.ron`): strengths of the attacks the abilities and fields make,
/// the defence they give, what a spell shot or a mana leak costs, the limits, the scenery of the field
/// and what the walls of a city do. The battle rules stand by themselves: nothing of them is in
/// `rules.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BattleRules {
    /// The strength of the fire attack Immolation adds to a melee attack.
    pub immolation_strength: i32,
    /// The blows Invulnerability stops on top of the defence roll.
    pub invulnerability_block: i32,
    /// Mana a Mana Leak takes from a caster each turn.
    pub mana_leak_drain: i32,
    /// The defence a unit gains against an attack from outside a walled city: on a standing
    /// wall, and elsewhere in the city (`wall_defence` everywhere with `walls_cover_city`).
    pub wall_defence: i32,
    pub city_defence: i32,
    /// The strength of Call Lightning's bolt, and of a Wall of Fire's burn.
    pub call_lightning_strength: i32,
    pub wall_of_fire_strength: i32,
    /// The strength of the dispel a magic node's realm sets against the spells of other realms
    /// (percent chance by the dispel formula), and the bonus its aura gives its creatures.
    pub node_counter_strength: u32,
    pub node_aura_bonus: i32,
    /// What each enemy spell takes from a Counter Magic reserve.
    pub counter_magic_drain: u32,
    /// The mana one spell shot costs a caster.
    pub shot_mana: i32,
    /// The hit points of the web that holds a unit.
    pub web_hits: i32,
    /// A battle is a draw after this many turns.
    pub turn_limit: u32,
    /// The least and the most mana a Power Drain takes.
    pub power_drain_min: i32,
    pub power_drain_max: i32,
    /// The penalty to the resistance roll that frees a unit from Stasis.
    pub freeing_modifier: i32,
    /// Magic Vortex: random moves a turn, the damage it crushes a unit with, the strength of its
    /// bolt, and the damage to the city it adds each turn inside it.
    pub vortex_random_moves: usize,
    pub vortex_crush_damage: i32,
    pub vortex_bolt_strength: i32,
    pub vortex_city_damage: u32,
    /// How thick the scenery of the field is.
    pub scenery: SceneryRules,
    /// What the walls of a city do.
    pub city_wall: CityWallRules,
    /// Where the units of each side start.
    pub deployment: DeploymentRules,
}

/// The block of squares where a side's units start: they fill the columns (the x of the square, the
/// front one first), each column's rows (the y of the square) in the order given, the first
/// places going to the first units. The square of a lair's structure is skipped for the defenders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeployBlock {
    pub columns: Vec<i32>,
    pub rows: Vec<i32>,
}

impl DeployBlock {
    /// The squares of the block in the order the units take them.
    pub fn squares(&self) -> Vec<(i32, i32)> {
        self.columns
            .iter()
            .flat_map(|&x| self.rows.iter().map(move |&y| (x, y)))
            .collect()
    }
}

/// Where the units start in a field battle and in a city battle (`deployment` of `battle.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeploymentRules {
    pub attacker: DeployBlock,
    pub defender: DeployBlock,
    pub city_attacker: DeployBlock,
    /// The defenders of a city (at a wizard's fortress they also keep off the fortress's square).
    pub city_defender: DeployBlock,
}

/// What the walls of a city do in a battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CityWallRules {
    /// A standing tower (a corner of the wall) can be entered by a unit: no, as in the original. When
    /// it cannot, nobody moves onto it, flying or teleporting units included, no summoned or merged
    /// unit comes up on it and a confused unit does not wander onto it. A broken tower can be entered.
    pub tower_enterable: bool,
    /// A unit on foot can step onto a standing wall section from outside (not the gate's, not a tower).
    pub section_enterable: bool,
    /// A flying, teleporting or non-corporeal unit can cross a standing wall section (the walkers' rule
    /// above applies to it when not).
    pub passers_cross: bool,
    /// A unit on foot can step onto the gate's square from outside while its wall stands.
    pub gate_enterable: bool,
    /// The gate, a feature of the wall: its square, counted from the corner of the city area (the
    /// wall's north-west tower). It must be a wall square that is not a corner; a test checks it.
    pub gate: (i32, i32),
    /// The chance in a hundred that a wall crusher's blow on a standing wall section brings it down: a
    /// blow in melee (also the one on a unit standing on the wall), and a shot.
    pub crush_melee_percent: u32,
    pub crush_ranged_percent: u32,
}

/// What fighting earns: experience, fame and the levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenownRules {
    /// Experience a won battle gives each surviving unit of the winner for each dead enemy unit,
    /// and the point every unit gains each turn.
    pub xp_per_kill: u32,
    pub xp_per_turn: u32,
    /// Units dead on one side that cost it a point of fame and earn the other side one.
    pub fame_kills: usize,
    /// A fantastic unit that costs this much (its mana budget) is a very rare foe: another point.
    pub rare_foe_cost: u32,
    /// The fame Just Cause adds while it lasts, and the conqueror of a fortress city gains.
    pub just_cause_fame: i32,
    pub fortress_prize_fame: i32,
    /// The level (from 0) Heroism lifts a unit to.
    pub heroism_level: usize,
    /// What Charmed adds to a hero's resistance: enough to pass every check.
    pub charmed_resistance: i32,
}

impl Default for RenownRules {
    fn default() -> Self {
        Self {
            xp_per_kill: 2,
            xp_per_turn: 1,
            fame_kills: 4,
            rare_foe_cost: 600,
            just_cause_fame: 10,
            fortress_prize_fame: 5,
            heroism_level: 3,
            charmed_resistance: 30,
        }
    }
}

/// What the overland spells do: chances in percent of a hundred, strengths, penalties, and the
/// buildings and realms they single out (numbers of `buildings.ron`, `realms.ron`, `units.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellEffectRules {
    /// A volcano: each building of the city under it collapses, its square gets a mineral, and
    /// it cools into mountains each turn; Armageddon's volcano under a city takes a building, a
    /// unit and a citizen with this chance.
    pub collapse_percent: u32,
    pub mineral_percent: u32,
    pub cool_percent: u32,
    pub armageddon_city_percent: u32,
    /// A corrupted square heals by itself each turn.
    pub corruption_fade_percent: u32,
    /// The unrest another wizard's Armageddon, and a Great Wasting, add to a city.
    pub armageddon_unrest: i32,
    pub wasting_unrest: i32,
    /// Chaos Rift: bolts on each unit in the city every turn, their strength, and the chance a
    /// building falls.
    pub chaos_rift_bolts: u32,
    pub chaos_rift_strength: i32,
    pub chaos_rift_building_percent: u32,
    /// Meteor Storm: the strength of the fire attack on each unit outside a city, and the chance a
    /// building of another wizard's city falls.
    pub meteor_strength: i32,
    pub meteor_building_percent: u32,
    /// Call the Void: the chance each building falls, the damage to each unit, the chance each
    /// square near the city is corrupted and each citizen but the first dies.
    pub void_building_percent: u32,
    pub void_unit_damage: u32,
    pub void_corrupt_percent: u32,
    pub void_citizen_percent: u32,
    /// Nature's Wrath: the chance a unit dies and a building falls.
    pub wrath_unit_percent: u32,
    pub wrath_building_percent: u32,
    /// Great Wasting and Armageddon: `min_squares` plus up to `square_spread - 1` squares a turn,
    /// none within `safe_distance` of the caster's cities, `scatter_tries` draws for them; a sea
    /// square that was rolled is kept with `sea_percent`.
    pub safe_distance: i32,
    pub min_squares: u32,
    pub square_spread: u32,
    pub scatter_tries: u32,
    pub sea_percent: u32,
    /// The unit Lycanthropy makes of a normal unit.
    pub werewolves: u16,
    /// Black Wind's and Great Unsummoning's penalty to the resistance roll.
    pub black_wind_modifier: i32,
    pub unsummoning_modifier: i32,
    /// Drain Power takes the sum of `drain_rolls` rolls of a die of `drain_die` sides.
    pub drain_rolls: u32,
    pub drain_die: u32,
    /// Earthquake: the chance a building falls and a unit that cannot fly or slip through the
    /// ground dies.
    pub earthquake_building_percent: u32,
    pub earthquake_unit_percent: u32,
    /// How hard Spell Lock is to dispel, and the strength of Suppress Magic's standing dispel.
    pub spell_lock_difficulty: u32,
    pub suppress_magic_strength: u32,
    /// The realms whose spells Consecration keeps off the city and the units in it.
    pub consecration_realms: Vec<u16>,
    /// The buildings whose magic power Evil Presence destroys.
    pub religious_buildings: Vec<u16>,
    /// The City Walls building.
    pub city_walls: u16,
    /// The buildings that allow the use of nightshade, and the dispel strength each plant adds.
    pub nightshade_buildings: Vec<u16>,
    pub nightshade_strength: u32,
}

impl Default for SpellEffectRules {
    fn default() -> Self {
        Self {
            collapse_percent: 15,
            mineral_percent: 20,
            cool_percent: 2,
            armageddon_city_percent: 25,
            corruption_fade_percent: 2,
            armageddon_unrest: 2,
            wasting_unrest: 1,
            chaos_rift_bolts: 5,
            chaos_rift_strength: 8,
            chaos_rift_building_percent: 5,
            meteor_strength: 4,
            meteor_building_percent: 1,
            void_building_percent: 50,
            void_unit_damage: 10,
            void_corrupt_percent: 50,
            void_citizen_percent: 50,
            wrath_unit_percent: 10,
            wrath_building_percent: 5,
            safe_distance: 2,
            min_squares: 3,
            square_spread: 4,
            scatter_tries: 50,
            sea_percent: 20,
            werewolves: 168,
            black_wind_modifier: -1,
            unsummoning_modifier: -2,
            drain_rolls: 10,
            drain_die: 20,
            earthquake_building_percent: 15,
            earthquake_unit_percent: 25,
            spell_lock_difficulty: 150,
            suppress_magic_strength: 500,
            consecration_realms: vec![4, 2],
            religious_buildings: vec![22, 23, 24, 25],
            city_walls: 35,
            nightshade_buildings: vec![22, 10, 23, 24, 25, 17, 18, 19, 21],
            nightshade_strength: 100,
        }
    }
}

/// The rows a setting of the game changes: the retorts that cost two picks under Adamant Alchemy
/// and 2-Pick Myrran, the ships whose capacity the player sets, and the cavalry and halberdier
/// units that the setting `cavalry_halberdier_abilities` gives First Strike and Negate First
/// Strike (numbers of `retorts.ron`, `units.ron` and `abilities.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingTargets {
    pub alchemy_retort: u16,
    pub myrran_retort: u16,
    pub trireme_unit: u16,
    pub galley_unit: u16,
    /// The cavalry units: the setting gives each of them `first_strike`.
    pub cavalry_units: Vec<u16>,
    /// The halberdier units: the setting gives each of them `negate_first_strike`.
    pub halberdier_units: Vec<u16>,
    pub first_strike: u16,
    pub negate_first_strike: u16,
}

impl Default for SettingTargets {
    fn default() -> Self {
        Self {
            alchemy_retort: 0,
            myrran_retort: 9,
            trireme_unit: 35,
            galley_unit: 36,
            cavalry_units: vec![42, 60, 98, 107, 141],
            halberdier_units: vec![48, 59, 68, 77, 85, 97, 116, 122, 139, 149],
            first_strike: 23,
            negate_first_strike: 48,
        }
    }
}

/// Abilities (numbers of `abilities.ron`) the rules treat as a group, and what they are worth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityRules {
    /// What a unit's ability does for how quickly it gets to a square, in percent of its movement
    /// rate (`mobility`): `(ability, percent)`; the best that applies counts, none is multiplied.
    pub mobility: Vec<(u16, u32)>,
    /// The same for the movement kind of the unit: flying and swimming.
    pub flying_movement_pct: u32,
    pub swimming_movement_pct: u32,
    /// Abilities that carry the whole stack a unit stands in (pathfinding, wind walking).
    pub stack_carriers: Vec<u16>,
    /// What an undead unit is immune to, besides what its kind already is.
    pub undead_immunities: Vec<u16>,
    /// The magic ranged attacks, the only ones Arcane Power adds to.
    pub magic_ranged: Vec<u16>,
    /// The hero abilities that train the stack, and their super forms.
    pub armsmaster: u16,
    pub super_armsmaster: u16,
    pub mutation: MutationRules,
}

/// Chaos mutations: what each gives, and the abilities that mark a mutated unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationRules {
    /// The ability that marks a unit as mutated, and the ones that name its mutation.
    pub mutated: u16,
    pub armor_marker: u16,
    pub wings_marker: u16,
    pub breath_marker: u16,
    /// The ability the wings give (flying) and the breath (fire breath).
    pub wings_ability: u16,
    pub breath_ability: u16,
    /// The thrown-attack ability: a unit with it and a ranged attack of at most
    /// `thrown_most` may still get the breath.
    pub thrown_ability: u16,
    pub thrown_most: i32,
    /// Defence the demon-skin armour adds; whole moves a unit with wings has at least; strength
    /// and ammunition of the fire breath.
    pub armor_defense: i32,
    pub wings_moves: i32,
    pub breath_strength: i32,
    pub breath_ammo: i32,
}

impl Default for MutationRules {
    fn default() -> Self {
        Self {
            mutated: 98,
            armor_marker: 104,
            wings_marker: 105,
            breath_marker: 106,
            wings_ability: 24,
            breath_ability: 20,
            thrown_ability: 69,
            thrown_most: 3,
            armor_defense: 3,
            wings_moves: 2,
            breath_strength: 2,
            breath_ammo: 1,
        }
    }
}

impl Default for AbilityRules {
    fn default() -> Self {
        Self {
            mobility: vec![
                (49, 300),
                (74, 300),
                (24, 200),
                (50, 200),
                (45, 120),
                (25, 120),
                (71, 120),
            ],
            flying_movement_pct: 200,
            swimming_movement_pct: 150,
            stack_carriers: vec![50, 74],
            undead_immunities: vec![16, 10, 52, 31],
            magic_ranged: vec![36, 22, 60, 14, 29, 58, 47],
            armsmaster: 81,
            super_armsmaster: 82,
            mutation: MutationRules::default(),
        }
    }
}

/// What it costs to make an item and how many a wizard may hold and carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemRules {
    /// What the plain item of each kind costs before its powers (a kind not listed costs nothing).
    pub base_cost: Vec<(ItemKind, u32)>,
    /// The defence a kind adds of its own (the mail), on top of its powers.
    pub kind_defense: Vec<(ItemKind, i32)>,
    /// Most powers on one item, and most charges of an embedded spell.
    pub max_powers: usize,
    pub max_charges: u32,
    /// The group of powers that are free to combine (the special powers); the others allow one
    /// power each.
    pub free_group: u8,
    /// The most a single power may cost on an item of Enchant Item.
    pub enchant_max_power_cost: u32,
    /// What moving an item to a hero somewhere else costs, in mana.
    pub transfer_mana: i32,
    /// A random item asked for no value is worth `random_value_base`, plus a hundred-step of
    /// `random_value_step` up to `random_value_steps` times; its powers cost at most
    /// `random_max_power_cost` (Enchant Item: the cap above) and it has at most `random_max_powers`
    /// of them (`enchant_random_powers` from Enchant Item, `other_random_powers` on a kind that is
    /// not in `many_power_kinds`).
    pub random_value_base: u32,
    pub random_value_step: u32,
    pub random_value_steps: u32,
    pub random_max_power_cost: u32,
    pub enchant_random_powers: usize,
    pub many_random_powers: usize,
    pub other_random_powers: usize,
    pub many_power_kinds: Vec<ItemKind>,
    /// How many draws the search for a prefab item and for a random item's powers gets.
    pub search_tries: u32,
}

impl Default for ItemRules {
    fn default() -> Self {
        use ItemKind::*;
        Self {
            base_cost: vec![
                (Sword, 100),
                (Mace, 100),
                (Axe, 100),
                (Bow, 100),
                (Staff, 300),
                (Wand, 200),
                (Misc, 50),
                (Shield, 100),
                (Chainmail, 100),
                (Platemail, 300),
            ],
            kind_defense: vec![(Chainmail, 1), (Platemail, 2)],
            max_powers: 4,
            max_charges: 4,
            free_group: 9,
            enchant_max_power_cost: 200,
            transfer_mana: 20,
            random_value_base: 700,
            random_value_step: 100,
            random_value_steps: 10,
            random_max_power_cost: 20_000,
            enchant_random_powers: 2,
            many_random_powers: 4,
            other_random_powers: 3,
            many_power_kinds: vec![Sword, Mace, Axe, Staff, Wand],
            search_tries: 50,
        }
    }
}

impl ItemRules {
    /// What the plain item of a kind costs before its powers.
    pub fn base_cost(&self, kind: ItemKind) -> u32 {
        self.base_cost
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0, |&(_, c)| c)
    }

    /// The defence the kind adds of its own.
    pub fn kind_defense(&self, kind: ItemKind) -> i32 {
        self.kind_defense
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(0, |&(_, d)| d)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WizardRules {
    /// The tax rate everyone starts with: a gold for each citizen.
    pub start_tax_rate: u8,
    /// Citizens in the capital at the start, how many of them farm, and its buildings.
    pub capital_figures: u32,
    pub capital_farmers: u32,
    pub capital_buildings: Vec<Id<BuildingDef>>,
    /// The most retorts a wizard has, and how many spells are offered for research at once.
    pub max_retorts: usize,
    pub research_slots: usize,
    /// A wizard with less casting skill than this does not resign to the Spell of Return.
    pub resign_below_skill: i32,
    /// A spell cast overland that can also be cast in battle costs this many times the battle
    /// price.
    pub overland_cost_factor: i32,
    /// Evil Omens: life and nature spells cost this many percent more.
    pub evil_omens_percent: i32,
    /// Each book beyond `book_steps_from` in the realm of a spell takes `book_step_percent` off
    /// its cost and adds as much to its research.
    pub book_steps_from: i32,
    pub book_step_percent: i32,
    /// Books of one realm that take `mastery_discount` research points off the Spell of Mastery.
    pub mastery_discount_books: i32,
    pub mastery_discount: i32,
    /// A hero's mana in battle: `(casting skill + 1) * (level + 1) * hero_mana_percent / 100`.
    pub hero_mana_percent: i32,
}

impl Default for WizardRules {
    fn default() -> Self {
        Self {
            start_tax_rate: 2,
            capital_figures: 4,
            capital_farmers: 3,
            capital_buildings: Vec::new(),
            max_retorts: 6,
            research_slots: 8,
            resign_below_skill: 40,
            overland_cost_factor: 5,
            evil_omens_percent: 50,
            book_steps_from: 7,
            book_step_percent: 10,
            mastery_discount_books: 11,
            mastery_discount: 3000,
            hero_mana_percent: 250,
        }
    }
}

/// What a good is worth to a wizard in a trade, in percent (`worth` = cost * percent / 100).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeRules {
    /// What one more book in the realm of a spell adds to its worth, and the books that count.
    pub spell_book_pct: i32,
    pub spell_book_cap: i32,
    /// An item to a holder with a hero that has a free slot for it, with one whose slot for it is
    /// taken, and with no hero that could use it.
    pub item_free_slot_pct: i32,
    pub item_taken_slot_pct: i32,
    pub item_no_user_pct: i32,
    /// A unit to a wizard at war.
    pub unit_at_war_pct: i32,
    /// What one spearman of combat rating (`unit_ratings.ron`) is worth in gold: a unit is worth
    /// at least its rating times this, whatever it cost to make (the median unit cost per spearman
    /// of the rating file is about ten). 0 leaves a unit at its cost.
    #[serde(default)]
    pub gold_per_rating: i32,
}

impl Default for TradeRules {
    fn default() -> Self {
        Self {
            spell_book_pct: 5,
            spell_book_cap: 8,
            item_free_slot_pct: 150,
            item_taken_slot_pct: 100,
            item_no_user_pct: 50,
            unit_at_war_pct: 125,
            gold_per_rating: 0,
        }
    }
}

/// The combat forecast before an attack (setting `combat_forecast`, `Forecast` in core).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastRules {
    /// The shares of the other side's worth (percent, rising), below which a side is completely
    /// outmatched, much weaker and slightly weaker; the same shares turned over (a side worth
    /// 100 / share times the other) are the marks of being slightly stronger, much stronger and
    /// the other side outmatched. Between the two inner marks the sides are even.
    pub bands_pct: [u32; 3],
}

impl Default for ForecastRules {
    fn default() -> Self {
        Self {
            bands_pct: [33, 60, 85],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomacyRules {
    /// What Subversion takes off every other wizard's relation to its target.
    pub subversion_penalty: i32,
    /// The most messages a negotiation takes: the offer and the counters that follow it.
    pub max_rounds: u8,
    /// A pact needs a relation above `pact_relation`, an alliance above `alliance_relation`.
    pub pact_relation: i32,
    pub alliance_relation: i32,
}

impl Default for DiplomacyRules {
    fn default() -> Self {
        Self {
            subversion_penalty: 25,
            max_rounds: 4,
            pact_relation: 10,
            alliance_relation: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferRules {
    /// No mercenaries are offered before this turn; the computer wizards do not hire before
    /// `ai_first_turn`.
    pub first_turn: u32,
    pub ai_first_turn: u32,
    /// The most any offer's chance can be (percent) before the computer wizards' bonus, and that
    /// bonus.
    pub max_chance: i32,
    pub ai_bonus: i32,
}

impl Default for OfferRules {
    fn default() -> Self {
        Self {
            first_turn: 30,
            ai_first_turn: 50,
            max_chance: 10,
            ai_bonus: 10,
        }
    }
}

/// How thick the scenery of a battle field is, as a percent of what the original lays: the field
/// is bigger than the original's, so the original's number is first scaled to its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SceneryRules {
    /// The trees of a field on forest ground.
    pub forest_trees_percent: u32,
    /// The rocks of a field on mountain ground.
    pub mountain_rocks_percent: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutpostRules {
    /// Tens of people a new outpost starts with, and at which it becomes a city of one (kept: a
    /// new hamlet starts with a head start).
    pub start_tens: u32,
    pub city_tens: u32,
}

impl Default for OutpostRules {
    fn default() -> Self {
        Self {
            start_tens: 3,
            city_tens: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreatRules {
    /// The raider clock's limit.
    pub raider_limit: u32,
    /// Extra damage the fighting toll counts when monsters win a city.
    pub rampage_damage: u32,
    /// The cheapest monster a budget still buys.
    pub cheapest_monster: i32,
    /// Marriage and the Great Meteor wait for this turn.
    pub big_events_from_turn: u32,
    /// Lairs guarded by creatures of these realms (realm numbers) do not send out monsters.
    pub spared_realms: Vec<u16>,
    /// The alternative raid from the sea (`threat/sea_raid.rs`, setting `sea_raids`).
    #[serde(default)]
    pub sea_raid: SeaRaidRules,
}

/// The raid from the sea: a free party of a random race lands on a far coast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeaRaidRules {
    /// The chance, in percent, that a raid is a sea raid when the setting is on.
    pub chance_percent: u32,
    /// The ship: `(from turn, unit number)`, in rising order of turn; the last one that has begun
    /// is sent.
    pub ships: Vec<(u32, u16)>,
    /// The party's size by turn, in rising order of turn: each step is `(from turn, base, dice,
    /// pool)`. The party has `base` plus one roll of a die of `dice` sides fighters, each rolled
    /// among the `pool` cheapest military units of the race (0: all of them). The last step that
    /// has begun counts.
    pub parties: Vec<(u32, u32, u32, usize)>,
    /// How many coast squares are rolled; the one furthest from any town is the landing.
    pub coast_rolls: u32,
}

impl Default for SeaRaidRules {
    fn default() -> Self {
        Self {
            chance_percent: 25,
            ships: vec![(0, 35 /* trireme */), (120, 36 /* galley */)],
            parties: vec![(0, 0, 2, 2), (121, 1, 2, 4), (251, 2, 3, 0)],
            coast_rolls: 4,
        }
    }
}

impl Default for ThreatRules {
    fn default() -> Self {
        Self {
            raider_limit: 30,
            rampage_damage: 40,
            cheapest_monster: 25,
            big_events_from_turn: 150,
            spared_realms: vec![3 /* life */],
            sea_raid: SeaRaidRules::default(),
        }
    }
}

/// The range multiplier of a spell cast in battle, in percent of its cost: `at_fortress` at the
/// fortress itself, else `base` plus `step` for each of the `beyond` distances the battle is
/// further than; a battle on another plane counts as `far` squares away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastingRange {
    pub at_fortress: i32,
    pub base: i32,
    pub step: i32,
    pub beyond: Vec<u32>,
    pub far: u32,
}

impl Default for CastingRange {
    fn default() -> Self {
        Self {
            at_fortress: 5,
            base: 10,
            step: 5,
            beyond: vec![5, 10, 15, 20],
            far: 30,
        }
    }
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            to_hit_percent: 30,
            block_percent: 30,
            tax_unrest_percent: vec![0, 10, 20, 30, 45, 60, 75],
            racial_unrest: Vec::new(),
            autobuild_order: Vec::new(),
            casting_range: CastingRange::default(),
            trade: TradeRules::default(),
            forecast: ForecastRules::default(),
            diplomacy: DiplomacyRules::default(),
            offer: OfferRules::default(),
            outpost: OutpostRules::default(),
            threat: ThreatRules::default(),
            garrison_minimum: Vec::new(),
            city_size_from: Vec::new(),
            starting_buildings: Vec::new(),
            limited_ai_races: Vec::new(),
            event_ores: Vec::new(),
            wizard: WizardRules::default(),
            item: ItemRules::default(),
            abilities: AbilityRules::default(),
            spell_effects: SpellEffectRules::default(),
            renown: RenownRules::default(),
            targets: SettingTargets::default(),
        }
    }
}

macro_rules! impl_numbered_def {
    ($($t:ty),* $(,)?) => {
        $(impl Def for $t {
            fn id(&self) -> &str {
                &self.id
            }

            fn num(&self) -> u16 {
                self.num
            }
        })*
    };
}

impl_numbered_def!(
    RealmDef,
    ItemDef,
    ItemPowerDef,
    LevelPathDef,
    WizardDef,
    RetortDef
);

impl Def for UnitDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl Def for RaceDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl Def for AbilityDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl BuildingDef {
    /// Whether a city of `race` may build it: the race is in the building's list. The list is
    /// complete (it already accounts for the buildings this one needs), and an empty list is no
    /// race.
    pub fn buildable_by(&self, race: super::RaceId) -> bool {
        self.races.contains(&race)
    }
}

impl Rules {
    /// The 0-based level that `xp` experience points give a unit on its level path: the last
    /// level whose threshold they reach (0 for a unit without a path). Nothing a unit is given
    /// besides (Heroism, a wizard's retorts) is in it.
    pub fn xp_level(&self, unit: &UnitDef, xp: u32) -> usize {
        unit.level_path
            .map(|p| self.level_paths.get(p))
            .map_or(0, |path| {
                path.levels.iter().rposition(|l| l.xp <= xp).unwrap_or(0)
            })
    }

    /// The combat rating of a unit at an experience level (from 0; the level of
    /// `UnitView::unit_level`): its worth in High Men spearmen (`UnitRatings`), a hero's times
    /// the hero factor. `None` for a unit with no row (a settler, a unit of a rule set that has no
    /// ratings): the callers fall back to `battle::power`.
    pub fn combat_rating(&self, unit: &UnitDef, level: usize) -> Option<f32> {
        let row = self.ratings.row(unit.num)?;
        let value = *row.levels.get(level).or_else(|| row.levels.last())?;
        Some(if unit.kind == UnitKind::Hero {
            value * self.ratings.hero_factor
        } else {
            value
        })
    }

    /// [`Self::combat_rating`] in the units of `battle::power` (times `power_scale`): what the AI
    /// and the game weigh a unit by.
    pub fn combat_power(&self, unit: &UnitDef, level: usize) -> Option<f32> {
        self.combat_rating(unit, level)
            .map(|r| r * self.ratings.power_scale)
    }

    /// Whether `race` is left out of the building's list for its own sake, not because it cannot
    /// build what the building needs: what the original's help text names as a race's missing
    /// buildings (`cant_build`).
    pub fn building_refused_to_race_itself(
        &self,
        building: &BuildingDef,
        race: super::RaceId,
    ) -> bool {
        !building.buildable_by(race)
            && building
                .requires
                .iter()
                .all(|&need| self.buildings.get(need).buildable_by(race))
    }
}

impl Def for BuildingDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

impl SpellDef {
    /// The types the spell harms with that a unit's immunity can stop: `harm_types`, less the
    /// death type when the spell is an exception to death immunity and the rule
    /// `death_immunity_exceptions` is on, and less the magic type likewise for magic immunity.
    pub fn harmful_types(
        &self,
        death_exceptions: bool,
        magic_exceptions: bool,
    ) -> super::damage_type::DamageTypes {
        let mut types = self.harm_types;
        if self.death_immunity_exception && death_exceptions {
            types = types.without(super::damage_type::DamageType::Death);
        }
        if self.magic_immunity_exception && magic_exceptions {
            types = types.without(super::damage_type::DamageType::Magic);
        }
        types
    }
    /// What this spell, lasting on a unit, gives up when `newer` is cast on the same unit: if it
    /// is not with `newer` (`not_with`), every plain bonus it adds (a passive, always-on `Add`
    /// modifier) comes back negated and tagged with this spell, so the two do not both count.
    pub fn bonuses_given_up_for(&self, newer: SpellId) -> Vec<Modifier> {
        if !self.not_with.contains(&newer) {
            return Vec::new();
        }
        self.effects
            .iter()
            .filter_map(|r| match (r.when, &r.condition, &r.effect) {
                (
                    Trigger::Passive,
                    Condition::Always,
                    Effect::Modify {
                        stat,
                        op: ModOp::Add,
                        value,
                        types,
                    },
                ) => Some(
                    Modifier::amount(
                        *stat,
                        ModOp::Add,
                        -*value,
                        Source::Spell(SpellId::new(self.num)),
                    )
                    .of_types(*types),
                ),
                _ => None,
            })
            .collect()
    }

    /// Whether the spell runs the named `Custom` hook.
    pub fn has_hook(&self, hook: &str) -> bool {
        self.hook_args(hook).is_some()
    }

    /// The arguments the spell gives the named `Custom` hook, if it runs it.
    /// Whether the spell carries `hook` with `arg` among its arguments.
    pub fn has_hook_arg(&self, hook: &str, arg: &str) -> bool {
        self.hook_args(hook)
            .is_some_and(|a| a.iter().any(|x| x == arg))
    }

    pub fn hook_args(&self, hook: &str) -> Option<&[String]> {
        self.effects.iter().find_map(|r| {
            r.effect
                .as_hook()
                .filter(|(n, _)| *n == hook)
                .map(|(_, a)| a)
        })
    }
}

impl Def for SpellDef {
    fn id(&self) -> &str {
        &self.id
    }

    fn num(&self) -> u16 {
        self.num
    }
}

/// All rules definitions.
/// `Default` is the empty rules (zeros and empty lists), for a game that is not given content yet;
/// the loader never uses it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rules {
    pub config: RulesConfig,
    pub races: Registry<RaceDef>,
    pub units: Registry<UnitDef>,
    pub abilities: Registry<AbilityDef>,
    pub buildings: Registry<BuildingDef>,
    pub realms: Registry<RealmDef>,
    pub spells: Registry<SpellDef>,
    pub items: Registry<ItemDef>,
    pub item_powers: Registry<ItemPowerDef>,
    pub level_paths: Registry<LevelPathDef>,
    pub wizards: Registry<WizardDef>,
    pub retorts: Registry<RetortDef>,
    /// The starting record of each hero type, in the order of the hero units (optional).
    pub hero_templates: Vec<HeroTemplate>,
    /// The unit combat ratings (`unit_ratings.ron`, optional): what each unit is worth in a fight.
    pub ratings: UnitRatings,
    /// What the ratings buy per resource (`unit_efficiency.ron`, optional).
    pub efficiency: UnitEfficiencies,
    /// What the load left out so that the game can still play (`docs/content-policy.md`): one issue
    /// for each row dropped, each with a `Degrade` outcome. Empty for a game with nothing wrong.
    pub degraded: Vec<crate::issue::ContentIssue>,
    /// The rules of the battle (`battle.ron`, optional: without it the original's numbers).
    pub battle: BattleRules,
    /// The guard and treasure rules of the sites (`lairs.ron`).
    pub lairs: super::lair::LairRules,
    /// How the AI weighs a place for a city (`ai_sites.ron`, optional).
    pub ai_sites: super::ai_site::SiteRules,
}
