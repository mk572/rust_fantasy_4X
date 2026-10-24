//! The messages in `content/text/` and the code agree: every key the code names exists, every key
//! is used, and a file holds only the keys of its area (`battle.ron` holds `battle.*`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A literal like "hall.txt" is a file name, not a key.
const FILE_EXTENSIONS: [&str; 4] = ["ron", "txt", "png", "json"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The area of each file of the English messages, and its keys.
fn written() -> Vec<(String, Vec<String>)> {
    let folder = root().join("content/text/en");
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .expect("content/text/en exists")
        .map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| {
            let entries: std::collections::BTreeMap<String, f4x_core::text::Entry> =
                ron::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let area = path.file_stem().unwrap().to_string_lossy().into_owned();
            (area, entries.into_keys().collect())
        })
        .collect()
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The string literals of the code that look like a key of one of the areas.
fn named_in_code(areas: &[String]) -> BTreeSet<String> {
    let mut files = Vec::new();
    for krate in std::fs::read_dir(root().join("crates")).unwrap() {
        sources(&krate.unwrap().path().join("src"), &mut files);
    }
    let mut found = BTreeSet::new();
    for file in files {
        // The tests of the text module itself use made-up keys.
        if file.ends_with("text/tests.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&file).unwrap();
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
    found
}

#[test]
fn a_file_holds_only_the_keys_of_its_area() {
    for (area, keys) in written() {
        for key in keys {
            assert!(
                key.starts_with(&format!("{area}.")),
                "{key} is in {area}.ron but is not an {area} key"
            );
        }
    }
}

#[test]
fn every_key_the_code_names_is_written() {
    let written = written();
    let areas: Vec<String> = written.iter().map(|(a, _)| a.clone()).collect();
    let keys: BTreeSet<String> = written.into_iter().flat_map(|(_, k)| k).collect();
    let missing: Vec<_> = named_in_code(&areas).difference(&keys).cloned().collect();
    assert!(
        missing.is_empty(),
        "keys the code names but no file has: {missing:?}"
    );
}

#[test]
fn every_key_that_is_written_is_used() {
    let written = written();
    let areas: Vec<String> = written.iter().map(|(a, _)| a.clone()).collect();
    let used = named_in_code(&areas);
    let unused: Vec<_> = written
        .into_iter()
        .flat_map(|(_, k)| k)
        .filter(|k| !used.contains(k))
        .collect();
    assert!(unused.is_empty(), "keys no code uses: {unused:?}");
}

/// Files of the client whose literals are not messages: the command line, the picture files, the
/// developer console, the key-name table.
const EXEMPT_FILES: [&str; 4] = ["main.rs", "art.rs", "console_screen.rs", "hotkeys.rs"];

/// Lines that look like text but are not: number formats, the space between wrapped words, a
/// screenshot fixture, the debug form of an error, the help parser, an error for the developer.
const EXEMPT_LINES: [&str; 22] = [
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
const EXEMPT_FRAGMENTS: [&str; 6] = [
    "[{name}]",
    "+{prod}%",
    "+{gold}%",
    "{first}: {r}",
    "saves/slot",
    "{year:04}",
];

#[test]
fn the_client_holds_no_message_in_its_code() {
    let mut files = Vec::new();
    sources(&root().join("crates/f4x-client/src"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if EXEMPT_FILES.contains(&name.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(&file).unwrap();
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
        "text in the client's code; move it to content/text/en/ (docs/text.md):\n{}",
        found.join("\n")
    );
}

/// A word of three letters, and not an id, a path or a picture file's name.
fn looks_like_text(lit: &str) -> bool {
    const ART: [&str; 14] = [
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
