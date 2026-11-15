//! Rendering the original's XMI songs on an emulated Roland MT-32 (`mt32-rs`, LGPL-2.1-or-later),
//! shared by the two programs of this folder: `xmi-play` (listen to a song) and `f4x-music` (render
//! every song to OGG for the game; started by `f4x-setup`).
//!
//! This is its own workspace and not part of `crates/`: `mt32-rs` is LGPL and the game (CC0) must not
//! link it (task 363, `docs/installer-design.md` 3.3). The programs are separate executables.
//!
//! Tests: `wav_bytes` and the OGG round trip below, `xmi` on songs built byte by byte; the rendering
//! itself needs the MT-32 ROMs (not in the repository) and is tried by hand.

pub mod ogg;
pub mod render;
pub mod xmi;
