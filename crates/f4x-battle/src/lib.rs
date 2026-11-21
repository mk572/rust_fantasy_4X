//! The tactical battle: the rules and the field of a fight, below the simulation of the world.
//! Reads `f4x-def`; `f4x-core` builds on it (`docs/design.md` section 2).
//!
//! Tests: none, because it only declares modules.

pub mod attack;
pub mod battle;
pub mod battlemagic;
pub mod map;
pub mod power;
pub mod resistance;
pub mod rng;
pub mod side;
pub mod special;
#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
pub mod unit;
pub mod weapon;

pub use side::Side;
pub use unit::{
    BattleUnit, Confusion, Fate, FieldBonus, HeroSheet, ItemCharge, Lasting, Sheet, Stats, Status,
    UnitCasting, WarpKind, WizardSheet, RANGED_ATTACK_COST,
};
