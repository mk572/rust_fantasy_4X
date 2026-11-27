//! The game's random numbers: the generator and the die live in `f4x-battle`.
//!
//! Tests: none, because it only re-exports.

pub use f4x_battle::rng::{random, GameRng};
/// For whoever draws raw values from a [`GameRng`].
pub use rand::RngCore;
