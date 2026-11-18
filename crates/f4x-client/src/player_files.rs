//! The player's own files. The game saves are in `saves/`; what the player chose (the preferences,
//! the default game settings, the keys, the language) is in `settings/`. Files an earlier version
//! kept in `saves/` are moved over once.

use std::path::Path;

/// The folder of the player's settings, under the data folder.
pub(crate) const FOLDER: &str = "settings";

/// The files that used to be in `saves/`.
const MOVED: &[&str] = &["preferences.txt", "default_game.ron", "hotkeys.ron"];

/// Move each of the old files from `saves/` of `base` to `settings/`, unless the new place has the
/// file already (then it wins and the old one stays where it is). Returns the names it moved.
pub(crate) fn migrate(base: &Path) -> std::io::Result<Vec<&'static str>> {
    let mut moved = Vec::new();
    for &name in MOVED {
        let old = base.join("saves").join(name);
        let new = base.join(FOLDER).join(name);
        if !old.is_file() || new.exists() {
            continue;
        }
        std::fs::create_dir_all(base.join(FOLDER))?;
        std::fs::rename(&old, &new)?;
        moved.push(name);
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_def::testdir::{TestDir, TestResult};

    #[test]
    fn the_old_files_move_from_the_saves_to_the_settings_once() -> TestResult {
        let dir = TestDir::new("player_files");
        std::fs::create_dir_all(dir.join("saves"))?;
        std::fs::write(dir.join("saves/preferences.txt"), "sound_effects=0\n")?;
        std::fs::write(dir.join("saves/slot1.ron"), "a game")?;
        let moved = migrate(&dir)?;
        assert_eq!(moved, vec!["preferences.txt"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("settings/preferences.txt"))?,
            "sound_effects=0\n"
        );
        assert!(!dir.join("saves/preferences.txt").exists());
        // A game save stays where it is.
        assert!(dir.join("saves/slot1.ron").exists());
        assert!(migrate(&dir)?.is_empty());
        Ok(())
    }

    #[test]
    fn a_file_in_the_new_place_wins_and_the_old_one_is_left() -> TestResult {
        let dir = TestDir::new("player_files_both");
        std::fs::create_dir_all(dir.join("saves"))?;
        std::fs::create_dir_all(dir.join("settings"))?;
        std::fs::write(dir.join("saves/hotkeys.ron"), "old")?;
        std::fs::write(dir.join("settings/hotkeys.ron"), "new")?;
        assert!(migrate(&dir)?.is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.join("settings/hotkeys.ron"))?,
            "new"
        );
        assert!(dir.join("saves/hotkeys.ron").exists());
        Ok(())
    }
}
