//! The rules framework: definitions, registries, stats and effects.
//! See `docs/rules-framework.md`.
//!
//! Tests: none, because it only declares modules.

pub mod defs;
pub mod effects;
pub mod exec;
pub mod ids;
pub mod lair;
pub mod registry;
pub mod stats;

pub use defs::*;
pub use ids::Id;
pub use registry::{Def, Registry};
pub use stats::{ModOp, Modifier, Source, Stat};

pub type RaceId = Id<RaceDef>;
pub type UnitId = Id<UnitDef>;
pub type AbilityId = Id<AbilityDef>;
pub type BuildingId = Id<BuildingDef>;
pub type RealmId = Id<RealmDef>;
pub type SpellId = Id<SpellDef>;
pub type ItemId = Id<ItemDef>;
pub type LevelPathId = Id<LevelPathDef>;
