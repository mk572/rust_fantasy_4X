//! The features of the map: towers, nodes, lairs and the other places.
//!
//! Tests: none, because it only declares modules.

pub use f4x_def::feature as def;
pub mod entity;
pub mod lair;
pub mod node;
#[cfg(test)]
mod testutil;
