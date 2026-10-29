//! A folder of its own for one test. Tests that write files use this instead of a fixed name in
//! the temp folder, so that two test runs at the same time (two `cargo test`, nextest) and two tests
//! of one run never share a folder.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

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
    fn two_folders_with_one_label_are_different_and_empty_and_removed_on_drop() {
        let a = TestDir::new("same");
        let b = TestDir::new("same");
        assert_ne!(*a, *b);
        std::fs::write(a.join("x"), b"x").unwrap();
        assert_eq!(std::fs::read_dir(&*b).unwrap().count(), 0);
        let gone = a.to_path_buf();
        drop(a);
        assert!(!gone.exists());
    }
}
