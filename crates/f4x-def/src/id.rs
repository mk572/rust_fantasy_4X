//! Small identifiers every layer shares: who a player is and the creation number of a unit.
//!
//! Tests: none, because it only holds data types without behaviour.

use serde::{Deserialize, Serialize};

/// Who a player is outside the world (fog, diplomacy, map owners, saves, commands). A component of
/// the player entity; never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// The creation number of a unit, kept for a stable order (see `f4x-core`'s `game::serial`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Serial(pub u64);
