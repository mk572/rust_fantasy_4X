//! The tactical battle: the rules and the field of a fight, below the simulation of the world.
//! Reads `f4x-def`; `f4x-core` builds on it (`docs/design.md` section 2).
//!
//! Tests: none, because it only declares modules.

pub mod attack;
pub mod map;
pub mod resistance;
pub mod rng;
pub mod side;
pub mod special;
pub mod unit;
pub mod weapon;

pub use side::Side;
pub use unit::{
    BaseStats, BattleUnit, Confusion, Fate, FieldBonus, ItemCharge, Lasting, Status, UnitCasting,
    WarpKind, RANGED_ATTACK_COST,
};
