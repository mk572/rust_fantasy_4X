//! Definition types: static content describing races, units, buildings, spells and
//! so on. Runtime state (damage taken, remaining duration) lives in ECS components.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::effects::EffectRule;
use super::registry::{Def, Registry};
use super::stats::Stat;
use crate::content::TerrainClass;

macro_rules! impl_def {
    ($($t:ty),*) => { $(impl Def for $t { fn id(&self) -> &str { &self.id } })* };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceDef {
    pub id: String,
    pub name: String,
    /// Units cities of this race can build.
    pub units: Vec<String>,
    /// Buildings cities of this race can build (besides race-neutral ones).
    pub buildings: Vec<String>,
    /// Terrain the race's cities prefer; used by AI and map placement.
    #[serde(default)]
    pub home_terrain: Vec<TerrainClass>,
    /// Applied to every city of the race.
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Names handed to new cities, in order.
    #[serde(default)]
    pub city_names: Vec<String>,
    /// Food each farmer grows.
    #[serde(default = "two")]
    pub farmer_food: u8,
    /// Production per worker, in half-units (4 = two).
    #[serde(default = "four")]
    pub worker_production_x2: u8,
    /// Mana per citizen, in half-units (2 = one per citizen).
    #[serde(default)]
    pub mana_per_citizen_x2: u8,
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
}

fn two() -> u8 {
    2
}
fn four() -> u8 {
    4
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
pub struct Upkeep {
    #[serde(default)]
    pub gold: u32,
    #[serde(default)]
    pub food: u32,
    #[serde(default)]
    pub mana: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemSlot {
    Weapon,
    Armor,
    Misc,
}

/// Extra data for [`UnitKind::Hero`] units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroDef {
    /// Slots the hero can carry items in.
    pub slots: Vec<ItemSlot>,
    /// Gold to hire.
    pub hire_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub kind: UnitKind,
    /// Required for `Normal` units; heroes and creatures have none.
    #[serde(default)]
    pub race: Option<String>,
    /// Required for `Summoned` units.
    #[serde(default)]
    pub realm: Option<String>,
    /// Production cost (gold-equivalent) or summoning mana cost.
    pub cost: u32,
    #[serde(default)]
    pub upkeep: Upkeep,
    pub stats: BTreeMap<Stat, i32>,
    pub movement: MovementType,
    #[serde(default)]
    pub abilities: Vec<String>,
    /// Buildings the producing city must have.
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub level_path: Option<String>,
    #[serde(default)]
    pub hero: Option<HeroDef>,
    /// Picture for the client: `(file, entry)` in the local art set.
    #[serde(default)]
    pub art: Option<(String, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub effects: Vec<EffectRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    pub cost: u32,
    #[serde(default)]
    pub upkeep_gold: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Building made redundant by this one. It still counts as built (for requirements
    /// and effects) but costs no upkeep.
    #[serde(default)]
    pub replaces: Option<String>,
    /// Races that can build it; empty means every race.
    #[serde(default)]
    pub races: Vec<String>,
    #[serde(default)]
    pub effects: Vec<EffectRule>,
    /// Footprint in the city picture: 0 is 2x2, 1 is 2x3, 2 is 3x2, 3 is 3x3.
    #[serde(default)]
    pub shape: u8,
}

/// A school of magic. Data, so the set of realms is not hard-coded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmDef {
    pub id: String,
    pub name: String,
    pub color: (u8, u8, u8),
    /// Spells (ids) a wizard with books in this realm starts with when they choose none, in
    /// the order they are taken: common ones first.
    #[serde(default)]
    pub default_spells: Vec<String>,
}

/// A ready-made wizard for the creation screen: a name, a book and retort loadout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Spell books as `(realm id, count)`.
    pub books: Vec<(String, u8)>,
    pub retorts: Vec<crate::magic::Retort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    Instant,
    Permanent,
    Turns(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellDef {
    pub id: String,
    pub name: String,
    pub realm: String,
    pub rarity: Rarity,
    /// Mana to cast.
    pub cost: u32,
    pub research_cost: u32,
    pub contexts: Vec<CastContext>,
    pub target: SpellTarget,
    pub duration: Duration,
    /// Mana per turn to keep a lasting spell going.
    #[serde(default)]
    pub upkeep: u32,
    /// Strength a dispel must beat.
    #[serde(default)]
    pub dispel_difficulty: u32,
    pub effects: Vec<EffectRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub slot: ItemSlot,
    pub cost: u32,
    pub effects: Vec<EffectRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelDef {
    pub name: String,
    /// Experience needed to reach this level.
    pub xp: u32,
    pub effects: Vec<EffectRule>,
}

/// Experience thresholds and what each level grants (units and heroes share the mechanism).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelPathDef {
    pub id: String,
    pub levels: Vec<LevelDef>,
}

/// Tunable numbers that are not content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Base chance (percent) for each figure's attack to hit.
    pub to_hit_percent: u8,
    /// Chance (percent) for each point of defense to block a hit.
    pub block_percent: u8,
    /// Minimum Chebyshev distance between cities on one plane.
    pub min_city_distance: u32,
    /// Largest city, in citizens (thousands of people).
    pub max_city_size: u32,
    /// Unrest percent caused by each tax rate; the rate is also half the gold per citizen.
    pub tax_unrest_percent: Vec<u8>,
    /// Unrest (in tens of percent) in a city of one race under a capital of another:
    /// `racial_unrest[capital race][city race]`, indexed in race file order. Empty means none.
    pub racial_unrest: Vec<Vec<i8>>,
    /// Building ids in the order the Grand Vizier picks them (our own list).
    pub autobuild_order: Vec<String>,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            to_hit_percent: 30,
            block_percent: 30,
            min_city_distance: 4,
            max_city_size: 25,
            tax_unrest_percent: vec![0, 10, 20, 30, 45, 60, 75],
            racial_unrest: Vec::new(),
            autobuild_order: Vec::new(),
        }
    }
}

impl_def!(
    RaceDef,
    UnitDef,
    AbilityDef,
    BuildingDef,
    RealmDef,
    SpellDef,
    ItemDef,
    LevelPathDef,
    WizardDef
);

/// All rules definitions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rules {
    pub config: RulesConfig,
    pub races: Registry<RaceDef>,
    pub units: Registry<UnitDef>,
    pub abilities: Registry<AbilityDef>,
    pub buildings: Registry<BuildingDef>,
    pub realms: Registry<RealmDef>,
    pub spells: Registry<SpellDef>,
    pub items: Registry<ItemDef>,
    pub level_paths: Registry<LevelPathDef>,
    pub wizards: Registry<WizardDef>,
}
