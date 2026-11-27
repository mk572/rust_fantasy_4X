//! A world in a file: a crash-safe write and the checksum check on reading. A saved game is a
//! world together with the game's own state, and `f4x-master` reads and writes it.

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

/// Write the world to `path` (see [`write_text`]).
pub fn save_game(path: &Path, world: &World) -> Result<(), SaveError> {
    let text = world.snapshot().to_text().map_err(SaveError::Load)?;
    write_text(path, &text)
}

/// Write `text` to `path`. It goes to a temporary file next to it and is renamed into place, so a
/// crash while saving never leaves a half-written save over a good one.
pub fn write_text(path: &Path, text: &str) -> Result<(), SaveError> {
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

/// The text of a save file.
pub fn read_text(path: &Path) -> Result<String, SaveError> {
    std::fs::read_to_string(path).map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))
}

/// Run `read` on a save loaded into `content`: a setting the save does not name comes from the
/// player's default when the caller set one (`with_fallback`), else from the content the save is
/// loaded into: never from a built-in.
pub fn with_content_fallback<T>(
    content: &Content,
    read: impl FnOnce() -> Result<T, LoadError>,
) -> Result<T, SaveError> {
    match content.settings.default_game() {
        Some(f) if !f4x_def::settings::has_fallback() => f4x_def::settings::with_fallback(f, read),
        _ => read(),
    }
    .map_err(SaveError::Load)
}

/// Read a saved world and rebuild it against `content`. The file is checked against its checksum:
/// an edited one still loads, and the caller warns about it. One whose settings are out of range,
/// or that was made with other content files, is refused.
pub fn load_game(path: &Path, content: Content) -> Result<(World, Integrity), SaveError> {
    let text = read_text(path)?;
    // The content is moved into the closure, so the fallback is taken from a handle of it.
    let content = std::sync::Arc::new(content);
    let world = with_content_fallback(&content, || {
        Snapshot::from_text(&text)?
            .restore_with(content.clone(), ContentCheck::Strict)
            .map(|(world, _)| world)
    })?;
    Ok((world, Snapshot::integrity(&text)))
}
