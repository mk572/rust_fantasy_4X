//! `f4x-tools game-methods`: how many methods `Game` has per source file of f4x-core. A report on
//! how well the code is divided, not a test and not a limit: it prints the files over `GUIDELINE`
//! and exits with success whatever the counts are.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// About how many methods on `Game` one file should have; `game/access.rs` and `game/cheat.rs` are
/// deliberate surfaces and may go beyond it.
const GUIDELINE: usize = 12;

pub fn run(_args: &[String]) -> Result<(), String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-core/src");
    let counts = counts(&src)?;
    let mut ranked: Vec<_> = counts.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!(
        "methods on Game: {} in {} files",
        counts.values().sum::<usize>(),
        counts.len()
    );
    for (file, n) in ranked.iter().filter(|(_, &n)| n > GUIDELINE) {
        println!("  over {GUIDELINE}: {file} ({n})");
    }
    Ok(())
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            source_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|n| n != "tests.rs")
        {
            out.push(path);
        }
    }
    Ok(())
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

fn counts(src: &Path) -> Result<BTreeMap<String, usize>, String> {
    let mut files = Vec::new();
    source_files(src, &mut files)?;
    let mut out = BTreeMap::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let n = count_methods(&text);
        if n > 0 {
            let rel = path
                .strip_prefix(src)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, n);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
