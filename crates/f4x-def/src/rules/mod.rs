//! The rules framework: definitions, registries, stats and effects.
//! See `docs/rules-framework.md`.
//!
//! Tests: none, because it only declares modules.

pub mod ai_battle;
pub mod ai_strategy;
pub mod attack_nature;
pub mod battle_number;
pub mod combat_die;
pub mod counter_effect;
pub mod damage_type;
pub mod defs;
pub mod effects;
pub mod facts;
pub mod forecast;
pub mod global_effect;
pub mod hook_name;
pub mod ids;
pub mod immunity;
pub mod lair;
pub mod lever;
pub mod realm;
pub mod registry;
pub mod retort_stat;
pub mod stacking;
pub mod stats;
pub mod target_error;
pub mod unit_effect;
pub mod unit_flag;

pub use combat_die::CombatDie;
pub use defs::*;
pub use ids::Id;

/// A unit ability: the number of its row in `abilities.ron` (the numbers are frozen; the
/// alias in the file is for people, and `Registry::id_of` turns it into the number).
pub type Ability = Id<AbilityDef>;
pub use registry::{Def, Registry};
pub use stats::{ModOp, Modifier, Source, Stat};

pub type RaceId = Id<RaceDef>;
pub type UnitId = Id<UnitDef>;
pub type AbilityId = Id<AbilityDef>;
pub type BuildingId = Id<BuildingDef>;
pub type RealmId = Id<RealmDef>;
pub type SpellId = Id<SpellDef>;
pub type ItemId = Id<ItemDef>;
pub type ItemPowerId = Id<ItemPowerDef>;
pub type LevelPathId = Id<LevelPathDef>;
