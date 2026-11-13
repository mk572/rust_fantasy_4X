//! Reading pictures out of sprite packs (`f4x_extract::pack`, `docs/local-assets.md`).
//!
//! `packs.tsv` in the assets folder (the parent of the art folder) says which pack holds the
//! pictures of each folder. `read` answers a picture's bytes as an image when a pack has it, and
//! `None` otherwise, so the caller goes on to the loose file. A pack is opened when a picture of its
//! folder is first asked for; the last few decoded sheets are kept, because a unit's frames are
//! asked for one after another.
//!
//! Tests: the path rule and a pack read end to end, in this file.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use f4x_extract::pack::Pack;
use macroquad::prelude::{Image, ImageFormat};

/// Decoded sheets kept at once: a few megabytes each at most.
const KEPT_SHEETS: usize = 8;

struct PackSet {
    root: PathBuf,
    /// Folder (relative to the root) to its pack file.
    folders: HashMap<String, String>,
    packs: HashMap<String, Option<Rc<Pack>>>,
    /// Decoded sheets, oldest first: `(pack, sheet)` and its pixels.
    sheets: Vec<((String, u16), Rc<Image>)>,
}

thread_local! {
    static SETS: RefCell<HashMap<PathBuf, PackSet>> = RefCell::new(HashMap::new());
}

/// `relative` joined to `base` and cleaned of `.` and `..`, with `/` separators; `None` when it
/// climbs out of the root.
fn clean(base: &str, relative: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let joined = format!("{base}/{}", relative.to_string_lossy().replace('\\', "/"));
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other.to_string()),
        }
    }
    Some(parts.join("/"))
}

impl PackSet {
    fn open(root: &Path) -> PackSet {
        let folders = std::fs::read_to_string(root.join("packs.tsv"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(d, p)| (d.to_string(), p.trim().to_string()))
            .collect();
        PackSet {
            root: root.to_path_buf(),
            folders,
            packs: HashMap::new(),
            sheets: Vec::new(),
        }
    }

    fn pack(&mut self, file: &str) -> Option<Rc<Pack>> {
        let root = &self.root;
        self.packs
            .entry(file.to_string())
            .or_insert_with(|| {
                let bytes = std::fs::read(root.join(file)).ok()?;
                match Pack::parse(bytes) {
                    Ok(pack) => Some(Rc::new(pack)),
                    Err(e) => {
                        eprintln!("{file}: {e}");
                        None
                    }
                }
            })
            .clone()
    }

    fn sheet(&mut self, file: &str, pack: &Pack, index: u16) -> Option<Rc<Image>> {
        let key = (file.to_string(), index);
        if let Some(at) = self.sheets.iter().position(|(k, _)| *k == key) {
            let kept = self.sheets.remove(at);
            let image = kept.1.clone();
            self.sheets.push(kept);
            return Some(image);
        }
        let png = pack.sheet_png(index)?;
        let image = Rc::new(Image::from_file_with_format(png, Some(ImageFormat::Png)).ok()?);
        if self.sheets.len() >= KEPT_SHEETS {
            self.sheets.remove(0);
        }
        self.sheets.push((key, image.clone()));
        Some(image)
    }

    /// The picture at `key` (`art/units/orc_archers/figure_0_00.png`), cut out of its sheet.
    fn read(&mut self, key: &str) -> Option<Image> {
        let (folder, name) = key.rsplit_once('/')?;
        let file = self.folders.get(folder)?.clone();
        let pack = self.pack(&file)?;
        let frame = pack.frame(name)?;
        let sheet = self.sheet(&file, &pack, frame.sheet)?;
        let (x, y, w, h) = (
            usize::from(frame.x),
            usize::from(frame.y),
            usize::from(frame.w),
            usize::from(frame.h),
        );
        let stride = usize::from(sheet.width) * 4;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            let from = row * stride + x * 4;
            bytes.extend_from_slice(sheet.bytes.get(from..from + w * 4)?);
        }
        Some(Image {
            bytes,
            width: frame.w,
            height: frame.h,
        })
    }
}

/// The picture `relative` (relative to the art folder `dir`) from a sprite pack, if one has it.
pub fn read(dir: &Path, relative: &Path) -> Option<Image> {
    let root = dir.parent()?;
    let base = dir.file_name()?.to_str()?;
    let key = clean(base, relative)?;
    SETS.with(|sets| {
        sets.borrow_mut()
            .entry(root.to_path_buf())
            .or_insert_with(|| PackSet::open(root))
            .read(&key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_extract::pack_build::{build, Picture};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_path_is_cleaned_against_the_art_folder() {
        assert_eq!(
            clean("art", Path::new("units/orc/figure_0_00.png")).as_deref(),
            Some("art/units/orc/figure_0_00.png")
        );
        assert_eq!(
            clean("art", Path::new("../screens/main_screen/a_00.png")).as_deref(),
            Some("screens/main_screen/a_00.png")
        );
        assert_eq!(clean("art", Path::new("../../outside.png")), None);
    }

    #[test]
    fn a_picture_comes_out_of_its_pack_and_an_unknown_one_does_not() -> TestResult {
        let root = std::env::temp_dir().join(format!("f4x_pack_set_{}", std::process::id()));
        std::fs::create_dir_all(root.join("art/units/orc"))?;
        let pictures = vec![
            Picture {
                name: "figure_0_00.png".into(),
                w: 3,
                h: 2,
                rgba: (0..24).collect(),
            },
            Picture {
                name: "figure_0_01.png".into(),
                w: 2,
                h: 2,
                rgba: (100..116).collect(),
            },
        ];
        std::fs::write(
            root.join("art/units/orc/orc_archers.pack"),
            build(&pictures)?,
        )?;
        std::fs::write(
            root.join("packs.tsv"),
            "art/units/orc_archers\tart/units/orc/orc_archers.pack\n",
        )?;
        let art = root.join("art");
        let got = read(&art, Path::new("units/orc_archers/figure_0_01.png"));
        let got = got.ok_or("no picture")?;
        assert_eq!((got.width, got.height), (2, 2));
        assert_eq!(got.bytes, (100..116).collect::<Vec<u8>>());
        assert!(read(&art, Path::new("units/orc_archers/nope.png")).is_none());
        assert!(read(&art, Path::new("units/other/figure_0_00.png")).is_none());
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
}
