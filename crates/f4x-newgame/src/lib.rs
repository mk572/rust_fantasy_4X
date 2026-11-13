//! Making a new game: the map generator, the placement of towers, nodes, lairs and neutral cities,
//! the wizards and their start, everything that runs once when a game begins and never again.
//! It builds a [`f4x_core::game::Game`] through core's public API; core never calls it.
//!
//! Tests: in each module, and in `tests/it`.

pub mod computer_wizard;
pub mod lair;
pub mod main_realm_spell;
pub mod mapgen;
pub mod neutral;
pub mod node;
pub mod place;
pub mod setup;
pub mod temperament;
