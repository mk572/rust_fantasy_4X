//! Test support, for this crate's tests and (with the `testutil` feature) for every crate above it:
//! a folder of its own for one test. Tests that write files use this instead of a fixed name in
//! the temp folder, so that two test runs at the same time (two `cargo test`, nextest) and two tests
//! of one run never share a folder.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// What a test returns when it uses `?` instead of `unwrap`: the error is the failure message.
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The game's own content folder (`crates/f4x-def/content`), for a test that loads it in layers or
/// reads a file of it.
pub fn shipped_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

/// The game's own content folder, loaded by the real loader (checks, names and messages
/// included), once per test process. Every test in the workspace gets its content here; a test
/// that needs its own copy (a `Game` owns its content) clones it.
pub fn shipped() -> &'static crate::content::Content {
    static SHIPPED: std::sync::OnceLock<crate::content::Content> = std::sync::OnceLock::new();
    SHIPPED.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
        crate::load::load_content(&dir).expect("the shipped content loads")
    })
}

/// Whether the deep run is on (`F4X_DEEP=1`, `scripts/test_deep.sh`). A normal run, the one after each
/// change, tries the ends of a range and the default; the deep run tries every value, every world, every
/// damage, to find where something breaks. A test that sweeps takes its values from [`spread`] and its
/// size from here.
pub fn deep() -> bool {
    std::env::var_os("F4X_DEEP").is_some_and(|v| v != "0" && !v.is_empty())
}

/// The values of a sweep to try: all of them in the deep run, else the lowest, the default and the
/// highest (each once, in order). `all` is in ascending order.
pub fn spread<T: Clone + PartialEq>(all: impl IntoIterator<Item = T>, default: &T) -> Vec<T> {
    let all: Vec<T> = all.into_iter().collect();
    if deep() {
        return all;
    }
    let mut out: Vec<T> = Vec::new();
    for v in all
        .first()
        .into_iter()
        .chain(Some(default))
        .chain(all.last())
    {
        if !out.contains(v) {
            out.push(v.clone());
        }
    }
    out
}

/// How many of `n` things to try: all of them in the deep run, else `normal`.
pub fn sample(n: usize, normal: usize) -> usize {
    if deep() {
        n
    } else {
        normal.min(n)
    }
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
