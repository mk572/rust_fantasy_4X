//! The rules framework: definitions, registries, stats and effects. The definitions are in
//! `f4x-def` and re-exported here under their old paths; `exec` runs the effects. See
//! `docs/rules-framework.md`.
//!
//! Tests: none, because it only declares modules.

pub mod exec;

pub use f4x_def::rules::*;
pub use f4x_def::rules::{defs, effects, ids, lair, registry, stats};
