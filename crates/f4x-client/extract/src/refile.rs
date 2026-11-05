//! `f4x-extract refile`: move the pictures `f4x-extract art` already wrote to where its current rules put
//! them, so a change of the rules (a new spell folder, a screen) does not need the original game
//! files read again. Only a picture whose place changed is touched; `index.tsv` is rewritten.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::screens::Placer;

/// The files one picture may have for each frame: the picture, its colour-cycle mask, its banner
/// mask and the eight steps of the node box's colour slide.
const SUFFIXES: [&str; 11] = [
    "", "_cycle", "_banner", "_slide_0", "_slide_1", "_slide_2", "_slide_3", "_slide_4",
    "_slide_5", "_slide_6", "_slide_7",
];

/// Where a picture's frames are: a folder and the file name without the frame.
fn location(
    art_dir: &Path,
    root: &Path,
    lbx: &str,
    entry: usize,
    place: Option<&str>,
) -> (PathBuf, String) {
    match place {
        Some(place) => {
            let (folder, name) = place.rsplit_once('/').unwrap_or(("", place));
            (root.join(folder), name.to_string())
        }
        None => (art_dir.join(lbx), format!("{entry:03}")),
    }
}

/// The pictures that have files in `art/<lbx>/`: `(lbx, entry)` and the number of frames.
fn loose_pictures(art_dir: &Path) -> BTreeMap<(String, usize), usize> {
    let mut found = BTreeMap::new();
    let Ok(dirs) = fs::read_dir(art_dir) else {
        return found;
    };
    for dir in dirs.flatten().filter(|d| d.path().is_dir()) {
        let lbx = dir.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            // `<entry>_<frame>.png`, three and two digits; the masks have a suffix and are skipped.
            let Some((entry, frame)) = name.strip_suffix(".png").and_then(|n| n.split_once('_'))
            else {
                continue;
            };
            if let (3, 2, Ok(entry), Ok(frame)) = (
                entry.len(),
                frame.len(),
                entry.parse::<usize>(),
                frame.parse::<usize>(),
            ) {
                let frames = found.entry((lbx.clone(), entry)).or_insert(0);
                *frames = (*frames).max(frame + 1);
            }
        }
    }
    found
}

/// Move what has to move. Returns how many pictures moved.
pub fn refile(art_dir: &Path, placer: &Placer) -> Result<usize, String> {
    let root = art_dir.with_file_name("");
    let index_path = root.join("index.tsv");
    let mut index: BTreeMap<(String, usize), String> = BTreeMap::new();
    for line in fs::read_to_string(&index_path).unwrap_or_default().lines() {
        let mut f = line.split('\t');
        if let (Some(lbx), Some(Ok(entry)), Some(place)) =
            (f.next(), f.next().map(str::parse::<usize>), f.next())
        {
            index.insert((lbx.to_string(), entry), place.to_string());
        }
    }
    let manifest = fs::read_to_string(art_dir.join("manifest.tsv"))
        .map_err(|e| format!("{}: {e} (run f4x-extract art first)", art_dir.display()))?;
    let mut pictures: BTreeMap<(String, usize), usize> = BTreeMap::new();
    for line in manifest.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if let (Some(&lbx), Some(Ok(entry)), Some(Ok(frames))) = (
            f.first(),
            f.get(1).map(|v| v.parse::<usize>()),
            f.get(2).map(|v| v.parse::<usize>()),
        ) {
            pictures.insert((lbx.to_string(), entry), frames);
        }
    }
    // Pictures a script made beside the extracted ones (`make_banner_colours.py` writes more gems
    // into `lilwiz/`) are not in the manifest; the files in `art/<lbx>/` say what they are.
    for (key, frames) in loose_pictures(art_dir) {
        pictures.entry(key).or_insert(frames);
    }
    let mut moved = 0;
    let mut missing = 0;
    for ((lbx, entry), frames) in pictures {
        let lbx = lbx.as_str();
        let key = (lbx.to_string(), entry);
        let new = placer.place(lbx, entry);
        if index.get(&key) == new.as_ref() {
            continue;
        }
        let (from_dir, from_name) = location(
            art_dir,
            &root,
            lbx,
            entry,
            index.get(&key).map(String::as_str),
        );
        let (to_dir, to_name) = location(art_dir, &root, lbx, entry, new.as_deref());
        fs::create_dir_all(&to_dir).map_err(|e| e.to_string())?;
        let mut any = false;
        for frame in 0..frames {
            for suffix in SUFFIXES {
                let from = from_dir.join(format!("{from_name}_{frame:02}{suffix}.png"));
                if from.exists() {
                    let to = to_dir.join(format!("{to_name}_{frame:02}{suffix}.png"));
                    fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
                    any = true;
                }
            }
        }
        if any {
            moved += 1;
        } else {
            missing += 1;
        }
        match new {
            Some(place) => index.insert(key, place),
            None => index.remove(&key),
        };
    }
    let text: String = index
        .iter()
        .map(|((lbx, entry), place)| format!("{lbx}\t{entry}\t{place}\n"))
        .collect();
    fs::write(&index_path, text).map_err(|e| e.to_string())?;
    // A folder `art/<lbx>/` is dropped once every picture in it has moved out.
    if let Ok(dirs) = fs::read_dir(art_dir) {
        for d in dirs.flatten() {
            let _ = fs::remove_dir(d.path());
        }
    }
    if missing > 0 {
        eprintln!("{missing} pictures had no files to move (not extracted?)");
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spell_art::parse_spell_art;
    use crate::testutil::TestResult;

    #[test]
    fn a_picture_moves_to_its_spell_folder_with_its_masks_and_the_index_follows() -> TestResult {
        // A folder of this test's own: the process id keeps parallel test runs apart.
        let root = std::env::temp_dir().join(format!("f4x-extract-refile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let art_dir = root.join("art");
        fs::create_dir_all(art_dir.join("cmbtfx"))?;
        for name in ["000_00.png", "000_01.png", "000_01_cycle.png", "001_00.png"] {
            fs::write(art_dir.join("cmbtfx").join(name), b"x")?;
        }
        fs::write(
            art_dir.join("manifest.tsv"),
            "cmbtfx\t0\t2\t60\t54\ncmbtfx\t1\t1\t60\t54\n",
        )?;
        fs::write(root.join("index.tsv"), "")?;
        let art =
            parse_spell_art(r#"(spells: {"fire_bolt": [(lbx: "cmbtfx", entry: 0, role: Cast)]})"#)?;
        let placer = Placer::new(crate::screens::rules()?, &[], &art, &[]);
        assert_eq!(refile(&art_dir, &placer)?, 1);
        let to = art_dir.join("spells/fire_bolt");
        assert!(to.join("cmbtfx_000_00.png").exists());
        assert!(to.join("cmbtfx_000_01.png").exists());
        assert!(to.join("cmbtfx_000_01_cycle.png").exists());
        assert!(!art_dir.join("cmbtfx/000_00.png").exists());
        assert!(art_dir.join("cmbtfx/001_00.png").exists());
        let index = fs::read_to_string(root.join("index.tsv"))?;
        assert_eq!(index, "cmbtfx\t0\tart/spells/fire_bolt/cmbtfx_000\n");
        // Run again: nothing is left to move.
        assert_eq!(refile(&art_dir, &placer)?, 0);
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn a_picture_a_script_made_beside_the_extracted_ones_is_filed_too() -> TestResult {
        let root = std::env::temp_dir().join(format!("f4x-extract-loose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let art_dir = root.join("art");
        fs::create_dir_all(art_dir.join("lilwiz"))?;
        // Entry 5 is in the manifest; entry 105 (a new colour) is only a file.
        fs::write(art_dir.join("lilwiz/005_00.png"), b"x")?;
        fs::write(art_dir.join("lilwiz/105_00.png"), b"x")?;
        fs::write(art_dir.join("lilwiz/105_00_cycle.png"), b"x")?;
        fs::write(art_dir.join("manifest.tsv"), "lilwiz\t5\t1\t40\t40\n")?;
        fs::write(root.join("index.tsv"), "")?;
        let wizards = crate::wizard_art::parse_wizard_art(
            r#"[(id: "merlin", pictures: (gem: 0)), (id: "raven", pictures: (gem: 5))]"#,
        )?;
        let placer = Placer::new(
            crate::screens::rules()?,
            &[],
            &crate::spell_art::SpellArt::default(),
            &wizards,
        );
        assert_eq!(refile(&art_dir, &placer)?, 2);
        let folder = root.join("art/wizards/raven");
        assert!(folder.join("gem_0_00.png").exists());
        assert!(folder.join("gem_5_00.png").exists());
        assert!(folder.join("gem_5_00_cycle.png").exists());
        let index = fs::read_to_string(root.join("index.tsv"))?;
        assert_eq!(
            index,
            "lilwiz\t5\tart/wizards/raven/gem_0\nlilwiz\t105\tart/wizards/raven/gem_5\n"
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
