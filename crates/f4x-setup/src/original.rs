//! Finding the original game's folder and checking that it is the right one.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The `.lbx` files of the original (names only), one per line: what a complete copy has.
const EXPECTED: &str = include_str!("../original_files.txt");

/// The magic number at bytes 2 and 3 of an `.lbx` file.
const LBX_MAGIC: u16 = 0xFEAD;

/// The file the folder is recognised by.
const SIGNATURE: &str = "FONTS.LBX";

/// What a folder holds, compared with a complete copy.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Check {
    /// Files of a complete copy that the folder lacks.
    pub missing: Vec<String>,
    /// Files that are there but are not `.lbx` archives (cut short or another file).
    pub damaged: Vec<String>,
}

impl Check {
    /// Nothing is missing and nothing is damaged.
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.damaged.is_empty()
    }

    /// What is wrong, in plain words (empty when the copy is complete).
    pub fn explain(&self) -> String {
        let list = |names: &[String]| {
            const SHOWN: usize = 8;
            let mut text = names
                .iter()
                .take(SHOWN)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if names.len() > SHOWN {
                text.push_str(&format!(" and {} more", names.len() - SHOWN));
            }
            text
        };
        let mut lines = Vec::new();
        if !self.missing.is_empty() {
            lines.push(format!(
                "{} of the game's files are missing from this folder: {}.",
                self.missing.len(),
                list(&self.missing)
            ));
        }
        if !self.damaged.is_empty() {
            lines.push(format!(
                "{} files are not what they should be (cut short, or another kind of file): {}.",
                self.damaged.len(),
                list(&self.damaged)
            ));
        }
        lines.join("\n")
    }
}

fn expected() -> impl Iterator<Item = &'static str> {
    EXPECTED.lines().map(str::trim).filter(|l| !l.is_empty())
}

/// The entries of a folder as `(upper-case name, path)`, so names match whatever their case.
fn entries(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let read = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    Ok(read
        .flatten()
        .filter_map(|e| Some((e.file_name().to_str()?.to_uppercase(), e.path())))
        .collect())
}

/// Find the folder with the original's files: `dir` itself, or the first folder directly inside it
/// that has them (the Steam download has the files one level down, in
/// `Master of Magic Official Release`).
pub fn find(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder.", dir.display()));
    }
    let has_signature = |d: &Path| entries(d).is_ok_and(|e| e.iter().any(|(n, _)| n == SIGNATURE));
    if has_signature(dir) {
        return Ok(dir.to_path_buf());
    }
    let mut inside: Vec<PathBuf> = entries(dir)?
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| p.is_dir() && has_signature(p))
        .collect();
    inside.sort();
    match inside.len() {
        0 => Err(format!(
            "this does not look like the original Master of Magic: {} has no {SIGNATURE}, \
             and neither has a folder inside it. Choose the folder that holds the game's .LBX files.",
            dir.display()
        )),
        1 => Ok(inside.remove(0)),
        _ => Err(format!(
            "{} holds the game's files more than once: {}. Choose the one you want.",
            dir.display(),
            inside
                .iter()
                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn has_lbx_header(path: &Path) -> bool {
    let mut head = [0u8; 4];
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    file.read_exact(&mut head).is_ok() && u16::from_le_bytes([head[2], head[3]]) == LBX_MAGIC
}

/// Compare the folder with a complete copy.
pub fn check(dir: &Path) -> Result<Check, String> {
    let present = entries(dir)?;
    let mut result = Check::default();
    for name in expected() {
        match present.iter().find(|(n, _)| n == name) {
            None => result.missing.push(name.to_string()),
            Some((_, path)) if !has_lbx_header(path) => result.damaged.push(name.to_string()),
            Some(_) => {}
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("f4x_setup_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A file with the `.lbx` header and nothing else.
    fn lbx(dir: &Path, name: &str) {
        fs::write(dir.join(name), [0, 0, 0xAD, 0xFE, 0, 0, 0, 0]).unwrap();
    }

    fn all_but(dir: &Path, skip: &[&str]) {
        for name in expected().filter(|n| !skip.contains(n)) {
            lbx(dir, name);
        }
    }

    #[test]
    fn the_expected_list_names_the_signature_and_has_no_duplicates() {
        let names: Vec<_> = expected().collect();
        assert!(names.contains(&SIGNATURE));
        assert!(names.len() > 90, "{}", names.len());
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    #[test]
    fn a_complete_copy_passes_whatever_the_case_of_the_names() {
        let dir = temp("complete");
        for name in expected() {
            lbx(&dir, &name.to_lowercase());
        }
        let result = check(&dir).unwrap();
        assert!(result.is_complete(), "{}", result.explain());
        assert_eq!(result.explain(), "");
    }

    #[test]
    fn missing_and_damaged_files_are_named() {
        let dir = temp("partial");
        all_but(&dir, &["BOOK.LBX", "COMBAT.LBX"]);
        fs::write(dir.join("ARMYLIST.LBX"), b"not an archive").unwrap();
        let result = check(&dir).unwrap();
        assert_eq!(result.missing, ["BOOK.LBX", "COMBAT.LBX"]);
        assert_eq!(result.damaged, ["ARMYLIST.LBX"]);
        let text = result.explain();
        assert!(text.contains("2 of the game's files are missing"), "{text}");
        assert!(text.contains("BOOK.LBX, COMBAT.LBX"), "{text}");
        assert!(text.contains("ARMYLIST.LBX"), "{text}");
    }

    #[test]
    fn a_long_list_is_cut_with_a_count() {
        let result = Check {
            missing: (0..12).map(|i| format!("F{i}.LBX")).collect(),
            damaged: Vec::new(),
        };
        assert!(
            result.explain().contains("and 4 more"),
            "{}",
            result.explain()
        );
    }

    #[test]
    fn the_folder_is_found_one_level_down() {
        let outer = temp("outer");
        let inner = outer.join("Master of Magic Official Release");
        fs::create_dir_all(&inner).unwrap();
        lbx(&inner, "fonts.lbx");
        assert_eq!(find(&outer).unwrap(), inner);
        assert_eq!(find(&inner).unwrap(), inner);
    }

    #[test]
    fn two_folders_with_the_game_are_not_guessed() {
        let outer = temp("twice");
        for name in ["Official Release", "Community Patch"] {
            let inner = outer.join(name);
            fs::create_dir_all(&inner).unwrap();
            lbx(&inner, "FONTS.LBX");
        }
        let text = find(&outer).unwrap_err();
        assert!(text.contains("more than once"), "{text}");
        assert!(text.contains("Community Patch, Official Release"), "{text}");
    }

    #[test]
    fn a_wrong_folder_and_a_missing_one_get_a_sentence() {
        let empty = temp("wrong");
        let text = find(&empty).unwrap_err();
        assert!(text.contains("does not look like the original"), "{text}");
        assert!(text.contains("FONTS.LBX"), "{text}");
        let text = find(&empty.join("nowhere")).unwrap_err();
        assert!(text.contains("is not a folder"), "{text}");
    }
}
