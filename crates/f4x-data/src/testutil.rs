//! Test support: the folder of the shipped content.
//!
//! Tests: none, because it is only used by the tests of the other files.

use std::path::{Path, PathBuf};

/// The game's own `content/` folder.
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// The client's own `content/` folder, which holds the names and messages.
pub fn client_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-client/content")
}
