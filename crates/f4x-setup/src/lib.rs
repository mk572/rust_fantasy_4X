//! The setup of the game from the player's own copy of the original (`docs/installer-design.md`).
//!
//! * [`original`] finds the original's folder and checks that it is the right one.
//! * [`install`] extracts the pictures into the data folder (`assets-local/`), leaving out what the
//!   game never draws.
//! * [`say`] holds the player-facing sentences, read by key from `crates/f4x-lang/content/text/`.
//! * [`flow`] is the order of things: welcome, choosing and checking the folder, the work. It talks
//!   to the player through the [`flow::Ui`] trait, so it is tested without windows.
//!
//! Nothing here uploads or copies anything from the original except into the player's own data
//! folder. Tests: in the modules, on small fixture folders; the real original is tried by
//! `tests/it/real_original.rs`, which is ignored by default and skips itself when
//! `reference/mom-classic` is not there. The windows themselves (`gui` in the binary) are tried by
//! hand.

pub mod flow;
pub mod install;
pub mod original;
pub mod say;
