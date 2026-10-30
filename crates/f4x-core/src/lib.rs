//! Deterministic game simulation. No I/O, no rendering.
//!
//! All state changes go through [`game::Game::apply`] as [`game::Command`]s, which yield
//! [`game::Event`]s. The command log plus the seed fully reproduces a game.
//!
//! Tests: none, because it only declares modules.

pub mod battle;
pub mod city;
pub mod content;
pub mod diplomacy;
pub mod feature;
pub mod game;
pub mod help;
pub mod hero;
pub mod item;
pub mod map;
pub mod player;
pub mod rules;
pub mod spell;
pub mod testdir;
pub mod text;
pub mod threat;
pub mod turn;
pub mod unit;
pub mod wizard;
pub mod world_event;
