//! The setup on the author's real copy of the original (`reference/mom-classic`). Takes about half
//! a minute, so it is ignored by default; CI has no original, and the test says so and passes.
//! Run: `cargo test -p f4x-setup --test it real_original -- --ignored`.

use std::fs;
use std::path::{Path, PathBuf};

use f4x_setup::{install, original};

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
    assert!(check.is_complete(), "{}", check.explain());

    let data = std::env::temp_dir().join(format!("f4x_setup_real_{}", std::process::id()));
    let _ = fs::remove_dir_all(&data);
    let mut said = Vec::new();
    let summary =
        install::install_art(&folder, &data, &repo(), &mut |s| said.push(s.to_string())).unwrap();

    assert!(summary.pictures > 10_000, "{summary:?}");
    assert_eq!(said.len(), 3, "{said:?}");
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
