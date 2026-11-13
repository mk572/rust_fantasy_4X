//! The words of the game, apart from any client: the messages by key (`text`: count, plural and
//! gender forms, `Texts`), the names of the rules' things and the name lists of a new game
//! (`names`), and the files they are read from, `content/text/<lang>/` (`docs/text.md`).
//! A client draws the words its own way (fonts and glyphs stay in the client); another client
//! reads the same files.
//!
//! Tests: with each module; the check that the client's code and the keys agree is in
//! `f4x-client/tests/it/text.rs`.

/// The language files, from the root of the repository (the folder the game and its tools run
/// from): `text/<lang>/*.ron`.
pub const LANG_CONTENT: &str = "crates/f4x-lang/content";

pub mod names;
pub mod text;
