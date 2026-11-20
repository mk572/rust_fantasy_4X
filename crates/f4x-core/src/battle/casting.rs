//! Casting in a battle: the types are in `f4x-battle` (`battlemagic`); this file keeps the old paths.
//!
//! Tests: none, because it only re-exports.

pub use f4x_battle::battlemagic::caster::{range_percent, BattleCaster, SpellAim};
pub use f4x_battle::battlemagic::casting::*;
pub use f4x_battle::battlemagic::error::SpellError;
