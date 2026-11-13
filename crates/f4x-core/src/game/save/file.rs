//! Reading and writing save files: the file side of [`Snapshot`] (a crash-safe write, the checksum
//! check on reading, the summary a save slot list shows).

use std::fmt;
use std::path::Path;

use crate::game::save::{Integrity, LoadError, Snapshot};
use crate::game::Game;
use f4x_def::content::Content;

#[derive(Debug)]
pub enum SaveError {
    Io(String),
    Load(LoadError),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(m) => write!(f, "save file problem: {m}"),
            SaveError::Load(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Write the game to `path`. The text goes to a temporary file next to it and is renamed
/// into place, so a crash while saving never leaves a half-written save over a good one.
pub fn save_game(path: &Path, game: &Game) -> Result<(), SaveError> {
    let text = game.snapshot().to_text().map_err(SaveError::Load)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| SaveError::Io(format!("{}: {e}", dir.display())))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| SaveError::Io(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))
}

/// Read a save and rebuild the game against `content`. The save is checked against its checksum:
/// an edited save still loads, and the caller warns about it.
pub fn load_game(path: &Path, content: Content) -> Result<(Game, Integrity), SaveError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))?;
    let game = Snapshot::from_text(&text)
        .and_then(|s| s.restore(content))
        .map_err(SaveError::Load)?;
    Ok((game, Snapshot::integrity(&text)))
}

/// What a save slot list shows about a save without rebuilding the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSummary {
    /// The human wizard's name.
    pub wizard: String,
    pub turn: u32,
    /// The game had ended when it was saved.
    pub finished: bool,
}

/// Read just enough of a save to describe it.
pub fn summarize_save(path: &Path) -> Result<SaveSummary, SaveError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))?;
    let snapshot = Snapshot::from_text(&text).map_err(SaveError::Load)?;
    let human = snapshot.human.unwrap_or(0);
    let wizard = snapshot
        .wizards
        .iter()
        .find(|w| w.player == human)
        .or_else(|| snapshot.wizards.first())
        .map_or_else(String::new, |w| w.name.clone());
    Ok(SaveSummary {
        wizard,
        turn: snapshot.turn,
        finished: snapshot.game_over.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::{fresh, shipped};
    use crate::game::Command;
    use f4x_def::testdir::TestDir;
    use f4x_def::testdir::TestResult;

    fn content() -> Content {
        shipped()
    }

    /// A new game on a real map with its wizards, cities and units, and a few turns played.
    fn game() -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = fresh(2, 7)?;
        // The first wizard researches something, so the file has a research in it.
        let first = g.player_ids().into_iter().find(|&p| g.wizard(p).is_some());
        let first = first.ok_or("no wizard")?;
        let spell = g.wizard(first).ok_or("no wizard")?.candidates()[0];
        g.set_research(first, spell)
            .map_err(|e| format!("set_research: {e:?}"))?;
        for _ in 0..3 {
            g.apply(Command::EndTurn);
        }
        Ok(g)
    }

    #[test]
    fn a_game_survives_the_file_round_trip() -> TestResult {
        let g = game()?;
        let folder = TestDir::new("round-trip");
        let path = folder.join("game.ron");
        save_game(&path, &g)?;
        assert!(
            !path.with_extension("ron.tmp").exists(),
            "no temporary file is left behind"
        );
        let (back, integrity) = load_game(&path, content())?;
        assert_eq!(integrity, Integrity::Intact);
        assert_eq!(back.snapshot(), g.snapshot());
        assert_eq!(back.turn, 4);
        assert!(back
            .player_ids()
            .into_iter()
            .filter_map(|p| back.wizard(p))
            .any(|w| w.researching().is_some()));
        Ok(())
    }

    #[test]
    fn saving_over_a_save_replaces_it() -> TestResult {
        let mut g = game()?;
        let folder = TestDir::new("overwrite");
        let path = folder.join("game.ron");
        save_game(&path, &g)?;
        g.apply(Command::EndTurn);
        save_game(&path, &g)?;
        assert_eq!(load_game(&path, content())?.0.turn, 5);
        Ok(())
    }

    #[test]
    fn missing_and_damaged_files_give_clear_errors() -> TestResult {
        let d = TestDir::new("errors");
        let missing = load_game(&d.join("nope.ron"), content())
            .err()
            .ok_or("the value is missing")?;
        assert!(matches!(missing, SaveError::Io(_)), "{missing}");
        let path = d.join("cut.ron");
        save_game(&path, &game()?)?;
        let text = std::fs::read_to_string(&path)?;
        std::fs::write(&path, &text[..text.len() / 2])?;
        let cut = load_game(&path, content())
            .err()
            .ok_or("the value is missing")?;
        assert!(matches!(cut, SaveError::Load(LoadError::Parse(_))), "{cut}");
        Ok(())
    }

    #[test]
    fn a_save_can_be_summarized_without_loading_it() -> TestResult {
        let g = game()?;
        let folder = TestDir::new("summary");
        let path = folder.join("s.ron");
        save_game(&path, &g)?;
        let s = summarize_save(&path)?;
        // Nobody is human in this game: the first wizard stands for it.
        let first = g.player_ids().into_iter().find_map(|p| g.wizard(p));
        assert_eq!(Some(s.wizard), first.map(|w| w.name().to_string()));
        assert_eq!(s.turn, g.turn);
        assert!(!s.finished);
        std::fs::write(&path, "not a save")?;
        assert!(summarize_save(&path).is_err());
        assert!(summarize_save(&folder.join("missing.ron")).is_err());
        Ok(())
    }
}
