//! `refile-art`: move the pictures `extract-art` already wrote to where its current rules put
//! them, so a change of the rules (a new spell folder, a screen) does not need the original game
//! files read again. Only a picture whose place changed is touched; `index.tsv` is rewritten.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::screens::Placer;

/// The files one picture may have for each frame: the picture, its colour-cycle mask and its
/// banner mask.
const SUFFIXES: [&str; 3] = ["", "_cycle", "_banner"];

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
        .map_err(|e| format!("{}: {e} (run extract-art first)", art_dir.display()))?;
    let mut moved = 0;
    let mut missing = 0;
    for line in manifest.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(&lbx), Some(Ok(entry)), Some(Ok(frames))) = (
            f.first(),
            f.get(1).map(|v| v.parse::<usize>()),
            f.get(2).map(|v| v.parse::<usize>()),
        ) else {
            continue;
        };
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
    use f4x_core::testdir::TestResult;
    use f4x_data::spell_art::parse_spell_art;

    #[test]
    fn a_picture_moves_to_its_spell_folder_with_its_masks_and_the_index_follows() -> TestResult {
        let root = f4x_core::testdir::TestDir::new("refile");
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
        let placer = Placer::new(crate::screens::rules()?, &[], &art);
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
        Ok(())
    }
}
