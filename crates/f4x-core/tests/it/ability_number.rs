//! The ability numbers written into the battle and the battle AI are the numbers of
//! `abilities.ron`, each with its alias as a comment: `Ability::new(23 /* first_strike */)`.
//! A number that names another ability than its comment, or has no comment, fails here.

use f4x_core::rules::Ability;
use std::fs;
use std::path::{Path, PathBuf};

const DIRS: &[&str] = &["crates/f4x-core/src/battle", "crates/f4x-ai/src"];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" {
                sources(&path, out);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
}

/// The number and the comment after each `Ability::new(` with a written number.
fn written(text: &str) -> Vec<(u16, Option<String>)> {
    let mut found = Vec::new();
    for part in text.split("Ability::new(").skip(1) {
        let rest = part.trim_start();
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(num) = digits.parse::<u16>() else {
            continue;
        };
        let after = rest[digits.len()..].trim_start();
        let after = after.strip_prefix(',').unwrap_or(after).trim_start();
        let alias = after
            .strip_prefix("/*")
            .and_then(|c| c.split_once("*/"))
            .map(|(alias, _)| alias.trim().to_string());
        found.push((num, alias));
    }
    found
}

#[test]
fn every_written_ability_number_has_the_alias_of_abilities_ron() {
    let content = f4x_def::testdir::shipped();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in DIRS {
        sources(&root.join(dir), &mut files);
    }
    assert!(!files.is_empty(), "no source files found");
    let mut wrong = Vec::new();
    let mut count = 0;
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        for (num, alias) in written(&text) {
            count += 1;
            let id = content
                .rules
                .abilities
                .try_get(Ability::new(num))
                .map(|a| a.id.as_str());
            if id.is_none() || alias.as_deref() != id {
                wrong.push(format!(
                    "{}: {num} is `{}`, comment says {alias:?}",
                    file.display(),
                    id.unwrap_or("no ability"),
                ));
            }
        }
    }
    assert!(
        count > 50,
        "only {count} numbers found: is the scan broken?"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
