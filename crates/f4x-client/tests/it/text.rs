//! The messages in `crates/f4x-lang/content/text/` and the code agree: every key the code names exists, every key
//! is used, and a file holds only the keys of its area (`battle.ron` holds `battle.*`).

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use f4x_def::testdir::TestResult;

/// A literal like "hall.txt" is a file name, not a key.
const FILE_EXTENSIONS: &[&str] = &["ron", "txt", "png", "json"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An area of the messages (a file) and its keys.
type Area = (String, Vec<String>);

/// The area of each file of the English messages, and its keys.
fn written() -> Result<Vec<Area>, Box<dyn Error>> {
    let folder = root().join("crates/f4x-lang/content/text/en");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "ron") {
            files.push(path);
        }
    }
    files.sort();
    let mut areas = Vec::new();
    for path in &files {
        let entries: std::collections::BTreeMap<String, ron::Value> =
            ron::from_str(&std::fs::read_to_string(path)?)?;
        let area = path
            .file_stem()
            .ok_or("a text file has no name")?
            .to_string_lossy()
            .into_owned();
        areas.push((area, entries.into_keys().collect()));
    }
    Ok(areas)
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> TestResult {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The string literals of the code that look like a key of one of the areas.
fn named_in_code(areas: &[String]) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut files = Vec::new();
    for krate in std::fs::read_dir(root().join("crates"))? {
        // Only the crate folders; stray files such as macOS's `.DS_Store` are not crates.
        let path = krate?.path();
        if path.is_dir() {
            sources(&path.join("src"), &mut files)?;
        }
    }
    let mut found = BTreeSet::new();
    for file in files {
        // The tests of the text module itself use made-up keys.
        if file.ends_with("text/tests.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&file)?;
        for part in src.split('"').skip(1).step_by(2) {
            let key = part.split_once('.');
            if key.is_some_and(|(area, rest)| {
                areas.iter().any(|a| a == area)
                    && !rest.is_empty()
                    && !FILE_EXTENSIONS.contains(&rest)
                    && rest.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'
                    })
            }) {
                found.insert(part.to_string());
            }
        }
    }
    Ok(found)
}

#[test]
fn a_file_holds_only_the_keys_of_its_area() -> TestResult {
    for (area, keys) in written()? {
        for key in keys {
            assert!(
                key.starts_with(&format!("{area}.")),
                "{key} is in {area}.ron but is not an {area} key"
            );
        }
    }
    Ok(())
}

#[test]
fn every_key_the_code_names_is_written() -> TestResult {
    let written = written()?;
    let areas: Vec<String> = written.iter().map(|(a, _)| a.clone()).collect();
    let keys: BTreeSet<String> = written.into_iter().flat_map(|(_, k)| k).collect();
    let missing: Vec<_> = named_in_code(&areas)?.difference(&keys).cloned().collect();
    assert!(
        missing.is_empty(),
        "keys the code names but no file has: {missing:?}"
    );
    Ok(())
}

#[test]
fn every_key_that_is_written_is_used() -> TestResult {
    let written = written()?;
    let areas: Vec<String> = written.iter().map(|(a, _)| a.clone()).collect();
    let used = named_in_code(&areas)?;
    let unused: Vec<_> = written
        .into_iter()
        .flat_map(|(_, k)| k)
        .filter(|k| !used.contains(k))
        .collect();
    assert!(unused.is_empty(), "keys no code uses: {unused:?}");
    Ok(())
}

/// Files of the client whose literals are not messages: the command line, the picture files, the
/// developer console, the key-name table, the check of the asset tables (start-up errors about
/// content files, for the developer).
const EXEMPT_FILES: &[&str] = &[
    "text.rs",
    "tests.rs",
    "main.rs",
    "art.rs",
    "console_screen.rs",
    "hotkeys.rs",
    "asset_check.rs",
    "icon_art.rs",
    "client_settings.rs",
    "names.rs",
    "event_message.rs",
];

/// Lines that look like text but are not: number formats, the space between wrapped words, a
/// screenshot fixture, the debug form of an error, the help parser, an error for the developer.
const EXEMPT_LINES: &[&str] = &[
    "{line} {word}",
    "{current} {word}",
    "format!(\"0.{tenths}x\")",
    "{why:?}",
    "t.text.contains(\"Casting Cost\")",
    "starts_with(\"Upkeep\")",
    "grew(\"Waldheim\"",
    "Test city {i}",
    "\"Spearmen\"",
    "{grow:+}",
    "{net_gold:+}",
    "\"Norport\"",
    "\"Barracks\"",
    "unknown event {kind}",
    "\"Fire Bolt\"",
    "{sign}{}",
    "({skill})",
    "{typed}{cursor}",
    "\"Zed\"",
    "build.name = \"You\"",
    "{name}_",
    "is not a preference",
];

/// The same, for the lines the list above cannot name by a fragment.
const EXEMPT_FRAGMENTS: &[&str] = &[
    "[{name}]",
    "+{prod}%",
    "+{gold}%",
    "{first}: {r}",
    "saves/slot",
    // test_app.rs, compiled for tests only: where a test's autosave goes.
    "tests/saves",
    "{year:04}",
    // first_run.rs: a file name, a folder and the AppleScript that shows the (texted) question.
    "Fantasy 4X Setup.app",
    "display dialog",
    "--question",
    "\"/Applications\"",
];

#[test]
fn the_client_holds_no_message_in_its_code() -> TestResult {
    let mut files = Vec::new();
    sources(&root().join("crates/f4x-client/src"), &mut files)?;
    let mut found = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .ok_or("a source file has no name")?
            .to_string_lossy()
            .into_owned();
        if EXEMPT_FILES.contains(&name.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(&file)?;
        for (n, line) in src.lines().enumerate() {
            // The tests of a file come last; their literals are fixtures.
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            let trimmed = line.trim_start();
            let skip = ["//", "use ", "mod ", "#["]
                .iter()
                .any(|p| trimmed.starts_with(p))
                || ["assert", "expect(", "println", "panic", "unreachable"]
                    .iter()
                    .any(|p| line.contains(p));
            if skip
                || EXEMPT_LINES.iter().any(|e| line.contains(e))
                || EXEMPT_FRAGMENTS.iter().any(|e| line.contains(e))
            {
                continue;
            }
            for lit in line.split('"').skip(1).step_by(2) {
                if looks_like_text(lit) {
                    found.push(format!("{name}:{}: {}", n + 1, trimmed));
                    break;
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "text in the client's code; move it to crates/f4x-lang/content/text/en/ (docs/text.md):\n{}",
        found.join("\n")
    );
    Ok(())
}

/// A word of three letters, and not an id, a path or a picture file's name.
fn looks_like_text(lit: &str) -> bool {
    const ART: &[&str] = &[
        "cityscap", "backgrnd", "resource", "reload", "buildscr", "units", "items", "itemisc",
        "hire", "wizards", "newgame", "figures", "spells", "special",
    ];
    // The names inside braces are not words of the message.
    let mut outside = String::new();
    let mut depth = 0;
    for c in lit.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            c if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    let has_word = outside
        .as_bytes()
        .windows(3)
        .any(|w| w.iter().all(u8::is_ascii_alphabetic));
    let id = lit
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.:/-".contains(c));
    has_word && !id && !ART.contains(&lit)
}

/// The keys of `name/word.ron` (`Names::word`), and the part before the dot of each.
fn words() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let path = root().join("crates/f4x-lang/content/text/en/name/word.ron");
    let rows: std::collections::BTreeMap<String, ron::Value> =
        ron::from_str(&std::fs::read_to_string(path)?)?;
    Ok(rows.into_keys().collect())
}

#[test]
fn every_word_the_code_names_is_written_and_every_word_is_used() -> TestResult {
    let written = words()?;
    let groups: Vec<String> = written
        .iter()
        .filter_map(|k| k.split_once('.').map(|(g, _)| g.to_string()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let named = named_in_code(&groups)?;
    let missing: Vec<_> = named.difference(&written).collect();
    assert!(
        missing.is_empty(),
        "words the code names but word.ron lacks: {missing:?}"
    );
    let unused: Vec<_> = written.difference(&named).collect();
    assert!(
        unused.is_empty(),
        "words in word.ron that no code names: {unused:?}"
    );
    Ok(())
}

/// The `Names` accessors that give a plain string although the name has a term (`unit_term`, ...):
/// a message that takes the string cannot ask for a case, an article or a number.
const FLAT_NAMES: &[&str] = &[
    "unit",
    "ability",
    "building",
    "realm",
    "retort",
    "spell",
    "item",
    "terrain",
    "feature",
    "race",
    "race_adjective",
    "race_plural",
    "item_name",
];

/// The code before its tests, with the comments dropped.
fn code_of(src: &str) -> String {
    let body = src.split("\n#[cfg(test)]").next().unwrap_or(src);
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Does `expr` call one of the flat accessors, as in `self.names.spell(..)`?
fn calls_flat_name(expr: &str) -> bool {
    FLAT_NAMES
        .iter()
        .any(|n| expr.contains(&format!("names.{n}(")) || expr.contains(&format!("names\n{n}(")))
        || expr.split("names").skip(1).any(|rest| {
            let rest = rest.trim_start();
            FLAT_NAMES.iter().any(|n| {
                rest.strip_prefix('.')
                    .is_some_and(|r| r.trim_start().starts_with(&format!("{n}(")))
            })
        })
}

/// A name of the game (a unit, a spell, an item, ...) goes into a message as a term
/// (`Args::term`, `Names::spell_term`), never as the string of `Names::spell`: German needs the
/// gender, the cases and the number of the name, and a string has lost them. A variable or a closure
/// that holds the flat string and then feeds `Args::text` is caught as well.
#[test]
fn a_name_goes_into_a_message_as_a_term() -> TestResult {
    let mut files = Vec::new();
    sources(&root().join("crates/f4x-client/src"), &mut files)?;
    let mut found = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .ok_or("a source file has no name")?
            .to_string_lossy()
            .into_owned();
        let code = code_of(&std::fs::read_to_string(&file)?);
        // `.text("key", <value>)`: the value as far as the closing parenthesis of the call.
        let mut values: Vec<String> = Vec::new();
        for part in code.split(".text(\"").skip(1) {
            let Some((_, rest)) = part.split_once("\",") else {
                continue;
            };
            let mut depth = 1;
            let mut end = rest.len();
            for (i, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            values.push(rest[..end].to_string());
        }
        // Variables and closures bound to a flat name: `let spell = ...names.spell(..)...;`.
        let mut bound: Vec<String> = Vec::new();
        for stmt in code.split("let ").skip(1) {
            let Some((head, tail)) = stmt.split_once('=') else {
                continue;
            };
            let var = head.trim().trim_start_matches("mut ").trim();
            let value = tail.split(';').next().unwrap_or("");
            // A struct literal (`SquareText { .. }`) holds the names, it is not one.
            let literal = value
                .trim_start()
                .chars()
                .next()
                .is_some_and(char::is_uppercase);
            if !literal
                && !var.is_empty()
                && var.chars().all(|c| c.is_alphanumeric() || c == '_')
                && calls_flat_name(value)
            {
                bound.push(var.to_string());
            }
        }
        for value in &values {
            let v = value.trim().trim_start_matches('&');
            let ident: String = v
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let held = !ident.is_empty() && bound.contains(&ident);
            if calls_flat_name(value) || held {
                found.push(format!("{name}: .text(.., {})", value.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a name of the game passed as text; pass its term (`Args::term`, `Names::*_term`, docs/text.md):\n{}",
        found.join("\n")
    );
    Ok(())
}
