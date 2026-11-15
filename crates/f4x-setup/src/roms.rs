//! The Roland MT-32 ROM files the music is rendered with (`docs/installer-design.md` 3.2).
//!
//! The project ships no ROM. The player has them, or the setup downloads them from the Internet
//! Archive after the player agreed. Either way the two files are checked against the SHA-256 values
//! pinned here, so a wrong revision or a damaged file is found before any song is rendered.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use f4x_lang::text::Args;
use sha2::{Digest, Sha256};

use crate::say::Say;

/// The Internet Archive item the ROMs come from (the README names the same one).
pub const SOURCE_PAGE: &str =
    "https://archive.org/details/mame-versioned-roland-mt-32-and-cm-32l-rom-files";
/// The zip of the item that holds the two files (and the other ROM sets, which are not used).
pub const ZIP_URL: &str = "https://archive.org/download/mame-versioned-roland-mt-32-and-cm-32l-rom-files/MT-32_and_CM-32L_MAME-Versioned_ROM_files..zip";
/// The size of that zip, for the dialog and as a limit.
pub const ZIP_BYTES: u64 = 2_900_164;

/// The two files of the new revision and their SHA-256.
pub const FILES: [(&str, &str); 2] = [
    (
        "mt32_ctrl_2_07.rom",
        "a1ee6ac3190bb94daeae1785b1c4b0f91dc1648880ecef0772df794f20673dfb",
    ),
    (
        "mt32_pcm.rom",
        "d9164063f293410cf33f2f64cdcea6893b44723fe41938e07ec9aef58b406238",
    ),
];

/// Where the setup keeps the ROMs, inside the data folder.
pub fn folder(data: &Path) -> PathBuf {
    data.join("roms")
}

/// Why the ROMs are not usable.
#[derive(Debug, PartialEq, Eq)]
pub enum RomError {
    /// Files that are not in the folder or the zip.
    Missing(Vec<String>),
    /// Files that are there but are not the expected revision or are damaged.
    WrongVersion(Vec<String>),
    /// The download failed, or the disk did.
    Failed(String),
}

impl RomError {
    /// The problem in plain words.
    pub fn explain(&self, say: &Say) -> String {
        match self {
            RomError::Missing(names) => say.get(
                "setup.err_roms_missing",
                &Args::new().list("names", names.clone()),
            ),
            RomError::WrongVersion(names) => say.get(
                "setup.err_roms_wrong",
                &Args::new().list("names", names.clone()),
            ),
            RomError::Failed(why) => say.get(
                "setup.err_roms_failed",
                &Args::new().text("why", why.clone()),
            ),
        }
    }
}

/// The SHA-256 of `bytes` as lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Check one set of file contents against the pinned values.
fn check_contents(
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<(String, Vec<u8>)>, RomError> {
    let (mut missing, mut wrong, mut good) = (Vec::new(), Vec::new(), Vec::new());
    for (name, hash) in FILES {
        match read(name) {
            None => missing.push(name.to_string()),
            Some(bytes) if sha256_hex(&bytes) != hash => wrong.push(name.to_string()),
            Some(bytes) => good.push((name.to_string(), bytes)),
        }
    }
    if !missing.is_empty() {
        Err(RomError::Missing(missing))
    } else if !wrong.is_empty() {
        Err(RomError::WrongVersion(wrong))
    } else {
        Ok(good)
    }
}

/// Check that `dir` holds the two files, unchanged.
pub fn check_folder(dir: &Path) -> Result<(), RomError> {
    check_contents(|name| fs::read(dir.join(name)).ok()).map(|_| ())
}

/// Copy the two files from `from` into `to` (after checking them).
pub fn copy_into(from: &Path, to: &Path) -> Result<(), RomError> {
    let files = check_contents(|name| fs::read(from.join(name)).ok())?;
    write_files(&files, to)
}

fn write_files(files: &[(String, Vec<u8>)], to: &Path) -> Result<(), RomError> {
    fs::create_dir_all(to).map_err(|e| RomError::Failed(format!("{}: {e}", to.display())))?;
    for (name, bytes) in files {
        fs::write(to.join(name), bytes)
            .map_err(|e| RomError::Failed(format!("{}: {e}", to.join(name).display())))?;
    }
    Ok(())
}

/// Take the two files out of the downloaded zip, check them and write them into `to`. The other
/// ROM sets in the zip are ignored.
pub fn from_zip(zip_bytes: &[u8], to: &Path) -> Result<(), RomError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| RomError::Failed(e.to_string()))?;
    let mut read = |name: &str| {
        let mut entry = archive.by_name(name).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    };
    // `by_name` needs the archive mutably, so read both before checking.
    let mut found: Vec<(String, Option<Vec<u8>>)> = Vec::new();
    for (name, _) in FILES {
        found.push((name.to_string(), read(name)));
    }
    let files = check_contents(|name| {
        found
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, bytes)| bytes.clone())
    })?;
    write_files(&files, to)
}

/// Gets a file from the Internet; the real one is in the binary, the tests give a fake.
pub trait Fetch {
    /// The bytes of `url`, at most `limit` of them; `progress` gets the bytes so far.
    fn get(&self, url: &str, limit: u64, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String>;
}

/// Download the zip, take the two files out and write them into `to`.
pub fn download(
    fetch: &dyn Fetch,
    to: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<(), RomError> {
    // A little more than the size on the archive's page, so a changed file is noticed by its hash.
    let bytes = fetch
        .get(ZIP_URL, ZIP_BYTES * 2, progress)
        .map_err(RomError::Failed)?;
    from_zip(&bytes, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::say::test_say;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("f4x_roms_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The real files from the author's folder, when it is there (it is not in CI).
    fn real_roms() -> Option<Vec<(String, Vec<u8>)>> {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/Roland MT-32/rom-new");
        FILES
            .iter()
            .map(|(name, _)| Some((name.to_string(), fs::read(dir.join(name)).ok()?)))
            .collect()
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
            let options = zip::write::SimpleFileOptions::default();
            for (name, bytes) in files {
                writer.start_file(*name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        out
    }

    #[test]
    fn the_pinned_hashes_are_sha256_of_the_right_length() {
        for (name, hash) in FILES {
            assert_eq!(hash.len(), 64, "{name}");
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        }
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_missing_and_a_changed_file_are_told_apart() {
        let dir = temp("check");
        assert_eq!(
            check_folder(&dir),
            Err(RomError::Missing(vec![
                FILES[0].0.into(),
                FILES[1].0.into()
            ]))
        );
        fs::write(dir.join(FILES[0].0), b"not a rom").unwrap();
        fs::write(dir.join(FILES[1].0), b"not a rom").unwrap();
        let error = check_folder(&dir).unwrap_err();
        assert_eq!(
            error,
            RomError::WrongVersion(vec![FILES[0].0.into(), FILES[1].0.into()])
        );
        let text = error.explain(&test_say());
        assert!(
            text.contains("mt32_ctrl_2_07.rom and mt32_pcm.rom"),
            "{text}"
        );
    }

    #[test]
    fn the_real_files_pass_and_are_copied_when_present() {
        let Some(files) = real_roms() else {
            eprintln!("no ROM files at reference/Roland MT-32/rom-new: skipped");
            return;
        };
        let from = temp("real_from");
        write_files(&files, &from).unwrap();
        check_folder(&from).unwrap();
        let to = temp("real_to");
        copy_into(&from, &to).unwrap();
        check_folder(&to).unwrap();
    }

    #[test]
    fn the_two_files_are_taken_from_a_zip_with_other_roms_and_checked() {
        let Some(files) = real_roms() else {
            eprintln!("no ROM files at reference/Roland MT-32/rom-new: skipped");
            return;
        };
        let zip = zip_of(&[
            ("cm32l_pcm.rom", b"another set"),
            (FILES[0].0, &files[0].1),
            (FILES[1].0, &files[1].1),
        ]);
        let to = temp("zip_ok");
        from_zip(&zip, &to).unwrap();
        check_folder(&to).unwrap();
        assert!(!to.join("cm32l_pcm.rom").exists());
        // A zip with a changed file is refused and writes nothing.
        let bad = zip_of(&[(FILES[0].0, b"changed"), (FILES[1].0, &files[1].1)]);
        let to = temp("zip_bad");
        assert!(matches!(
            from_zip(&bad, &to),
            Err(RomError::WrongVersion(_))
        ));
        assert!(!to.join(FILES[1].0).exists());
        // A zip without the files, and something that is no zip.
        let none = zip_of(&[("readme.txt", b"hi")]);
        assert!(matches!(from_zip(&none, &to), Err(RomError::Missing(_))));
        assert!(matches!(
            from_zip(b"not a zip", &to),
            Err(RomError::Failed(_))
        ));
    }

    struct Offline;
    impl Fetch for Offline {
        fn get(&self, url: &str, _: u64, _: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
            Err(format!("no network for {url}"))
        }
    }

    #[test]
    fn a_failed_download_says_why_and_the_url_is_the_pinned_one() {
        let error = download(&Offline, &temp("offline"), &mut |_| {}).unwrap_err();
        let text = error.explain(&test_say());
        assert!(text.contains("no network for"), "{text}");
        assert!(text.contains(ZIP_URL), "{text}");
        assert!(ZIP_URL.starts_with(&SOURCE_PAGE.replace("/details/", "/download/")));
    }
}
