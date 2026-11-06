//! The static definitions of the game: races, units, abilities, buildings, spells, items, realms,
//! terrains, features and the registries that hold them. They are data read from `content/*.ron`;
//! this crate imports nothing of the game, and `f4x-core` builds on it.
//!
//! Tests: with each module.

pub mod feature;
pub mod map;
pub mod rules;
#[cfg(any(test, feature = "testutil"))]
pub mod testdir;
pub mod text;
