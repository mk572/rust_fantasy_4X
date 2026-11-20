//! Immunities and resistance in combat: the rules are in `f4x-def` (`rules/immunity.rs`) and the die in
//! `f4x-battle` (`resistance.rs`); this file keeps the old paths.
//!
//! Tests: in battle/immunity/tests.rs.

pub use f4x_battle::resistance::resistance_failure;
pub use f4x_def::rules::attack_nature::{delivery, AttackNature, Mode};
pub use f4x_def::rules::immunity::{effective_defense, effective_resistance, Immunities};
pub use f4x_def::rules::realm::Realm;

#[cfg(test)]
mod tests;
