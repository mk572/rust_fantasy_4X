//! The vocabulary abilities, buildings, spells and items use to say what they do.
//! These are plain data (no scripting); the executor that runs them comes later.
//! References to other definitions are string ids, checked when content loads.

use serde::{Deserialize, Serialize};

use super::stats::{ModOp, Stat};
use crate::content::TerrainClass;
use crate::map::Plane;

/// Names of `Effect::Custom` hooks implemented in Rust. A `Custom` effect naming
/// anything else fails content validation.
///
/// `unimplemented` marks an imported rule the engine has no primitive for yet; the
/// number of uses is the to-do list for new effect primitives.
pub const CUSTOM_HOOKS: &[&str] = &[
    "unimplemented",
    crate::fortress::MOVE_FORTRESS,
    crate::fortress::MOVE_CIRCLE,
    crate::fortress::RETURN,
    crate::terraform::RAISE_VOLCANO,
    crate::terraform::CHANGE_TERRAIN,
    crate::globals::GLOBAL_EFFECT,
    crate::roads::ENCHANT_ROAD,
    crate::corruption::CORRUPT_LAND,
    crate::corruption::WARP_NODE,
    crate::endgame::SPELL_OF_MASTERY,
    crate::gates::CITY_GATE,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    Always,
    HasAbility(String),
    TerrainIs(TerrainClass),
    OnPlane(Plane),
    RaceIs(String),
    RealmIs(String),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Modify {
        stat: Stat,
        op: ModOp,
        value: i32,
    },
    GrantAbility(String),
    RemoveAbility(String),
    /// `resistible`: the target gets a resistance check.
    Damage {
        amount: i32,
        kind: DamageKind,
        resistible: bool,
    },
    Heal(i32),
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
        unit: String,
        count: u8,
    },
    ChangeTerrain(TerrainClass),
    SetFeature(String),
    /// Attach a lasting spell (by spell id) to the target.
    ApplyEnchantment(String),
    Dispel {
        strength: u32,
    },
    /// A named hook implemented in Rust; must appear in [`CUSTOM_HOOKS`].
    Custom(String),
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
