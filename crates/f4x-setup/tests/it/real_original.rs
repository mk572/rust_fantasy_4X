//! The setup on the author's real copy of the original (`reference/mom-classic`). Takes about half
//! a minute, so it is ignored by default; CI has no original, and the test says so and passes.
//! Run: `cargo test -p f4x-setup --test it real_original -- --ignored`.

use std::fs;
use std::path::{Path, PathBuf};

use f4x_setup::say::Say;
use f4x_setup::{install, original, roms, sound};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
#[ignore = "extracts the real original, about 30 seconds"]
fn the_real_original_installs_without_the_unused_pictures() {
    let reference = repo().join("reference/mom-classic/Master of Magic Official Release");
    if !reference.is_dir() {
        eprintln!("no original at {}: nothing to try", reference.display());
        return;
    }
    let folder = original::find(&reference).unwrap();
    let check = original::check(&folder).unwrap();
    let say = Say::load(&repo()).unwrap();
    assert!(check.is_complete(), "{}", check.explain(&say));

    let data = std::env::temp_dir().join(format!("f4x_setup_real_{}", std::process::id()));
    let _ = fs::remove_dir_all(&data);
    let mut said = Vec::new();
    let summary = install::install_art(&folder, &data, &repo(), &mut |s| said.push(s)).unwrap();

    assert!(summary.pictures > 10_000, "{summary:?}");
    assert_eq!(
        said,
        [
            install::Stage::Read,
            install::Stage::Pack,
            install::Stage::Place
        ]
    );
    let assets = data.join("assets-local");
    for part in ["art", "screens", "index.tsv", "packs.tsv"] {
        assert!(assets.join(part).exists(), "{part} is missing");
    }
    // What the game never reads is not written.
    assert!(!assets.join("art/manifest.tsv").exists());
    assert!(!assets.join("art/fonts").exists());
    assert!(!data.join("assets-unused").exists());
    assert!(!data.join(".assets-local.new").exists());
    // A second run replaces the first without leftovers.
    install::install_art(&folder, &data, &repo(), &mut |_| {}).unwrap();
    let _ = fs::remove_dir_all(&data);
}

#[test]
#[ignore = "makes the sound and the music of the real original, about 30 seconds"]
fn the_real_original_gives_its_sound_effects_and_music() {
    let reference = repo().join("reference/mom-classic/Master of Magic Official Release");
    let roms = repo().join("reference/Roland MT-32/rom-new");
    let tool = repo().join("tools/xmi-play/target/release/f4x-music");
    if !reference.is_dir() || roms::check_folder(&roms).is_err() || !tool.is_file() {
        eprintln!("no original, ROMs or built f4x-music: nothing to try");
        return;
    }
    let folder = original::find(&reference).unwrap();
    let data = std::env::temp_dir().join(format!("f4x_setup_sound_{}", std::process::id()));
    let _ = fs::remove_dir_all(&data);

    let effects = sound::install_effects(&folder, &data).unwrap();
    assert!(effects > 100, "{effects}");
    let audio = data.join("assets-local/audio");
    for archive in ["soundfx", "cmbtsnd", "newsound"] {
        assert!(audio.join(archive).is_dir(), "{archive}");
    }
    assert!(
        !audio.join("introsfx").exists(),
        "the game never plays these"
    );

    let mut last = (0, 0);
    let songs =
        sound::install_music(&folder, &data, &roms, &tool, &mut |d, t| last = (d, t)).unwrap();
    assert!(songs > 100, "{songs}");
    assert_eq!(last, (songs, songs));
    assert_eq!(fs::read_dir(audio.join("music")).unwrap().count(), songs);
    // Only the rendered files stay: no songs, no timbre library, no staging folder.
    assert!(!data.join(".audio.new").exists());
    assert!(fs::read_dir(audio.join("music")).unwrap().all(|e| e
        .unwrap()
        .path()
        .extension()
        .is_some_and(|x| x == "ogg")));
    let _ = fs::remove_dir_all(&data);
}
