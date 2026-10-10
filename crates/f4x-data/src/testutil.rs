//! Helpers for tests that load, edit and re-load content packs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use f4x_core::content::Content;

use crate::{load_packs, ContentError};

pub fn base_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/base")
}

/// A scratch directory removed on drop.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "f4x-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    pub fn write(&self, name: &str, content: &str) -> &Self {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        self
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copy the base pack, apply `(file, from, to)` text replacements, and load it.
pub fn load_edited_base(edits: &[(&str, &str, &str)]) -> Result<Content, ContentError> {
    let scratch = Scratch::new();
    for entry in std::fs::read_dir(base_dir()).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let mut text = std::fs::read_to_string(entry.path()).unwrap();
        for (f, from, to) in edits.iter().filter(|(f, ..)| *f == name) {
            assert!(text.contains(from), "{f} does not contain {from:?}");
            text = text.replace(from, to);
        }
        scratch.write(&name, &text);
    }
    load_packs(std::slice::from_ref(&scratch.0))
}

/// Problems reported when loading the edited base pack (empty if it loads).
pub fn problems_of(edits: &[(&str, &str, &str)]) -> Vec<String> {
    match load_edited_base(edits) {
        Ok(_) => Vec::new(),
        Err(ContentError::Invalid(p)) => p,
        Err(e) => panic!("expected validation problems, got {e}"),
    }
}

pub fn has(problems: &[String], needle: &str) -> bool {
    problems.iter().any(|m| m.contains(needle))
}
