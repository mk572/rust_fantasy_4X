//! Test support: the shipped content folder and a result type for `?` in tests.
//!
//! Tests: none; it is only used by the tests of the other files.

use std::path::{Path, PathBuf};

/// What a test returns when it uses `?` instead of `unwrap`.
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The client's own `content/` folder (`crates/f4x-client/content`).
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
}
