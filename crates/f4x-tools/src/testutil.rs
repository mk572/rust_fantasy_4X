//! The shipped content for the tests, read once per run.
//!
//! Tests: it is itself test support; its users are the tests of `metrics` and `screens`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use f4x_core::content::Content;

/// The folder of the shipped content.
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

pub fn shipped() -> &'static Content {
    static CONTENT: OnceLock<Content> = OnceLock::new();
    CONTENT
        .get_or_init(|| f4x_data::load_content(&content_dir()).expect("the shipped content loads"))
}
