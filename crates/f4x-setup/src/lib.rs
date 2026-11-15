//! The setup of the game from the player's own copy of the original (`docs/installer-design.md`).
//!
//! * [`original`] finds the original's folder and checks that it is the right one.
//! * [`install`] extracts the pictures into the data folder (`assets-local/`), leaving out what the
//!   game never draws.
//!
//! Nothing here uploads or copies anything from the original except into the player's own data
//! folder. Tests: in the modules, on small fixture folders; the real original is tried by
//! `tests/real_original.rs`, which skips itself when `reference/mom-classic` is not there.

pub mod install;
pub mod original;
