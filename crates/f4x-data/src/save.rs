//! Reading and writing save files.

use std::fmt;
use std::path::Path;

use f4x_core::content::Content;
use f4x_core::save::{LoadError, Snapshot};
use f4x_core::Game;

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

/// Read a save and rebuild the game against `content`.
pub fn load_game(path: &Path, content: Content) -> Result<Game, SaveError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))?;
    Snapshot::from_text(&text)
        .and_then(|s| s.restore(content))
        .map_err(SaveError::Load)
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_core::city::{PlayerId, Position};
    use f4x_core::map::Plane;
    use f4x_core::mapgen::{generate, GenParams};
    use f4x_core::player::Player;
    use f4x_core::Command;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-saves")
            .join(name);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn content() -> Content {
        crate::load_content(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")).unwrap()
    }

    /// A real generated map with the shipped content, a city, units and a few turns.
    fn game() -> Game {
        let content = content();
        let mut g = Game::new(21);
        g.map = Some(generate(21, &GenParams::default(), &content).unwrap());
        g.content = content;
        g.players.push(Player::new(PlayerId(0), "Merlin"));
        g.init_wizard(
            PlayerId(0),
            &[("life", 2), ("chaos", 1)],
            &[f4x_core::magic::Retort::Alchemy],
        )
        .unwrap();
        let first = g.research_choices(PlayerId(0))[0];
        g.set_research(PlayerId(0), first).unwrap();
        let pos = (0..40u32)
            .flat_map(|y| {
                (0..60u32).map(move |x| Position {
                    plane: Plane::Arcanus,
                    x,
                    y,
                })
            })
            .find(|&p| g.can_found_city(p).is_ok())
            .unwrap();
        let race = g.content.rules.races.id_of("sunfolk");
        g.apply(Command::FoundCity {
            owner: PlayerId(0),
            pos,
            name: "Dawn".into(),
            race,
            population: 3,
        });
        let spear = g.content.rules.units.id_of("spearman").unwrap();
        g.apply(Command::CreateUnit {
            owner: PlayerId(0),
            pos,
            unit: spear,
        });
        for _ in 0..3 {
            g.apply(Command::EndTurn);
        }
        g
    }

    #[test]
    fn a_game_survives_the_file_round_trip() {
        let g = game();
        let path = dir("round-trip").join("game.ron");
        save_game(&path, &g).unwrap();
        assert!(
            !path.with_extension("ron.tmp").exists(),
            "no temporary file is left behind"
        );
        let back = load_game(&path, content()).unwrap();
        assert_eq!(back.snapshot(), g.snapshot());
        assert_eq!(back.turn, 4);
        assert_eq!(back.magic(PlayerId(0)), g.magic(PlayerId(0)));
        assert!(back.magic(PlayerId(0)).unwrap().researching.is_some());
    }

    #[test]
    fn saving_over_a_save_replaces_it() {
        let mut g = game();
        let path = dir("overwrite").join("game.ron");
        save_game(&path, &g).unwrap();
        g.apply(Command::EndTurn);
        save_game(&path, &g).unwrap();
        assert_eq!(load_game(&path, content()).unwrap().turn, 5);
    }

    #[test]
    fn missing_and_damaged_files_give_clear_errors() {
        let d = dir("errors");
        let missing = load_game(&d.join("nope.ron"), content()).err().unwrap();
        assert!(matches!(missing, SaveError::Io(_)), "{missing}");
        let path = d.join("cut.ron");
        save_game(&path, &game()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, &text[..text.len() / 2]).unwrap();
        let cut = load_game(&path, content()).err().unwrap();
        assert!(matches!(cut, SaveError::Load(LoadError::Parse(_))), "{cut}");
    }
}
