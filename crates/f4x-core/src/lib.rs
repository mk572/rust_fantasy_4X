//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`game::Game::apply`] as [`game::Command`]s, which yield
//! [`game::Event`]s. The command log plus the seed fully reproduces a game.
//!
//! Tests: none, because it only declares modules.

/// The handle of a unit or a city in the world (`hecs::Entity`), for callers that hold one without
/// depending on `hecs`.
pub use hecs::Entity;

pub mod battle;
#[cfg(any(test, feature = "testutil"))]
pub mod check;
pub mod city;
pub mod diplomacy;
pub mod fast;
pub mod feature;
pub mod game;
pub mod hero;
pub mod hook;
pub mod item;
pub mod map;
pub mod path_grid;
pub mod player;
pub mod rules;
pub mod spell;
pub mod threat;
pub mod turn;
pub mod unit;
pub mod wizard;
pub mod world_event;
