use std::fs;
use std::path::{Path, PathBuf};

use f4x_extract::{art, pack_build, paths, spell_art, unit_art, wizard_art};
use f4x_lang::text::Args;

use crate::say::Say;

/// Where the pictures go, inside the data folder.
pub const ASSETS: &str = "assets-local";
/// The folder the pictures are made in; it replaces the old ones only when all is done.
const STAGING: &str = ".assets-local.new";
/// The parts of `assets-local/` this step makes (the sound has its own step).
const PARTS: [&str; 4] = ["art", "screens", "index.tsv", "packs.tsv"];

/// The stages of the work, reported as each starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Read,
    Pack,
    Place,
}

impl Stage {
    /// The key of the sentence that tells the player what is happening.
    pub fn key(self) -> &'static str {
        match self {
            Stage::Read => "setup.stage_read",
            Stage::Pack => "setup.stage_pack",
            Stage::Place => "setup.stage_place",
        }
    }
}

/// Why the work stopped.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// The pictures folder is a link; the work would end up somewhere else.
    Link { folder: PathBuf, target: PathBuf },
    /// Anything else, with what the extractor or the disk said.
    Failed(String),
}

impl InstallError {
    /// The problem in plain words.
    pub fn explain(&self, say: &Say) -> String {
        match self {
            InstallError::Link { folder, target } => say.get(
                "setup.err_link",
                &Args::new()
                    .text("folder", folder.display().to_string())
                    .text("target", target.display().to_string()),
            ),
            InstallError::Failed(why) => {
                say.get("setup.err_failed", &Args::new().text("why", why.clone()))
            }
        }
    }
}

impl From<String> for InstallError {
    fn from(why: String) -> Self {
        InstallError::Failed(why)
    }
}

/// What was made.
#[derive(Debug)]
pub struct Summary {
    pub pictures: usize,
    pub packs: usize,
    pub folder: PathBuf,
}

/// `data/assets-local`, after checking that it is not a link: the work would end up somewhere else
/// (the author's own working folder, for one).
pub fn checked_assets(data: &Path) -> Result<PathBuf, InstallError> {
    let assets = data.join(ASSETS);
    if fs::symlink_metadata(&assets).is_ok_and(|m| m.file_type().is_symlink()) {
        let target = fs::read_link(&assets).unwrap_or_default();
        return Err(InstallError::Link {
            folder: assets,
            target,
        });
    }
    Ok(assets)
}

/// Extract the pictures of the original in `original` into `data/assets-local/`.
///
/// `game` is the folder with the game's own files (`crates/...`), which hold the tables saying which
/// picture each game element uses. `stage` is told when each stage starts.
pub fn install_art(
    original: &Path,
    data: &Path,
    game: &Path,
    stage: &mut dyn FnMut(Stage),
) -> Result<Summary, InstallError> {
    let assets = checked_assets(data)?;
    paths::set_game_dir(game.to_path_buf());
    let client_content = game.join("crates/f4x-client/content");
    let content = [client_content.as_path()];
    let units = unit_art::load_unit_art(&content)?;
    let spells = spell_art::load_spell_art(&content)?;
    let wizards = wizard_art::load_wizard_art(&content)?;

    let staging = data.join(STAGING);
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    }
    fs::create_dir_all(staging.join("art")).map_err(|e| format!("{}: {e}", staging.display()))?;

    stage(Stage::Read);
    art::extract_with(
        original,
        &staging.join("art"),
        &units,
        &spells,
        &wizards,
        true,
    )?;

    stage(Stage::Pack);
    let packed = pack_build::pack_tree(&staging, &game.join("crates/f4x-def/content"), false)?;

    // What the game never reads: the list of what was extracted and the font pictures (the game
    // has its own fonts).
    let _ = fs::remove_file(staging.join("art/manifest.tsv"));
    let _ = fs::remove_dir_all(staging.join("art/fonts"));

    stage(Stage::Place);
    fs::create_dir_all(&assets).map_err(|e| format!("{}: {e}", assets.display()))?;
    for part in PARTS {
        let old = assets.join(part);
        if old.is_dir() {
            fs::remove_dir_all(&old).map_err(|e| format!("{}: {e}", old.display()))?;
        } else if old.exists() {
            fs::remove_file(&old).map_err(|e| format!("{}: {e}", old.display()))?;
        }
        let new = staging.join(part);
        if new.exists() {
            fs::rename(&new, &old).map_err(|e| format!("{}: {e}", old.display()))?;
        }
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(Summary {
        pictures: packed.pictures,
        packs: packed.packs,
        folder: assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_refuses_to_write_through_a_link() {
        let data = std::env::temp_dir().join(format!("f4x_setup_link_{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        fs::create_dir_all(data.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(data.join("elsewhere"), data.join(ASSETS)).unwrap();
        let err = install_art(
            Path::new("/nowhere"),
            &data,
            Path::new("/nowhere"),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Link { .. }), "{err:?}");
        let text = err.explain(&crate::say::test_say());
        assert!(text.contains("is a link to"), "{text}");
        assert!(text.contains("will not write through it"), "{text}");
        assert!(data.join("elsewhere").read_dir().unwrap().next().is_none());
    }
}
