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
/// Races own no unit or building lists: a unit belongs to a race through its `race`
/// field, and a building is open to a race unless its `races` list excludes it.
/// This keeps a mod's new unit or building to a single entry in a single file.
pub struct RaceDef {
    pub id: String,
    pub name: String,
    /// Terrain the race's cities prefer; used by AI and map placement.
    #[serde(default)]
    pub home_terrain: Vec<TerrainClass>,
    /// Applied to every city of the race.
    #[serde(default)]
    pub effects: Vec<EffectRule>,
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
    /// Races that can build it; empty means every race.
    #[serde(default)]
    pub races: Vec<String>,
    #[serde(default)]
    pub effects: Vec<EffectRule>,
}

/// A school of magic. Data, so the set of realms is not hard-coded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealmDef {
    pub id: String,
    pub name: String,
    pub color: (u8, u8, u8),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Base chance (percent) for each figure's attack to hit.
    pub to_hit_percent: u8,
    /// Chance (percent) for each point of defense to block a hit.
    pub block_percent: u8,
    /// Minimum Chebyshev distance between cities on one plane.
    pub min_city_distance: u32,
    /// Production a city makes before buildings and race modifiers.
    pub base_city_production: i32,
    /// Most units that may share one tile.
    pub max_stack: u32,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            to_hit_percent: 30,
            block_percent: 30,
            min_city_distance: 3,
            base_city_production: 2,
            max_stack: 9,
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
    LevelPathDef
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
}
