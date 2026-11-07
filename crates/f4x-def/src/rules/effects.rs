//! The vocabulary abilities, buildings, spells and items use to say what they do.
//! These are plain data (no scripting).
//! References to other definitions are string ids, checked when content loads.
//!
//! Tests: none, because it only declares the effect types; `rules/exec.rs` runs them and has the tests.

use serde::{Deserialize, Serialize};

use super::ids::Id;
use super::stats::{ModOp, Stat};
use super::{AbilityDef, RealmDef, SpellDef};
use crate::map::Plane;
use crate::map::TerrainClass;

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
    /// The spell in question is of this kind (for a wizard's retorts: what a spell costs and how
    /// fast it is researched).
    SpellIs(SpellTag),
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

/// A kind of spell a retort's rule can single out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellTag {
    /// A spell that summons a creature.
    Summoning,
    /// Enchant Item and Create Artifact.
    Crafting,
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
    /// A named hook implemented in Rust and shared by several spells. The name is a plain tag
    /// the content gives, with the arguments that tell the spells sharing it apart (`Custom(name:
    /// "wall", args: ["fire"])`); the game code that implements it checks both (`f4x_core::hook`).
    Custom {
        name: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Effect {
    /// A hook with no arguments.
    pub fn hook(name: &str) -> Effect {
        Effect::Custom {
            name: name.to_string(),
            args: Vec::new(),
        }
    }

    /// The hook's name and arguments, if this is a `Custom` effect.
    pub fn as_hook(&self) -> Option<(&str, &[String])> {
        match self {
            Effect::Custom { name, args } => Some((name, args)),
            _ => None,
        }
    }
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
