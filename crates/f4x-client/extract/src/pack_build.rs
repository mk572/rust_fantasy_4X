//! Writing sprite packs (`pack.rs` has the file layout): lay the pictures of a folder out on PNG
//! sheets, check that every picture reads back the same, and replace the loose files by the pack.
//!
//! `pack_tree` is the command `f4x-extract pack`. Where a folder is packed to:
//! - `art/units/<unit>/` goes to `art/units/<folder>/<unit>.pack`, where the folder is the unit's
//!   race (`high_elf/`), or `hero/`, `fantastic/` or `ship/` for a unit without one;
//! - any other folder `D/` goes to `D.pack`, beside it.
//!
//! `packs.tsv` in the root lists `folder<TAB>pack` for every pack, both relative to the root; the
//! client reads it once and opens a pack when a picture of its folder is first asked for. A folder
//! packed again merges: a loose file replaces the picture of the same name in its old pack.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pack::{encode, Frame, Pack};

/// The most a sheet takes in either direction: small enough for any graphics card, big enough for
/// a unit's 66 pictures in one piece.
const SHEET_MAX: usize = 2048;

/// A picture as RGBA bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Decode PNG bytes to 8-bit RGBA: `(width, height, bytes)`.
pub fn decode_png(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err("indexed PNG left unexpanded".to_string()),
    };
    if rgba.len() != w * h * 4 {
        return Err("PNG data of the wrong size".to_string());
    }
    Ok((w, h, rgba))
}

/// PNG bytes of an RGBA image (the same settings as the loose files).
pub fn encode_png(w: usize, h: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    enc.set_filter(png::FilterType::Paeth);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Where each picture goes on which sheet (shelf packing, tallest first), and each sheet's size.
/// A picture bigger than a sheet gets a sheet of its own.
fn layout(sizes: &[(usize, usize)]) -> (Vec<Frame>, Vec<(usize, usize)>) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| {
        (
            std::cmp::Reverse(sizes[i].1),
            std::cmp::Reverse(sizes[i].0),
            i,
        )
    });
    let mut frames = vec![
        Frame {
            sheet: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        sizes.len()
    ];
    let mut sheets: Vec<(usize, usize)> = Vec::new();
    // The shelf being filled on the last sheet: its next x, its top y and its height.
    let (mut x, mut y, mut shelf) = (0usize, 0usize, 0usize);
    for i in order {
        let (w, h) = sizes[i];
        let fits_a_sheet = w <= SHEET_MAX && h <= SHEET_MAX;
        let new_sheet = sheets.is_empty() || !fits_a_sheet || {
            if x + w > SHEET_MAX {
                y += shelf;
                x = 0;
                shelf = 0;
            }
            y + h > SHEET_MAX
        };
        if new_sheet {
            sheets.push((0, 0));
            (x, y, shelf) = (0, 0, 0);
        }
        let sheet = sheets.len() - 1;
        frames[i] = Frame {
            sheet: sheet as u16,
            x: x as u16,
            y: y as u16,
            w: w as u16,
            h: h as u16,
        };
        shelf = shelf.max(h);
        x += w;
        let size = &mut sheets[sheet];
        size.0 = size.0.max(x);
        size.1 = size.1.max(y + h);
        if !fits_a_sheet {
            // Alone on its sheet: the next picture starts a new one.
            (x, y, shelf) = (SHEET_MAX, SHEET_MAX, 0);
        }
    }
    (frames, sheets)
}

/// The bytes of a pack holding `pictures`.
pub fn build(pictures: &[Picture]) -> Result<Vec<u8>, String> {
    let sizes: Vec<(usize, usize)> = pictures.iter().map(|p| (p.w, p.h)).collect();
    let (frames, sheet_sizes) = layout(&sizes);
    let mut sheets: Vec<Vec<u8>> = sheet_sizes
        .iter()
        .map(|&(w, h)| vec![0; w * h * 4])
        .collect();
    for (p, f) in pictures.iter().zip(&frames) {
        let (sheet_w, _) = sheet_sizes[usize::from(f.sheet)];
        let target = &mut sheets[usize::from(f.sheet)];
        for row in 0..p.h {
            let from = row * p.w * 4;
            let to = ((usize::from(f.y) + row) * sheet_w + usize::from(f.x)) * 4;
            target[to..to + p.w * 4].copy_from_slice(&p.rgba[from..from + p.w * 4]);
        }
    }
    let pngs = sheets
        .iter()
        .zip(&sheet_sizes)
        .map(|(rgba, &(w, h))| encode_png(w, h, rgba))
        .collect::<Result<Vec<_>, _>>()?;
    let table: Vec<(String, Frame)> = pictures
        .iter()
        .zip(frames)
        .map(|(p, f)| (p.name.clone(), f))
        .collect();
    encode(&table, &pngs)
}

/// Every picture of a pack, decoded.
pub fn pictures_of(pack: &Pack) -> Result<Vec<Picture>, String> {
    let mut sheets = Vec::new();
    for i in 0..pack.sheet_count() {
        let png = pack.sheet_png(i as u16).ok_or("pack: sheet missing")?;
        sheets.push(decode_png(png)?);
    }
    let mut out = Vec::new();
    for name in pack.names() {
        let f = pack.frame(name).ok_or("pack: name without a frame")?;
        let (sw, sh, data) = sheets
            .get(usize::from(f.sheet))
            .ok_or("pack: sheet missing")?;
        let (x, y, w, h) = (
            usize::from(f.x),
            usize::from(f.y),
            usize::from(f.w),
            usize::from(f.h),
        );
        if x + w > *sw || y + h > *sh {
            return Err(format!("pack: {name} reaches outside its sheet"));
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            let at = (row * sw + x) * 4;
            rgba.extend_from_slice(&data[at..at + w * 4]);
        }
        out.push(Picture {
            name: name.to_string(),
            w,
            h,
            rgba,
        });
    }
    Ok(out)
}

/// What a unit's folder is: its race's id, or `hero`, `ship` or `fantastic` for one with no race.
/// Read from the rules' `units.ron` and `races.ron` (hand-kept, one entry per line), so a new unit
/// is filed by the rules and no table here needs a change.
pub fn unit_folders(rules_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let read = |file: &str| {
        let path = rules_dir.join(file);
        fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let field = |line: &str, key: &str| -> Option<String> {
        let at = line.find(key)? + key.len();
        let rest = &line[at..];
        let end = rest.find([',', ')', '"'])?;
        Some(rest[..end].to_string())
    };
    let mut races = BTreeMap::new();
    for line in read("races.ron")?.lines() {
        if let (Some(num), Some(id)) = (field(line, "num: "), field(line, "id: \"")) {
            races.insert(num, id);
        }
    }
    let mut out = BTreeMap::new();
    for line in read("units.ron")?.lines() {
        let (Some(id), Some(kind)) = (field(line, "id: \""), field(line, "kind: ")) else {
            continue;
        };
        let race = field(line, "race: Some(").and_then(|n| races.get(&n).cloned());
        let folder = match (race, kind.as_str()) {
            (Some(race), _) => race,
            (None, "Hero") => "hero".to_string(),
            (None, _) if matches!(id.as_str(), "trireme" | "galley" | "warship") => {
                "ship".to_string()
            }
            (None, _) => "fantastic".to_string(),
        };
        out.insert(id, folder);
    }
    if out.is_empty() {
        return Err("units.ron: no units found".to_string());
    }
    Ok(out)
}

/// What `pack_tree` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub packs: usize,
    pub pictures: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Whether a folder is packed: the pictures under `art/` the client reads one at a time. Not the
/// fonts (read in other ways), the screens (each picture is large already), and not `neutral/`,
/// `palette-swap/` or `shots/` (working files of scripts and the author, never read by the client).
fn skipped(dir: &str) -> bool {
    !dir.starts_with("art/") || dir.starts_with("art/fonts")
}

/// Every folder under `root` with PNG files in it, relative to the root, with `/` separators.
fn folders_with_pngs(root: &Path) -> Result<BTreeMap<String, Vec<PathBuf>>, String> {
    let mut found: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                todo.push(path);
            } else if path.extension().is_some_and(|e| e == "png") {
                let rel = dir
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !skipped(&rel) {
                    found.entry(rel).or_default().push(path);
                }
            }
        }
    }
    Ok(found)
}

/// The pack file a folder goes to, relative to the root.
fn pack_path(dir: &str, units: &BTreeMap<String, String>) -> String {
    if let Some(unit) = dir.strip_prefix("art/units/") {
        if let Some(folder) = units.get(unit) {
            return format!("art/units/{folder}/{unit}.pack");
        }
    }
    format!("{dir}.pack")
}

/// The rows of `packs.tsv`, a folder and its pack.
pub fn read_table(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join("packs.tsv"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(d, p)| (d.to_string(), p.to_string()))
        .collect()
}

/// Pack the loose pictures under `root` (`assets-local`). With `keep` the loose files stay.
pub fn pack_tree(root: &Path, rules_dir: &Path, keep: bool) -> Result<Report, String> {
    let units = unit_folders(rules_dir)?;
    let mut table = read_table(root);
    let mut report = Report::default();
    for (dir, files) in folders_with_pngs(root)? {
        let pack_rel = table
            .get(&dir)
            .cloned()
            .unwrap_or_else(|| pack_path(&dir, &units));
        let pack_file = root.join(&pack_rel);
        let mut pictures: BTreeMap<String, Picture> = BTreeMap::new();
        if let Ok(bytes) = fs::read(&pack_file) {
            for p in pictures_of(&Pack::parse(bytes)?)? {
                pictures.insert(p.name.clone(), p);
            }
        }
        let mut loose = Vec::new();
        for path in &files {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("{}: not a name", path.display()))?
                .to_string();
            let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            report.bytes_before += bytes.len() as u64;
            let (w, h, rgba) =
                decode_png(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            pictures.insert(name.clone(), Picture { name, w, h, rgba });
            loose.push(path);
        }
        let all: Vec<Picture> = pictures.into_values().collect();
        let bytes = build(&all)?;
        // Read it back before any loose file goes.
        let back: BTreeMap<String, Picture> = pictures_of(&Pack::parse(bytes.clone())?)?
            .into_iter()
            .map(|p| (p.name.clone(), p))
            .collect();
        if all.iter().any(|p| back.get(&p.name) != Some(p)) {
            return Err(format!("{pack_rel}: a picture does not read back the same"));
        }
        if let Some(parent) = pack_file.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&pack_file, &bytes).map_err(|e| format!("{}: {e}", pack_file.display()))?;
        report.packs += 1;
        report.pictures += all.len();
        report.bytes_after += bytes.len() as u64;
        table.insert(dir.clone(), pack_rel);
        if !keep {
            for path in loose {
                fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            // A folder left empty goes too (not the root, and not one with other files).
            let _ = fs::remove_dir(root.join(&dir));
        }
    }
    let rows: String = table.iter().map(|(d, p)| format!("{d}\t{p}\n")).collect();
    fs::write(root.join("packs.tsv"), rows).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Turn every pack under `root` back into loose PNG files (the scripts that make or change
/// pictures, `scripts/make_banner_colours.py` and the like, work on loose files; `pack_tree` packs
/// again afterwards and keeps what they wrote). Returns the number of pictures.
pub fn unpack_tree(root: &Path) -> Result<usize, String> {
    let mut count = 0;
    for (dir, pack_rel) in read_table(root) {
        let pack_file = root.join(&pack_rel);
        let bytes = fs::read(&pack_file).map_err(|e| format!("{}: {e}", pack_file.display()))?;
        let target = root.join(&dir);
        fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        for p in pictures_of(&Pack::parse(bytes)?)? {
            let file = target.join(&p.name);
            fs::write(&file, encode_png(p.w, p.h, &p.rgba)?)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            count += 1;
        }
        fs::remove_file(&pack_file).map_err(|e| format!("{}: {e}", pack_file.display()))?;
        if let Some(parent) = pack_file.parent() {
            // A race folder left empty goes too.
            let _ = fs::remove_dir(parent);
        }
    }
    let table = root.join("packs.tsv");
    if table.exists() {
        fs::remove_file(&table).map_err(|e| format!("{}: {e}", table.display()))?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{content_dir, TestResult};

    fn picture(name: &str, w: usize, h: usize, shade: u8) -> Picture {
        Picture {
            name: name.to_string(),
            w,
            h,
            rgba: (0..w * h * 4)
                .map(|i| shade.wrapping_add(i as u8))
                .collect(),
        }
    }

    #[test]
    fn pictures_read_back_unchanged_from_a_pack() -> TestResult {
        let pics = vec![
            picture("a.png", 30, 40, 1),
            picture("b.png", 5, 7, 2),
            picture("c.png", 64, 3, 3),
        ];
        let pack = Pack::parse(build(&pics)?)?;
        let mut back = pictures_of(&pack)?;
        back.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(back, pics);
        Ok(())
    }

    #[test]
    fn many_pictures_go_on_several_sheets_none_over_the_limit() -> TestResult {
        let pics: Vec<Picture> = (0..300)
            .map(|i| picture(&format!("p{i:03}.png"), 200, 150, i as u8))
            .collect();
        let pack = Pack::parse(build(&pics)?)?;
        assert!(pack.sheet_count() > 1);
        for i in 0..pack.sheet_count() {
            let (w, h, _) = decode_png(pack.sheet_png(i as u16).ok_or("no sheet")?)?;
            assert!(w <= SHEET_MAX && h <= SHEET_MAX);
        }
        assert_eq!(pictures_of(&pack)?.len(), 300);
        Ok(())
    }

    #[test]
    fn a_picture_bigger_than_a_sheet_gets_one_of_its_own() -> TestResult {
        let pics = vec![
            picture("small.png", 10, 10, 1),
            picture("huge.png", 2100, 5, 2),
        ];
        let pack = Pack::parse(build(&pics)?)?;
        assert_eq!(pack.sheet_count(), 2);
        assert_eq!(pictures_of(&pack)?.len(), 2);
        Ok(())
    }

    #[test]
    fn a_tree_is_packed_replaced_and_unpacked_with_the_same_pictures() -> TestResult {
        let root = std::env::temp_dir().join(format!("f4x_pack_tree_{}", std::process::id()));
        let unit = root.join("art/units/high_elf_spearmen");
        let spell = root.join("art/spells/fireball");
        fs::create_dir_all(&unit)?;
        fs::create_dir_all(&spell)?;
        fs::create_dir_all(root.join("screens/menu"))?;
        let a = picture("figure_0_00.png", 4, 3, 9);
        fs::write(unit.join(&a.name), encode_png(a.w, a.h, &a.rgba)?)?;
        let b = picture("000_00.png", 2, 2, 50);
        fs::write(spell.join(&b.name), encode_png(b.w, b.h, &b.rgba)?)?;
        fs::write(
            root.join("screens/menu/big_00.png"),
            encode_png(a.w, a.h, &a.rgba)?,
        )?;
        let rules = content_dir().join("../../f4x-def/content");

        let report = pack_tree(&root, &rules, false)?;
        assert_eq!((report.packs, report.pictures), (2, 2));
        assert!(root
            .join("art/units/high_elf/high_elf_spearmen.pack")
            .exists());
        assert!(root.join("art/spells/fireball.pack").exists());
        assert!(!unit.exists() && !spell.exists());
        assert!(
            root.join("screens/menu/big_00.png").exists(),
            "screens stay loose"
        );

        // A picture made later is merged into the pack, not lost.
        fs::create_dir_all(&unit)?;
        let c = picture("figure_0_01.png", 3, 3, 70);
        fs::write(unit.join(&c.name), encode_png(c.w, c.h, &c.rgba)?)?;
        pack_tree(&root, &rules, false)?;
        assert_eq!(unpack_tree(&root)?, 3);
        let (w, h, rgba) = decode_png(&fs::read(unit.join("figure_0_00.png"))?)?;
        assert_eq!((w, h, rgba), (a.w, a.h, a.rgba));
        assert!(!root.join("packs.tsv").exists());
        assert!(!root.join("art/units/high_elf").exists());
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn units_are_filed_by_race_and_the_others_by_kind() -> TestResult {
        let rules = content_dir().join("../../f4x-def/content");
        let folders = unit_folders(&rules)?;
        assert_eq!(
            folders.get("high_elf_spearmen").map(String::as_str),
            Some("high_elf")
        );
        assert_eq!(folders.get("dwarf").map(String::as_str), Some("hero"));
        assert_eq!(folders.get("trireme").map(String::as_str), Some("ship"));
        assert_eq!(folders.get("djinn").map(String::as_str), Some("fantastic"));
        assert_eq!(
            pack_path("art/units/high_elf_spearmen", &folders),
            "art/units/high_elf/high_elf_spearmen.pack"
        );
        assert_eq!(
            pack_path("art/spells/fireball", &folders),
            "art/spells/fireball.pack"
        );
        Ok(())
    }
}
