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
    crate::special_spells::TRANSMUTE,
    crate::special_spells::EARTH_LORE,
    crate::special_spells::NATURES_CURES,
    crate::special_spells::WORD_OF_RECALL,
    crate::special_spells::PLANE_SHIFT,
    crate::items::crafting::CRAFT_ITEM,
    crate::hero_spells::SUMMON_HERO,
    crate::hero_spells::SUMMON_CHAMPION,
    crate::hero_spells::INCARNATION,
    crate::hero_spells::RESURRECTION,
    crate::wizard_spells::DRAIN_POWER,
    crate::wizard_spells::CRUEL_UNMINDING,
    crate::wizard_spells::SUBVERSION,
    crate::wizard_spells::GREAT_UNSUMMONING,
    crate::wizard_spells::DEATH_WISH,
    crate::wizard_spells::EARTHQUAKE,
    crate::wizard_spells::BLACK_WIND,
    crate::wizard_spells::LYCANTHROPY,
    crate::endgame::SPELL_OF_MASTERY,
    crate::gates::CITY_GATE,
    crate::city_spells::CITY_FLAG,
    crate::city_spells::WALL_OF_STONE,
    crate::city_spells::SPELL_WARD,
    crate::dispel::DISJUNCTION,
    crate::dispel::DISJUNCTION_TRUE,
    crate::dispel::SPELL_BINDING,
    crate::dispel::SPELL_BLAST,
    crate::dispel::COUNTER_MAGIC,
    crate::battle::field::BATTLEFIELD,
    crate::battle::field::CALL_CHAOS,
    crate::battle::field::DISINTEGRATE,
    crate::battle::spells::ANIMATE_DEAD,
    crate::battle::spells::CREATURE_BINDING,
    crate::battle::spells::POSSESSION,
    crate::battle::spells::RAISE_DEAD,
    crate::battle::spells::RECALL_HERO,
    crate::battle::spells::EARTH_TO_MUD,
    crate::battle::spells::CRACKS_CALL,
    crate::battle::spells::DISRUPT,
    crate::battle::spells::LIFE_DRAIN,
    crate::battle::spells::WARP_WOOD,
    crate::battle::spells::WARP_CREATURE,
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
    GrantAbility(String),
    RemoveAbility(String),
    /// `resistible`: the target gets a resistance check, against `realm` with `resist_modifier`
    /// added to its resistance (negative for a spell that is hard to resist). A target with
    /// one of the `immune` abilities (`fire_immunity`, `magic_immunity`, ...) takes nothing.
    Damage {
        amount: i32,
        kind: DamageKind,
        resistible: bool,
        #[serde(default)]
        immune: Vec<String>,
        #[serde(default)]
        realm: Option<String>,
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
        realm: String,
        modifier: i32,
        #[serde(default)]
        immune: Vec<String>,
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
