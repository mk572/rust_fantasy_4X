//! The guard of the content files: each crate that has `.ron` files records the SHA-256 of each in
//! `content.sha256` at its root, and a test of the crate holds the files against the record. A
//! value changed by hand or by a script that should not have touched it breaks the test; a change
//! made on purpose is recorded with `python3 scripts/ron_hashes.py --update` and the new record
//! goes in the same commit as the change, so the history shows who changed what (task 653).
//!
//! The record has one line per file, `<hash>  <path from the crate's folder>`, sorted by path,
//! for every `.ron` file under the crate's folder except `target/`. `scripts/ron_hashes.py` writes
//! the same file by the same rules.
//!
//! Tests: below.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// The name of the record, in the crate's folder.
pub const RECORD: &str = "content.sha256";

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn collect(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                collect(&path, root, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "ron") {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(name, hex(&bytes));
        }
    }
    Ok(())
}

/// The hash of every `.ron` file under `crate_dir`, by path from it.
pub fn hashes_of(crate_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    collect(crate_dir, crate_dir, &mut out)?;
    Ok(out)
}

/// The record in `crate_dir`: path to hash.
pub fn recorded(crate_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let file = crate_dir.join(RECORD);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    text.lines()
        .map(|line| {
            line.split_once("  ")
                .map(|(hash, path)| (path.to_string(), hash.to_string()))
                .ok_or_else(|| format!("{RECORD}: bad line \"{line}\""))
        })
        .collect()
}

/// What differs between the record and the files: the changed, the added and the removed.
pub fn differences(crate_dir: &Path) -> Result<Vec<String>, String> {
    let now = hashes_of(crate_dir)?;
    let then = recorded(crate_dir)?;
    let mut out = Vec::new();
    for (path, hash) in &now {
        match then.get(path) {
            None => out.push(format!("added: {path}")),
            Some(old) if old != hash => out.push(format!("changed: {path}")),
            Some(_) => {}
        }
    }
    out.extend(
        then.keys()
            .filter(|p| !now.contains_key(*p))
            .map(|p| format!("removed: {p}")),
    );
    Ok(out)
}

/// The test of a crate: fails with the list of the files that differ from the record.
pub fn assert_unchanged(crate_dir: &Path) {
    let differ = differences(crate_dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        differ.is_empty(),
        "the .ron files of {} differ from {RECORD}:\n  {}\nIf the change is meant, record it with \
         `python3 scripts/ron_hashes.py --update` and commit {RECORD} with it; if not, find out \
         who changed the files.",
        crate_dir.display(),
        differ.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::{TestDir, TestResult};

    #[test]
    fn a_changed_added_and_removed_file_are_each_reported() -> TestResult {
        let dir = TestDir::new("content-hash");
        std::fs::create_dir_all(dir.join("content"))?;
        std::fs::write(dir.join("content/a.ron"), "1")?;
        std::fs::write(dir.join("content/b.ron"), "2")?;
        let record: String = hashes_of(&dir)?
            .iter()
            .map(|(p, h)| format!("{h}  {p}\n"))
            .collect();
        std::fs::write(dir.join(RECORD), record)?;
        assert!(differences(&dir)?.is_empty());
        std::fs::write(dir.join("content/a.ron"), "changed")?;
        std::fs::remove_file(dir.join("content/b.ron"))?;
        std::fs::write(dir.join("content/c.ron"), "3")?;
        assert_eq!(
            differences(&dir)?,
            [
                "changed: content/a.ron",
                "added: content/c.ron",
                "removed: content/b.ron"
            ]
        );
        Ok(())
    }

    #[test]
    fn the_files_of_this_crate_are_the_recorded_ones() {
        assert_unchanged(Path::new(env!("CARGO_MANIFEST_DIR")));
    }
}
