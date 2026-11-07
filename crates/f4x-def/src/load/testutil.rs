//! Test support: the folder of the shipped content.
//!
//! Tests: none, because it is only used by the tests of the other files.

use std::path::{Path, PathBuf};

/// The game's own `content/` folder.
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}
