//! Extracting the pictures of the original into the player's data folder.

use std::fs;
use std::path::{Path, PathBuf};

use f4x_extract::{art, pack_build, paths, spell_art, unit_art, wizard_art};

/// Where the pictures go, inside the data folder.
pub const ASSETS: &str = "assets-local";
/// The folder the pictures are made in; it replaces the old ones only when all is done.
const STAGING: &str = ".assets-local.new";
/// The parts of `assets-local/` this step makes (the sound has its own step).
const PARTS: [&str; 4] = ["art", "screens", "index.tsv", "packs.tsv"];

/// What was made.
#[derive(Debug)]
pub struct Summary {
    pub pictures: usize,
    pub packs: usize,
    pub folder: PathBuf,
}

/// Extract the pictures of the original in `original` into `data/assets-local/`.
///
/// `game` is the folder with the game's own files (`crates/...`), which hold the tables saying which
/// picture each game element uses. `say` gets one plain sentence at the start of each stage.
pub fn install_art(
    original: &Path,
    data: &Path,
    game: &Path,
    say: &mut dyn FnMut(&str),
) -> Result<Summary, String> {
    let assets = data.join(ASSETS);
    // A link would send the result somewhere else (the author's own working folder, for one).
    if fs::symlink_metadata(&assets).is_ok_and(|m| m.file_type().is_symlink()) {
        let target = fs::read_link(&assets).unwrap_or_default();
        return Err(format!(
            "{} is a link to {}. Setup will not write through it: remove the link, or choose \
             another data folder.",
            assets.display(),
            target.display()
        ));
    }
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

    say("Reading the pictures of the original game. This takes a few minutes.");
    art::extract_with(
        original,
        &staging.join("art"),
        &units,
        &spells,
        &wizards,
        true,
    )?;

    say("Packing the pictures.");
    let packed = pack_build::pack_tree(&staging, &game.join("crates/f4x-def/content"), false)?;

    // What the game never reads: the list of what was extracted and the font pictures (the game
    // has its own fonts).
    let _ = fs::remove_file(staging.join("art/manifest.tsv"));
    let _ = fs::remove_dir_all(staging.join("art/fonts"));

    say("Putting the pictures in place.");
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
        assert!(err.contains("is a link to"), "{err}");
        assert!(err.contains("will not write through it"), "{err}");
        assert!(data.join("elsewhere").read_dir().unwrap().next().is_none());
    }
}
