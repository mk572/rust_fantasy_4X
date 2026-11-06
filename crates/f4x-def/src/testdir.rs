//! Test support, for this crate's tests and (with the `testutil` feature) for every crate above it:
//! a folder of its own for one test. Tests that write files use this instead of a fixed name in
//! the temp folder, so that two test runs at the same time (two `cargo test`, nextest) and two tests
//! of one run never share a folder.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// What a test returns when it uses `?` instead of `unwrap`: the error is the failure message.
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The game's own `content/` folder, loaded by the real loader (checks, names and messages
/// included), once per test process. Every test in the workspace gets its content here; a test
/// that needs its own copy (a `Game` owns its content) clones it.
pub fn shipped() -> &'static crate::content::Content {
    static SHIPPED: std::sync::OnceLock<crate::content::Content> = std::sync::OnceLock::new();
    SHIPPED.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        crate::load::load_content(&dir).expect("the shipped content loads")
    })
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// An empty, new folder in the temp folder, removed again when this is dropped. It derefs to
/// its path.
pub struct TestDir(PathBuf);

impl TestDir {
    /// `label` names what the folder is for; the process id and a counter make the name unique.
    pub fn new(label: &str) -> TestDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("f4x-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp folder can be written");
        TestDir(path)
    }
}

impl Deref for TestDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_folders_with_one_label_are_different_and_empty_and_removed_on_drop() -> TestResult {
        let a = TestDir::new("same");
        let b = TestDir::new("same");
        assert_ne!(*a, *b);
        std::fs::write(a.join("x"), b"x")?;
        assert_eq!(std::fs::read_dir(&*b)?.count(), 0);
        let gone = a.to_path_buf();
        drop(a);
        assert!(!gone.exists());
        Ok(())
    }
}
