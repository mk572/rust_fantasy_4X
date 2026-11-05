//! Everything that reads the original game's files and writes the client's local assets (pictures,
//! sounds, music, fonts), and the definitions of which game element uses which asset
//! (`crates/f4x-client/content/spell_art.ron`, `crates/f4x-client/content/sound.ron`). No game logic: this crate depends on no other
//! crate of the workspace. The client depends on it for the definitions only
//! (`default-features = false`); the extraction code is the `extract` feature, on by default, so the
//! client does not link `png` or `fontdue`.
//!
//! Tests: in the modules that have logic apart from files (`art`, `audio`, `image`, `lbx`,
//! `refile`, `screens`, `sound`, `spell_art`).

#[cfg(feature = "extract")]
pub mod accent;
#[cfg(feature = "extract")]
pub mod art;
#[cfg(feature = "extract")]
pub mod audio;
#[cfg(feature = "extract")]
pub mod bdf;
#[cfg(feature = "extract")]
pub mod font_tool;
#[cfg(feature = "extract")]
pub mod fonts;
#[cfg(feature = "extract")]
pub mod image;
#[cfg(feature = "extract")]
pub mod lbx;
#[cfg(feature = "extract")]
pub mod map_art;
#[cfg(feature = "extract")]
pub mod refile;
#[cfg(feature = "extract")]
pub mod screens;
pub mod sound;
pub mod spell_art;
#[cfg(test)]
mod testutil;
pub mod unit_art;
pub mod wizard_art;
