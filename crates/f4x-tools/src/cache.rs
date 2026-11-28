//! The reference-only game of `soak-versus` does not change while only the live AI is edited, so
//! its result is kept in `<out>/cache/` and reused. The key is a fingerprint of everything that
//! decides that game: the rules, the content, the reference AI, the soak's own code and the
//! options.
//!
//! Tests: the fingerprint of a folder (readable, missing); the kept games are checked by `f4x-tools soak-versus`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::soak::{GameResult, Options};

/// The folders and files whose contents decide the reference-only game.
const SOURCES: &[&str] = &[
    "crates/f4x-core/src",
    "crates/f4x-def/src",
    "crates/f4x-battle/src",
    "crates/f4x-master/src",
    "crates/f4x-newgame/src",
    "crates/f4x-ai-ref/src",
    "crates/f4x-tools/src",
    "Cargo.lock",
    "crates/f4x-def/content",
];

fn hash_path(path: &Path, hasher: &mut DefaultHasher) -> Result<(), String> {
    let said = |e: std::io::Error| format!("{}: {e}", path.display());
    if path.is_dir() {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path).map_err(said)? {
            entries.push(entry.map_err(said)?.path());
        }
        entries.sort();
        for entry in entries {
            hash_path(&entry, hasher)?;
        }
    } else {
        let bytes = std::fs::read(path).map_err(said)?;
        path.hash(hasher);
        bytes.hash(hasher);
    }
    Ok(())
}

/// The fingerprint of the reference-only game of `seed` under these options. A source that
/// cannot be read (missing, no permission, run from the wrong folder) makes the fingerprint
/// worthless: a changed file would go unseen and an old game be reused as the new one. No key
/// then means no cache, and the reason is said once.
fn key(opts: &Options, seed: u64) -> Option<String> {
    let mut hasher = DefaultHasher::new();
    let mut hashed = SOURCES.iter().copied().map(Path::new).collect::<Vec<_>>();
    hashed.push(&opts.dir);
    if let Some(from) = &opts.from {
        hashed.push(from);
    }
    for path in hashed {
        if let Err(why) = hash_path(path, &mut hasher) {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| eprintln!("no cache for the reference games: {why}"));
            return None;
        }
    }
    hash_options(opts, seed, &mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Every option that changes the kept game or its kept report. All fields are named, so that a
/// new option does not compile until it is sorted into one of the two kinds.
fn hash_options(opts: &Options, seed: u64, hasher: &mut DefaultHasher) {
    let Options {
        turns,
        players,
        events,
        grant_mastery,
        snapshots_every,
        map,
        land,
        magic,
        difficulty,
        mastery,
        spell_return,
        rules,
        starting_units,
        // The seed of this game comes in by itself.
        first: _,
        last: _,
        // What is printed or written while a game is played.
        every: _,
        trace: _,
        detail: _,
        refused_log: _,
        no_refused_log: _,
        out: _,
        jobs: _,
        // With no cache asked for, no game is kept.
        no_cache: _,
        // Their contents are hashed with the sources.
        dir: _,
        from: _,
    } = opts;
    seed.hash(hasher);
    (turns, players, events, grant_mastery, snapshots_every).hash(hasher);
    (map, land, magic, difficulty).hash(hasher);
    (mastery, spell_return, rules, starting_units).hash(hasher);
}

fn file(out: &Path, opts: &Options, seed: u64) -> Option<PathBuf> {
    let key = key(opts, seed)?;
    Some(
        out.join("cache")
            .join(format!("ref-all-seed{seed}-{key}.ron")),
    )
}

/// The kept result of the reference-only game, if there is one for this build.
pub fn load(out: &Path, opts: &Options, seed: u64) -> Option<GameResult> {
    let text = std::fs::read_to_string(file(out, opts, seed)?).ok()?;
    ron::from_str(&text).ok()
}

pub fn store(out: &Path, opts: &Options, seed: u64, result: &GameResult) -> Result<(), String> {
    let Some(path) = file(out, opts, seed) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = ron::to_string(result).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::{TestDir, TestResult};

    fn fingerprint(path: &Path) -> Result<u64, String> {
        let mut hasher = DefaultHasher::new();
        hash_path(path, &mut hasher)?;
        Ok(hasher.finish())
    }

    #[test]
    fn the_fingerprint_follows_the_contents_of_a_folder() -> TestResult {
        let dir = TestDir::new("cache-fingerprint");
        std::fs::write(dir.join("a.txt"), "one")?;
        let before = fingerprint(&dir)?;
        assert_eq!(before, fingerprint(&dir)?);
        std::fs::write(dir.join("a.txt"), "two")?;
        assert_ne!(before, fingerprint(&dir)?);
        Ok(())
    }

    #[test]
    fn the_key_follows_every_option_that_changes_the_game() -> TestResult {
        let of = |args: &[&str]| -> Result<u64, String> {
            let args: Vec<String> = args.iter().map(|&a| a.to_string()).collect();
            let opts = Options::parse(&args, "")?;
            let mut hasher = DefaultHasher::new();
            hash_options(&opts, 1, &mut hasher);
            Ok(hasher.finish())
        };
        let plain = of(&[])?;
        assert_eq!(plain, of(&["--jobs", "2", "--every", "10", "--detail"])?);
        for args in [
            &["--turns", "7"][..],
            &["--players", "3"],
            &["--events"],
            &["--grant-mastery", "5"],
            &["--snapshots-every", "9"],
            &["--map", "40x30"],
            &["--land", "40"],
            &["--magic", "2"],
            &["--difficulty", "3"],
            &["--rules", "mk"],
            &["--mastery", "off"],
            &["--return", "off"],
            &["--start-units", "trireme"],
        ] {
            assert_ne!(plain, of(args)?, "{args:?}");
        }
        Ok(())
    }

    #[test]
    fn a_source_that_is_not_there_is_an_error_not_a_smaller_fingerprint() -> TestResult {
        let dir = TestDir::new("cache-missing");
        let why = fingerprint(&dir.join("nothing")).err().ok_or("no error")?;
        assert!(why.contains("nothing"), "{why}");
        Ok(())
    }
}
