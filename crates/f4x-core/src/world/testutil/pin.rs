//! Pinned digests: a test plays something, writes one line for each checkpoint and holds the lines
//! against a file (`docs/f4x-core-test-design.md`, class 8).
//!
//! Tests: through the tests that use it (`c8_drift.rs` here, `drift.rs` in `f4x-master`).

use std::collections::BTreeMap;
use std::path::Path;

/// The lines of a pin file, by checkpoint; comments and empty lines left out. A file that cannot be
/// read has none.
fn read(file: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn fields(line: &str) -> BTreeMap<&str, &str> {
    line.split(' ').filter_map(|f| f.split_once('=')).collect()
}

/// Hold the lines of `now` (checkpoint, then `field=value` words) against the pins of `file`, and
/// fail naming the checkpoints and fields that moved. With `F4X_REFRESH_PINS` set the file is
/// written instead, under `header`; `refresh` is the command that does so, for the failure.
#[track_caller]
pub fn check(file: &Path, header: &str, refresh: &str, now: &BTreeMap<String, String>) {
    if std::env::var_os("F4X_REFRESH_PINS").is_some() {
        let mut out = header.to_string();
        for (k, v) in now {
            out.push_str(&format!("{k}: {v}\n"));
        }
        std::fs::write(file, out).expect("the pins are written");
        return;
    }
    let pinned = read(file);
    let name = file.display();
    assert!(!pinned.is_empty(), "{name} is missing or empty: {refresh}");
    let mut moved = Vec::new();
    for (key, line) in now {
        match pinned.get(key) {
            None => moved.push(format!("{key}: not pinned")),
            Some(was) if was != line => {
                let (a, b) = (fields(was), fields(line));
                let changed: Vec<&str> = b
                    .iter()
                    .filter(|(k, v)| a.get(*k) != Some(v))
                    .map(|(k, _)| *k)
                    .chain(a.keys().filter(|k| !b.contains_key(*k)).copied())
                    .collect();
                moved.push(format!("{key}: moved in {}", changed.join(", ")));
            }
            Some(_) => {}
        }
    }
    for key in pinned.keys().filter(|k| !now.contains_key(*k)) {
        moved.push(format!("{key}: pinned but no longer played"));
    }
    assert!(
        moved.is_empty(),
        "the game moved ({} of {} checkpoints). If that is meant, regenerate and review the diff of {name}:\n\
         {refresh}\n{}",
        moved.len(),
        now.len(),
        moved.join("\n")
    );
}
