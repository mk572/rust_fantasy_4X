//! The vocabulary abilities, buildings, spells and items use to say what they do.
//! These are plain data (no scripting).
//! References to other definitions are string ids, checked when content loads.
//!
//! Tests: none, because it only declares the effect types; `rules/exec.rs` runs them and has the tests.

use serde::{Deserialize, Serialize};

use super::ids::Id;
use super::stats::{ModOp, Stat};
use super::{AbilityDef, RealmDef, SpellDef};
use crate::map::terrain::TerrainClass;
use crate::map::Plane;

/// Names of the `Effect::Custom` hooks implemented in Rust, shared by several spells. A
/// `Custom` effect naming anything else fails content validation.
///
/// `unimplemented` marks an imported rule the engine has no primitive for yet; the
/// number of uses is the to-do list for new effect primitives.
pub const CUSTOM_HOOKS: &[&str] = &[
    "unimplemented",
    crate::spell::global_enchantment::GLOBAL_EFFECT,
    crate::item::crafting::CRAFT_ITEM,
    crate::city::gate::EARTH_GATE,
    crate::city::gate::ASTRAL_GATE,
    crate::unit::HEROISM,
    crate::spell::city::CITY_FLAG,
    crate::battle::field::BATTLEFIELD,
    crate::battle::field::DISINTEGRATE,
];

/// The spells that have code of their own in Rust (`Effect::Special`), by number: the
/// number is the only link between the spell and its code. A `Special` effect on a spell
/// not in this list fails content validation.
pub const SPECIAL_SPELLS: [u16; 50] = [
    1,   // earth_to_mud
    3,   // wall_of_stone
    10,  // earth_lore
    11,  // cracks_call
    15,  // change_terrain
    18,  // transmute
    19,  // natures_cures
    26,  // earthquake
    28,  // move_fortress
    43,  // floating_island
    47,  // word_of_recall
    48,  // counter_magic
    55,  // enchant_road
    58,  // spell_blast
    61,  // disjunction_true
    68,  // stasis
    72,  // spell_ward
    73,  // creature_binding
    75,  // great_unsummoning
    76,  // spell_binding
    81,  // warp_wood
    82,  // disrupt
    85,  // corruption
    89,  // warp_creature
    93,  // chaos_channels
    98,  // raise_volcano
    105, // magic_vortex
    115, // call_chaos
    119, // call_the_void
    132, // plane_shift
    133, // resurrection
    137, // raise_dead
    142, // incarnation
    167, // life_drain
    171, // drain_power
    172, // possession
    173, // lycanthropy
    177, // subversion
    186, // warp_node
    187, // black_wind
    191, // cruel_unminding
    195, // animate_dead
    199, // death_wish
    203, // summoning_circle
    205, // recall_hero
    208, // summon_hero
    210, // disjunction
    212, // summon_champion
    213, // spell_of_mastery
    214, // spell_of_return
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    Always,
    HasAbility(Id<AbilityDef>),
    TerrainIs(TerrainClass),
    OnPlane(Plane),
    RaceIs(Id<super::RaceDef>),
    RealmIs(Id<RealmDef>),
    /// The subject is a unit of this kind (normal units, heroes, creatures).
    KindIs(crate::rules::UnitKind),
    InBattle,
    InCity,
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetSel {
    This,
    Owner,
    AllOwnUnits,
    AllEnemyUnits,
    UnitsInTile,
    UnitsInArea(u8),
    City,
    Tile,
    /// The target picked when casting.
    Chosen,
    /// Every unit on the square of the chosen target (a square, a unit or a city), whoever
    /// owns it, a city's garrison included. For spells that rain on a whole square.
    UnitsOnChosenSquare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    /// Applies continuously while the source exists.
    Passive,
    OnTurnStart,
    OnCast,
    OnAttack,
    OnDefend,
    OnDeath,
    OnBuilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageKind {
    Physical,
    Fire,
    Cold,
    Lightning,
    Poison,
    Death,
    Holy,
}

/// How a damage spell is dealt in a battle (the original's attack attributes). None of it
/// matters overland, where the damage is a flat number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DamageTraits {
    /// Halves the target's defence.
    pub armor_piercing: bool,
    /// The target's defence does not count at all.
    pub ignores_defense: bool,
    /// Every figure of the target is hit on its own, none losing more than one figure.
    pub area: bool,
    /// A chain of bolts, each one die weaker than the last.
    pub decaying: bool,
    /// Mana beyond the cost that buys one more point of strength; 0 if extra mana does nothing.
    pub mana_per_point: u32,
}

/// Which units a [`Effect::Slay`] spell can destroy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SlayOnly {
    #[default]
    Any,
    /// Creatures of a realm (summoned and fantastic units), not the races' units.
    Fantastic,
    /// Creatures of the chaos or death realm.
    ChaosOrDeath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Modify {
        stat: Stat,
        op: ModOp,
        value: i32,
    },
    GrantAbility(Id<AbilityDef>),
    RemoveAbility(Id<AbilityDef>),
    /// `resistible`: the target gets a resistance check, against `realm` with `resist_modifier`
    /// added to its resistance (negative for a spell that is hard to resist). A target with
    /// one of the `immune` abilities (`fire_immunity`, `magic_immunity`, ...) takes nothing.
    Damage {
        amount: i32,
        kind: DamageKind,
        resistible: bool,
        #[serde(default)]
        immune: Vec<Id<AbilityDef>>,
        #[serde(default)]
        realm: Option<Id<RealmDef>>,
        #[serde(default)]
        resist_modifier: i32,
        #[serde(default)]
        traits: DamageTraits,
    },
    Heal(i32),
    /// Battle only: each figure of the target must resist (a check against `realm` with
    /// `modifier` added to the resistance) or is destroyed. A target with one of the `immune`
    /// abilities takes no harm. `below`: the target is destroyed outright if its resistance
    /// plus `modifier` is under this (Disintegrate) instead. `mana_per_step`: each this much
    /// mana beyond the cost lowers the resistance by one more (Banish).
    Slay {
        realm: Id<RealmDef>,
        modifier: i32,
        #[serde(default)]
        immune: Vec<Id<AbilityDef>>,
        #[serde(default)]
        only: SlayOnly,
        #[serde(default)]
        below: Option<i32>,
        #[serde(default)]
        mana_per_step: u32,
    },
    AddResource {
        #[serde(default)]
        gold: i32,
        #[serde(default)]
        food: i32,
        #[serde(default)]
        mana: i32,
        #[serde(default)]
        research: i32,
    },
    Summon {
        unit: super::UnitId,
        count: u8,
    },
    ChangeTerrain(TerrainClass),
    /// Put a map feature on the tile: its number in `features.ron`.
    SetFeature(crate::map::FeatureId),
    /// Attach a lasting spell (by spell id) to the target.
    ApplyEnchantment(Id<SpellDef>),
    Dispel {
        strength: u32,
    },
    /// A named hook implemented in Rust and shared by several spells; must appear in
    /// [`CUSTOM_HOOKS`].
    Custom(String),
    /// The spell's own code in Rust, found by the spell's number ([`SPECIAL_SPELLS`]).
    Special,
}

fn always() -> Condition {
    Condition::Always
}
fn passive() -> Trigger {
    Trigger::Passive
}
fn this() -> TargetSel {
    TargetSel::This
}

/// One thing a definition does: when, under what condition, to whom, and what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRule {
    #[serde(default = "passive")]
    pub when: Trigger,
    #[serde(default = "always")]
    pub condition: Condition,
    #[serde(default = "this")]
    pub target: TargetSel,
    pub effect: Effect,
}
