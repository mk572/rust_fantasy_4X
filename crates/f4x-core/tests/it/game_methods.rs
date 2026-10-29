//! Measures how many methods `Game` has per source file (task 183). A guideline, not a rule: the
//! test never fails on the counts. It compares them with `game_method_limits.txt` (the counts
//! at the last baseline) and with `GUIDELINE`, and prints what is over either; see it with
//! `cargo test -p f4x-core --test it game_methods -- --nocapture`.
//!
//! When methods move out of `Game`, rewrite the baseline with
//! `F4X_WRITE_LIMITS=1 cargo test -p f4x-core --test it game_methods`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// About how many methods on `Game` one file should have; `game/access.rs` and `game/cheat.rs` are
/// deliberate surfaces and may go beyond it.
const GUIDELINE: usize = 12;

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|n| n != "tests.rs")
        {
            out.push(path);
        }
    }
}

/// Whether a line opens an `impl` block for `Game` itself (a trait implemented for `Game` counts).
fn opens_game_impl(line: &str) -> bool {
    let Some(head) = line.strip_prefix("impl ") else {
        return false;
    };
    let Some(head) = head.trim_end().strip_suffix('{') else {
        return false;
    };
    let target = head.rsplit(" for ").next().unwrap_or(head).trim();
    matches!(
        target,
        "Game" | "crate::game::Game" | "f4x_core::game::Game"
    )
}

/// Methods in `impl Game` blocks of one file: `fn` items at the block's top level (four spaces in).
fn count_methods(text: &str) -> usize {
    let mut inside = false;
    let mut count = 0;
    for line in text.lines() {
        if !inside {
            inside = opens_game_impl(line);
        } else if line.starts_with('}') {
            inside = false;
        } else if let Some(item) = line.strip_prefix("    ") {
            if item.starts_with(' ') {
                continue;
            }
            let item = item
                .strip_prefix("pub(crate) ")
                .or_else(|| item.strip_prefix("pub "))
                .unwrap_or(item);
            if item.starts_with("fn ") || item.starts_with("const fn ") {
                count += 1;
            }
        }
    }
    count
}

fn counts() -> BTreeMap<String, usize> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files);
    let mut out = BTreeMap::new();
    for path in files {
        let n = count_methods(&fs::read_to_string(&path).unwrap());
        if n > 0 {
            let rel = path
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, n);
        }
    }
    out
}

const HEADER: &str = "# Methods in `impl Game` blocks per source file of f4x-core at the last baseline (task 183).\n\
# A measurement, read by tests/game_methods.rs, which reports growth and files over the guideline.\n\
# Regenerate: F4X_WRITE_LIMITS=1 cargo test -p f4x-core --test it game_methods\n";

#[test]
fn report_game_methods() {
    let limits_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("game_method_limits.txt");
    let now = counts();

    if std::env::var_os("F4X_WRITE_LIMITS").is_some() {
        let mut text = String::from(HEADER);
        for (file, n) in &now {
            text.push_str(&format!("{file} {n}\n"));
        }
        fs::write(&limits_path, text).unwrap();
        return;
    }

    let baseline: BTreeMap<String, usize> = fs::read_to_string(&limits_path)
        .map(|text| {
            text.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .filter_map(|l| {
                    let (file, n) = l.rsplit_once(' ')?;
                    Some((file.to_string(), n.parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();

    println!(
        "methods on Game: {} in {} files",
        now.values().sum::<usize>(),
        now.len()
    );
    for (file, &n) in &now {
        let before = baseline.get(file).copied().unwrap_or(0);
        if n > before {
            println!("  grew: {file} {before} -> {n}");
        }
    }
    for (file, &n) in &now {
        if n > GUIDELINE {
            println!("  over {GUIDELINE}: {file} ({n})");
        }
    }
}

#[test]
fn counter_reads_game_impls_only() {
    let text = "\
impl Game {
    pub fn a(&self) {}
    pub(crate) fn b(&self) {
        fn inner() {}
    }
    fn c(&self) {}
}

impl Other {
    fn d(&self) {}
}

impl Trait for Game {
    fn e(&self) {}
}

impl crate::game::Game {
    pub fn f(&self) {}
}
";
    assert_eq!(count_methods(text), 5);
}
