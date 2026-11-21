//! Magic and spellcasting that belong to battles: the battlefield enchantments, the casting wizard,
//! why a cast is refused and what a spell does in a battle. What casting does to the world is in
//! `f4x-core`'s `battle/`.
//!
//! Tests: none, because it only declares modules.

pub mod aim;
pub mod caster;
pub mod casting;
pub mod error;
pub mod field;
pub mod spell;
pub mod spell_type;
pub mod unit_cast;
pub mod vortex;
