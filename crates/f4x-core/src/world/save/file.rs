//! Reading and writing save files: the file side of [`Snapshot`] (a crash-safe write, the checksum
//! check on reading, the summary a save slot list shows).

use std::fmt;
use std::path::Path;

use crate::world::save::{ContentCheck, Integrity, LoadError, Snapshot};
use crate::world::World;
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
pub fn save_game(path: &Path, world: &World) -> Result<(), SaveError> {
    let text = world.snapshot().to_text().map_err(SaveError::Load)?;
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

/// A save that was read: the game, whether the file was edited, and what the player should be told.
pub struct Loaded {
    pub world: World,
    pub integrity: Integrity,
    /// Rows the game plays without, and rows it was played without that are back
    /// (`Snapshot::restore_with`).
    pub notes: Vec<f4x_def::load::ContentIssue>,
}

/// Read a save and rebuild the game against `content`. The save is checked against its checksum:
/// an edited save still loads, and the caller warns about it. A save whose settings are out of
/// range, or that was made with other content files, is refused (`load_game_with` can ignore the
/// second).
pub fn load_game(path: &Path, content: Content) -> Result<(World, Integrity), SaveError> {
    let loaded = load_game_with(path, content, ContentCheck::Strict)?;
    Ok((loaded.world, loaded.integrity))
}

/// [`load_game`] with the choice of the content check, and the notes for the player.
pub fn load_game_with(
    path: &Path,
    content: Content,
    check: ContentCheck,
) -> Result<Loaded, SaveError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))?;
    // A setting the save does not name comes from the player's default when the caller set one
    // (`with_fallback`), else from the content the save is loaded into: never from a built-in.
    let fallback = content.settings.default_game();
    let read = |content| Snapshot::from_text(&text).and_then(|s| s.restore_with(content, check));
    let (world, notes) = match fallback {
        Some(f) if !f4x_def::settings::has_fallback() => {
            f4x_def::settings::with_fallback(f, || read(content))
        }
        _ => read(content),
    }
    .map_err(SaveError::Load)?;
    Ok(Loaded {
        world,
        integrity: Snapshot::integrity(&text),
        notes,
    })
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
