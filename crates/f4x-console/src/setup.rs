//! Starting a game: the setup lives in the core crate; the console re-exports it.
//!
//! Tests: none, because it only re-exports the core setup, whose tests are in the core crate.

pub use f4x_core::game::setup::*;
