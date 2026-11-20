//! Tactical battles.
//!
//! A [`Battle`] is a self-contained record, not part of the overland world: it is built from
//! the units that meet (their statistics are copied at the start, with every modifier
//! applied), played through [`Battle::apply`] with [`BattleCommand`]s, and when it ends the
//! game takes the results back (`game.rs`). Because it needs nothing from the world, battles
//! can be tested, replayed and driven by an AI on their own.
//!
//! Sides alternate: the attacker plays all its units, then the defender, and a new turn
//! starts when the defender is done. A side loses when it has no active unit left.
//!
//! The battle itself is in `f4x-battle`; this module holds what ties it to the game and re-exports the
//! rest at the old paths.
//!
//! Tests: in battle/tests.rs.

pub use f4x_battle::attack;
pub use f4x_battle::battle::*;
pub use f4x_battle::battle::{content_ref, legal_command, load};
pub use f4x_battle::battlemagic::field;
pub use f4x_battle::battlemagic::vortex;
pub use f4x_battle::map;
pub use f4x_battle::map::Pos;
pub use f4x_battle::power;
pub use f4x_battle::special;
pub use f4x_battle::{
    BaseStats, BattleUnit, Confusion, Fate, FieldBonus, ItemCharge, Lasting, Side, Status,
    UnitCasting, WarpKind, RANGED_ATTACK_COST,
};

pub mod casting;
pub mod forecast;
pub mod game;
pub mod immunity;
pub mod info;
pub mod save;
pub mod spell;
pub mod spoils;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ability_tests;
