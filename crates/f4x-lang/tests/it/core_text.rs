//! The refusals of `f4x-core` name their text by a key (`Rejection::key`); core cannot read the language
//! files (this crate depends on it), so the check is here: every key core names exists in English and in
//! German, so that no refusal reaches the player as a bare key.

use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a source folder") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every string of the form `"refuse_x.name"` in the source of core.
fn refusal_keys() -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-core/src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    let mut keys = std::collections::BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a source file");
        for piece in text.split('"').skip(1).step_by(2) {
            let valid = piece.starts_with("refuse_")
                && piece.split_once('.').is_some_and(|(a, b)| {
                    !b.is_empty()
                        && a.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                        && b.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                });
            if valid {
                keys.insert(piece.to_string());
            }
        }
    }
    keys.into_iter().collect()
}

#[test]
fn every_refusal_of_core_has_a_text_in_each_language() {
    let keys = refusal_keys();
    assert!(keys.len() > 50, "the keys were not found: {keys:?}");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
    for lang in ["en", "de"] {
        let texts = f4x_lang::text::load_texts(&[dir.as_path()], lang).expect("the texts load");
        let missing: Vec<_> = keys.iter().filter(|k| !texts.has_own(k)).collect();
        assert!(missing.is_empty(), "{lang}: no text for {missing:#?}");
    }
}
