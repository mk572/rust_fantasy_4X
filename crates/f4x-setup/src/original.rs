//! Finding the original game's folder and checking that it is the right one.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use f4x_lang::text::Args;

use crate::say::Say;

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
    pub fn explain(&self, say: &Say) -> String {
        let mut lines = Vec::new();
        if !self.missing.is_empty() {
            let args = Args::new()
                .count("n", self.missing.len())
                .text("names", say.names(&self.missing));
            lines.push(say.get("setup.check_missing", &args));
        }
        if !self.damaged.is_empty() {
            let args = Args::new()
                .count("n", self.damaged.len())
                .text("names", say.names(&self.damaged));
            lines.push(say.get("setup.check_damaged", &args));
        }
        lines.join("\n")
    }
}

/// Why no folder of the original was found.
#[derive(Debug, PartialEq, Eq)]
pub enum FindError {
    NotAFolder(PathBuf),
    Unreadable(PathBuf, String),
    /// Neither the folder nor one inside it has the signature file.
    NoGame(PathBuf),
    /// More than one folder inside has it: the names of the folders.
    Several(PathBuf, Vec<String>),
}

impl FindError {
    /// The problem in plain words.
    pub fn explain(&self, say: &Say) -> String {
        match self {
            FindError::NotAFolder(path) => say.get(
                "setup.err_not_folder",
                &Args::new().text("path", path.display().to_string()),
            ),
            FindError::Unreadable(path, why) => say.get(
                "setup.err_unreadable",
                &Args::new()
                    .text("path", path.display().to_string())
                    .text("why", why.clone()),
            ),
            FindError::NoGame(path) => say.get(
                "setup.err_no_game",
                &Args::new()
                    .text("path", path.display().to_string())
                    .text("file", SIGNATURE),
            ),
            FindError::Several(path, names) => say.get(
                "setup.err_several",
                &Args::new()
                    .text("path", path.display().to_string())
                    .list("names", names.clone()),
            ),
        }
    }
}

fn expected() -> impl Iterator<Item = &'static str> {
    EXPECTED.lines().map(str::trim).filter(|l| !l.is_empty())
}

/// The entries of a folder as `(upper-case name, path)`, so names match whatever their case.
fn entries(dir: &Path) -> Result<Vec<(String, PathBuf)>, FindError> {
    let read =
        fs::read_dir(dir).map_err(|e| FindError::Unreadable(dir.to_path_buf(), e.to_string()))?;
    Ok(read
        .flatten()
        .filter_map(|e| Some((e.file_name().to_str()?.to_uppercase(), e.path())))
        .collect())
}

/// Find the folder with the original's files: `dir` itself, or the first folder directly inside it
/// that has them (the Steam download has the files one level down, in
/// `Master of Magic Official Release`).
pub fn find(dir: &Path) -> Result<PathBuf, FindError> {
    if !dir.is_dir() {
        return Err(FindError::NotAFolder(dir.to_path_buf()));
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
        0 => Err(FindError::NoGame(dir.to_path_buf())),
        1 => Ok(inside.remove(0)),
        _ => Err(FindError::Several(
            dir.to_path_buf(),
            inside
                .iter()
                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                .collect(),
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
pub fn check(dir: &Path) -> Result<Check, FindError> {
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

    use crate::say::test_say;

    #[test]
    fn a_complete_copy_passes_whatever_the_case_of_the_names() {
        let dir = temp("complete");
        for name in expected() {
            lbx(&dir, &name.to_lowercase());
        }
        let result = check(&dir).unwrap();
        assert!(result.is_complete(), "{}", result.explain(&test_say()));
        assert_eq!(result.explain(&test_say()), "");
    }

    #[test]
    fn missing_and_damaged_files_are_named() {
        let dir = temp("partial");
        all_but(&dir, &["BOOK.LBX", "COMBAT.LBX"]);
        fs::write(dir.join("ARMYLIST.LBX"), b"not an archive").unwrap();
        let result = check(&dir).unwrap();
        assert_eq!(result.missing, ["BOOK.LBX", "COMBAT.LBX"]);
        assert_eq!(result.damaged, ["ARMYLIST.LBX"]);
        let text = result.explain(&test_say());
        assert!(text.contains("2 of the game's files are missing"), "{text}");
        assert!(text.contains("BOOK.LBX and COMBAT.LBX"), "{text}");
        assert!(text.contains("One file is not what it should be"), "{text}");
        assert!(text.contains("ARMYLIST.LBX"), "{text}");
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
        let error = find(&outer).unwrap_err();
        assert!(matches!(error, FindError::Several(..)), "{error:?}");
        let text = error.explain(&test_say());
        assert!(text.contains("more than once"), "{text}");
        assert!(
            text.contains("Community Patch and Official Release"),
            "{text}"
        );
    }

    #[test]
    fn a_wrong_folder_and_a_missing_one_get_a_sentence() {
        let empty = temp("wrong");
        let error = find(&empty).unwrap_err();
        assert_eq!(error, FindError::NoGame(empty.clone()));
        let text = error.explain(&test_say());
        assert!(text.contains("does not look like the original"), "{text}");
        assert!(text.contains("FONTS.LBX"), "{text}");
        let text = find(&empty.join("nowhere"))
            .unwrap_err()
            .explain(&test_say());
        assert!(text.contains("is not a folder"), "{text}");
    }
}
