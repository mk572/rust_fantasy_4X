//! The shipped content, for the tests of this folder.

use std::path::{Path, PathBuf};

use f4x_core::content::Content;

/// The game's own `content/` folder.
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

pub fn content() -> Content {
    f4x_data::load_content(&content_dir()).expect("the shipped content loads")
}
