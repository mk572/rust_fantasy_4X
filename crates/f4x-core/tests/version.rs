//! The game version is set once, in the workspace `Cargo.toml` (task 307). The docs that state it
//! must say the same, so that nobody reading `CLAUDE.md` or the README is told a stale number.

use std::fs;
use std::path::Path;

fn states_version(file: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let line = format!("**Version: {}**", env!("CARGO_PKG_VERSION"));
    assert!(
        text.contains(&line),
        "{file} must contain the line `{line}` (the version in Cargo.toml)"
    );
}

#[test]
fn the_docs_state_the_cargo_version() {
    for file in ["CLAUDE.md", "README.md", "docs/ecs-structure.md"] {
        states_version(file);
    }
}
