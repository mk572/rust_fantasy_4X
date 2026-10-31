//! The reference-only game of `soak-versus` does not change while only the live AI is edited, so
//! its result is kept in `<out>/cache/` and reused. The key is a fingerprint of everything that
//! decides that game: the rules, the content, the reference AI, the soak's own code and the
//! options.
//!
//! Tests: none, because it hashes the repository's files and writes the kept games; `f4x-tools soak-versus` is its check.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::soak::{GameResult, Options};

/// The folders and files whose contents decide the reference-only game.
const SOURCES: [&str; 6] = [
    "crates/f4x-core/src",
    "crates/f4x-data/src",
    "crates/f4x-ai-ref/src",
    "crates/f4x-tools/src",
    "Cargo.lock",
    "content",
];

fn hash_path(path: &Path, hasher: &mut DefaultHasher) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for entry in entries {
            hash_path(&entry, hasher);
        }
    } else if let Ok(bytes) = std::fs::read(path) {
        path.hash(hasher);
        bytes.hash(hasher);
    }
}

/// The fingerprint of the reference-only game of `seed` under these options.
fn key(opts: &Options, seed: u64) -> String {
    let mut hasher = DefaultHasher::new();
    for source in SOURCES {
        hash_path(Path::new(source), &mut hasher);
    }
    hash_path(&opts.dir, &mut hasher);
    (
        seed,
        opts.turns,
        opts.players,
        opts.grant_mastery,
        opts.snapshots_every,
    )
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn file(out: &Path, opts: &Options, seed: u64) -> PathBuf {
    out.join("cache")
        .join(format!("ref-all-seed{seed}-{}.ron", key(opts, seed)))
}

/// The kept result of the reference-only game, if there is one for this build.
pub fn load(out: &Path, opts: &Options, seed: u64) -> Option<GameResult> {
    let text = std::fs::read_to_string(file(out, opts, seed)).ok()?;
    ron::from_str(&text).ok()
}

pub fn store(out: &Path, opts: &Options, seed: u64, result: &GameResult) -> Result<(), String> {
    let path = file(out, opts, seed);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = ron::to_string(result).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
