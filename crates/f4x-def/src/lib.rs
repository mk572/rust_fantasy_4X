//! The static definitions of the game: races, units, abilities, buildings, spells, items, realms,
//! terrains, features and the registries that hold them. They are data read from `crates/f4x-def/content/*.ron`;
//! this crate imports nothing of the game, and `f4x-core` builds on it.
//!
//! Tests: with each module.

/// The game's own rules files, from the root of the repository (the folder the game and its tools
/// run from). A mod's folder is layered over it (`load::load_content_layers`).
pub const SHIPPED_CONTENT: &str = "crates/f4x-def/content";

pub mod content;
pub mod feature;
pub mod half_int;
pub mod help;
pub mod hero;
pub mod load;
pub mod map;
pub mod names;
#[cfg(any(test, feature = "testutil"))]
pub mod original;
pub mod rules;
pub mod settings;
#[cfg(any(test, feature = "testutil"))]
pub mod testdir;
pub mod text;
