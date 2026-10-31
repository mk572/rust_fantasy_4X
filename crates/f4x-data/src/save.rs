//! Reading and writing save files.

use std::fmt;
use std::path::Path;

use f4x_core::content::Content;
use f4x_core::game::save::{Integrity, LoadError, Snapshot};
use f4x_core::game::Game;

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
    use f4x_core::game::Command;
    use f4x_core::map::mapgen::{generate, GenParams};
    use f4x_core::map::Plane;
    use f4x_core::map::Position;
    use f4x_core::player::PlayerId;
    use f4x_core::testdir::TestDir;
    use f4x_core::testdir::TestResult;

    fn content() -> Content {
        crate::load_content_layers(&[&crate::testutil::content_dir()])
            .expect("the shipped content loads")
    }

    /// A real generated map with the shipped content, a city, units and a few turns.
    fn game() -> Result<Game, Box<dyn std::error::Error>> {
        let content = content();
        let params = GenParams::default();
        let mut g = Game::new(21);
        g.set_generated(generate(21, &params, &content)?);
        g.set_content(content);
        g.add_player(PlayerId(0), "Merlin");
        g.init_wizard(
            PlayerId(0),
            &[("life", 2), ("chaos", 1)],
            &[f4x_core::wizard::magic::Retort::Alchemy],
            None,
        )
        .map_err(|e| format!("init_wizard: {e:?}"))?;
        let first = g
            .wizard(PlayerId(0))
            .ok_or("the wizard is missing")?
            .candidates()[0];
        g.set_research(PlayerId(0), first)
            .map_err(|e| format!("set_research: {e:?}"))?;
        let pos = (0..params.height)
            .flat_map(|y| {
                (0..params.width).map(move |x| Position {
                    plane: Plane::Arcanus,
                    x,
                    y,
                })
            })
            .find(|&p| g.can_found_city(p).is_ok())
            .ok_or("the map has no square for a city")?;
        let race = g.content().rules.races.id_of("barbarian");
        g.apply(Command::FoundCity {
            owner: PlayerId(0),
            pos,
            name: "Dawn".into(),
            race,
            population: 3,
        });
        let spear = g
            .content()
            .rules
            .units
            .id_of("barbarian_spearmen")
            .ok_or("barbarian_spearmen is not in the content")?;
        g.apply(Command::CreateUnit {
            owner: PlayerId(0),
            pos,
            unit: spear,
        });
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
            .wizard(PlayerId(0))
            .ok_or("the value is missing")?
            .researching()
            .is_some());
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
        assert_eq!(s.wizard, "Merlin");
        assert_eq!(s.turn, g.turn);
        assert!(!s.finished);
        std::fs::write(&path, "not a save")?;
        assert!(summarize_save(&path).is_err());
        assert!(summarize_save(&folder.join("missing.ron")).is_err());
        Ok(())
    }
}
