//! Writing sprite packs (`pack.rs` has the sheet layout): put the pictures of a folder on PNG
//! sheets with a uniform grid, check that every picture reads back the same, and replace the loose
//! files by the sheets.
//!
//! `pack_tree` is the command `f4x-extract pack`. Where a folder's sheets go:
//! - `art/units/<unit>/` goes to `art/units/<folder>/<unit>.sheet.png`, where the folder is the
//!   unit's race (`high_elf/`), or `hero/`, `fantastic/` or `ship/` for a unit without one;
//! - `art/cursors/` keeps its sheet inside, `art/cursors/cursors.sheet.png`;
//! - any other folder `D/` goes to `D.sheet.png`, beside it.
//!
//! A folder whose pictures differ much in size gets more sheets (`.sheet2.png`...), so no grid is
//! mostly empty. `packs.tsv` in the root lists `folder<TAB>sheet` for every sheet, both relative to
//! the root; the client reads it once and opens a folder's sheets when a picture is first asked for.
//! A folder packed again merges: a loose file replaces the picture of the same name on its sheets.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pack::{sheet_text, Pack, Sheet};

/// A sheet is at most this many pixels, so one decoded sheet stays a few megabytes.
const SHEET_PIXELS: usize = 4_000_000;
/// The widest a sheet is (the client reads sheets with 16-bit sizes).
const SHEET_WIDTH: usize = 4096;
/// A grid may waste this many times the area the pictures need before the sheet is split.
const WASTE: usize = 2;

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

/// PNG bytes of an RGBA image (the same settings as the loose files), with a text chunk.
pub fn encode_png(
    w: usize,
    h: usize,
    rgba: &[u8],
    text: Option<(&str, String)>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    enc.set_filter(png::FilterType::Paeth);
    if let Some((keyword, text)) = text {
        enc.add_text_chunk(keyword.to_string(), text)
            .map_err(|e| e.to_string())?;
    }
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Cut the pictures into groups that share a grid without wasting much: largest first, a new group
/// when the grid of the group would be `WASTE` times the area its pictures need, or too big.
fn groups(pictures: &[Picture]) -> Vec<Vec<&Picture>> {
    let mut order: Vec<&Picture> = pictures.iter().collect();
    order.sort_by_key(|p| {
        (
            std::cmp::Reverse(p.w * p.h),
            std::cmp::Reverse(p.w),
            &p.name,
        )
    });
    let mut out: Vec<Vec<&Picture>> = Vec::new();
    let (mut cw, mut ch, mut used) = (0, 0, 0);
    for p in order {
        let (nw, nh) = (cw.max(p.w), ch.max(p.h));
        let count = out.last().map_or(0, Vec::len) + 1;
        let grid = count * nw * nh;
        let fits =
            !out.is_empty() && grid <= WASTE * (used + p.w * p.h).max(1) && grid <= SHEET_PIXELS;
        if !fits {
            out.push(Vec::new());
            (cw, ch, used) = (0, 0, 0);
        }
        cw = cw.max(p.w);
        ch = ch.max(p.h);
        used += p.w * p.h;
        if let Some(group) = out.last_mut() {
            group.push(p);
        }
    }
    out
}

/// The PNG bytes of the sheets holding `pictures`, one uniform grid each.
pub fn build(pictures: &[Picture]) -> Result<Vec<Vec<u8>>, String> {
    let mut sheets = Vec::new();
    for mut group in groups(pictures) {
        group.sort_by(|a, b| a.name.cmp(&b.name));
        let cw = group.iter().map(|p| p.w).max().unwrap_or(1).max(1);
        let ch = group.iter().map(|p| p.h).max().unwrap_or(1).max(1);
        // About square, never wider than the limit.
        let wanted = ((group.len() * ch) as f64 / cw as f64).sqrt().ceil() as usize;
        let cols = wanted.clamp(1, (SHEET_WIDTH / cw).max(1)).min(group.len());
        let rows = group.len().div_ceil(cols);
        let (sheet_w, sheet_h) = (cols * cw, rows * ch);
        let mut rgba = vec![0; sheet_w * sheet_h * 4];
        for (i, p) in group.iter().enumerate() {
            let (x, y) = (i % cols * cw, i / cols * ch);
            for row in 0..p.h {
                let from = row * p.w * 4;
                let to = ((y + row) * sheet_w + x) * 4;
                rgba[to..to + p.w * 4].copy_from_slice(&p.rgba[from..from + p.w * 4]);
            }
        }
        let list: Vec<(String, usize, usize)> =
            group.iter().map(|p| (p.name.clone(), p.w, p.h)).collect();
        let text = sheet_text(cw, ch, &list);
        sheets.push(encode_png(
            sheet_w,
            sheet_h,
            &rgba,
            Some((crate::pack::KEYWORD, text)),
        )?);
    }
    Ok(sheets)
}

/// Every picture of a folder's sheets, decoded.
pub fn pictures_of(sheets: &[Vec<u8>]) -> Result<Vec<Picture>, String> {
    let mut out = Vec::new();
    for png in sheets {
        let sheet = Sheet::parse(png)?;
        let (width, height, data) = decode_png(png)?;
        for i in 0..sheet.pictures.len() {
            let (x, y, w, h) = sheet.rect(i).ok_or("sheet: a picture is missing")?;
            if x + w > width || y + h > height {
                return Err("sheet: a picture reaches outside the image".to_string());
            }
            let mut rgba = Vec::with_capacity(w * h * 4);
            for row in y..y + h {
                let at = (row * width + x) * 4;
                rgba.extend_from_slice(&data[at..at + w * 4]);
            }
            out.push(Picture {
                name: sheet.pictures[i].0.clone(),
                w,
                h,
                rgba,
            });
        }
    }
    Ok(out)
}

/// The same as [`Pack`], for the bytes of sheets.
pub fn pack_of(sheets: &[Vec<u8>]) -> Result<Pack, String> {
    Ok(Pack::new(
        sheets
            .iter()
            .map(|b| Sheet::parse(b))
            .collect::<Result<_, _>>()?,
    ))
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

/// The realm folder of each spell, keyed `spell:<id>` (so it shares a map with the units): the
/// spell's realm in `spells.ron` by the id in `realms.ron`. `art/spells/_shared` and `_book` stay
/// where they are.
pub fn spell_folders(rules_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let read = |file: &str| {
        let path = rules_dir.join(file);
        fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let field = |line: &str, key: &str| -> Option<String> {
        let at = line.find(key)? + key.len();
        let rest = &line[at..];
        Some(rest[..rest.find([',', ')', '"'])?].to_string())
    };
    let mut realms = BTreeMap::new();
    for line in read("realms.ron")?.lines() {
        if let (Some(num), Some(id)) = (field(line, "num: "), field(line, "id: \"")) {
            realms.insert(num, id);
        }
    }
    let mut out = BTreeMap::new();
    for line in read("spells.ron")?.lines() {
        if let (Some(id), Some(realm)) = (field(line, "id: \""), field(line, "realm: ")) {
            if let Some(name) = realms.get(&realm) {
                out.insert(format!("spell:{id}"), name.clone());
            }
        }
    }
    if out.is_empty() {
        return Err("spells.ron: no spells found".to_string());
    }
    Ok(out)
}

/// What `pack_tree` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub packs: usize,
    pub sheets: usize,
    pub pictures: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Whether a folder is packed: the pictures under `art/` the client reads one at a time, and the
/// city screen's buildings (`screens/city_screen/cityscape`) and its citizens, one pack for each
/// race (`screens/city_screen/citizens/<race>`). Not the fonts (read in other ways), not the other
/// screens (each picture is large already), and not `neutral/`, `palette-swap/` or `shots/`
/// (working files of scripts and the author, never read by the client).
fn skipped(dir: &str) -> bool {
    let packed_screen =
        dir == "screens/city_screen/cityscape" || dir.starts_with("screens/city_screen/citizens/");
    !(dir.starts_with("art/") && !dir.starts_with("art/fonts") || packed_screen)
}

/// A sheet file (`x.sheet.png`, `x.sheet2.png`), not a loose picture.
fn is_sheet(name: &str) -> bool {
    name.rsplit_once(".sheet").is_some_and(|(_, rest)| {
        rest.strip_suffix(".png")
            .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
    })
}

/// Every folder under `root` with loose PNG files in it, relative to the root, with `/` separators.
fn folders_with_pngs(root: &Path) -> Result<BTreeMap<String, Vec<PathBuf>>, String> {
    let mut found: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                todo.push(path);
            } else if name.ends_with(".png") && !is_sheet(name) {
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

/// The first sheet file of a folder, relative to the root; later ones are `.sheet2.png`...
fn sheet_base(dir: &str, units: &BTreeMap<String, String>) -> String {
    if let Some(unit) = dir.strip_prefix("art/units/") {
        if let Some(folder) = units.get(unit) {
            return format!("art/units/{folder}/{unit}");
        }
    }
    if let Some(spell) = dir.strip_prefix("art/spells/") {
        if let Some(realm) = units.get(&format!("spell:{spell}")) {
            return format!("art/spells/{realm}/{spell}");
        }
    }
    if dir == "art/cursors" {
        return "art/cursors/cursors".to_string();
    }
    dir.to_string()
}

fn sheet_file(base: &str, index: usize) -> String {
    if index == 0 {
        format!("{base}.sheet.png")
    } else {
        format!("{base}.sheet{}.png", index + 1)
    }
}

/// The rows of `packs.tsv`: a folder and its sheets, in order.
pub fn read_table(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in fs::read_to_string(root.join("packs.tsv"))
        .unwrap_or_default()
        .lines()
    {
        if let Some((dir, sheet)) = line.split_once('\t') {
            table
                .entry(dir.to_string())
                .or_default()
                .push(sheet.to_string());
        }
    }
    table
}

fn write_table(root: &Path, table: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let rows: String = table
        .iter()
        .flat_map(|(d, sheets)| sheets.iter().map(move |s| format!("{d}\t{s}\n")))
        .collect();
    fs::write(root.join("packs.tsv"), rows).map_err(|e| e.to_string())
}

fn read_sheets(root: &Path, files: &[String]) -> Result<Vec<Vec<u8>>, String> {
    files
        .iter()
        .map(|f| fs::read(root.join(f)).map_err(|e| format!("{f}: {e}")))
        .collect()
}

/// Pack the loose pictures under `root` (`assets-local`). With `keep` the loose files stay.
pub fn pack_tree(root: &Path, rules_dir: &Path, keep: bool) -> Result<Report, String> {
    let mut units = unit_folders(rules_dir)?;
    units.extend(spell_folders(rules_dir)?);
    let mut table = read_table(root);
    let mut report = Report::default();
    for (dir, files) in folders_with_pngs(root)? {
        let old = table.get(&dir).cloned().unwrap_or_default();
        let base = match old.first() {
            Some(first) => first.trim_end_matches(".sheet.png").to_string(),
            None => sheet_base(&dir, &units),
        };
        let mut pictures: BTreeMap<String, Picture> = BTreeMap::new();
        for p in pictures_of(&read_sheets(root, &old)?)? {
            pictures.insert(p.name.clone(), p);
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
        let sheets = build(&all)?;
        // Read them back before any loose file goes.
        let back: BTreeMap<String, Picture> = pictures_of(&sheets)?
            .into_iter()
            .map(|p| (p.name.clone(), p))
            .collect();
        if back.len() != all.len() || all.iter().any(|p| back.get(&p.name) != Some(p)) {
            return Err(format!("{base}: a picture does not read back the same"));
        }
        for file in &old {
            let _ = fs::remove_file(root.join(file));
        }
        let names: Vec<String> = (0..sheets.len()).map(|i| sheet_file(&base, i)).collect();
        for (file, bytes) in names.iter().zip(&sheets) {
            let target = root.join(file);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
            report.bytes_after += bytes.len() as u64;
        }
        report.packs += 1;
        report.sheets += sheets.len();
        report.pictures += all.len();
        table.insert(dir.clone(), names);
        if !keep {
            for path in loose {
                fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            // A folder left empty goes too (not the root, and not one with other files).
            let _ = fs::remove_dir(root.join(&dir));
        }
    }
    write_table(root, &table)?;
    Ok(report)
}

/// Turn every sheet under `root` back into loose PNG files (the scripts that make or change
/// pictures, `scripts/make_banner_colours.py` and the like, work on loose files; `pack_tree` packs
/// again afterwards and keeps what they wrote). Returns the number of pictures.
pub fn unpack_tree(root: &Path) -> Result<usize, String> {
    let mut count = 0;
    for (dir, files) in read_table(root) {
        let target = root.join(&dir);
        fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        for p in pictures_of(&read_sheets(root, &files)?)? {
            let file = target.join(&p.name);
            fs::write(&file, encode_png(p.w, p.h, &p.rgba, None)?)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            count += 1;
        }
        for f in &files {
            let path = root.join(f);
            fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if let Some(parent) = path.parent() {
                // A race folder left empty goes too.
                let _ = fs::remove_dir(parent);
            }
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

    fn sorted(mut v: Vec<Picture>) -> Vec<Picture> {
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    #[test]
    fn pictures_read_back_unchanged_from_a_sheet_that_is_a_plain_png() -> TestResult {
        let pics = vec![
            picture("a.png", 30, 40, 1),
            picture("b.png", 25, 35, 2),
            picture("c.png", 28, 33, 3),
        ];
        let sheets = build(&pics)?;
        assert_eq!(sheets.len(), 1);
        // Any reader of PNG files gets the image: same width as cols * cell, same height as rows * cell.
        let (w, h, _) = decode_png(&sheets[0])?;
        let sheet = Sheet::parse(&sheets[0])?;
        assert_eq!((sheet.cell_w, sheet.cell_h), (30, 40));
        assert_eq!(w, sheet.cols * 30);
        assert_eq!(h, 3usize.div_ceil(sheet.cols) * 40);
        assert_eq!(sorted(pictures_of(&sheets)?), pics);
        Ok(())
    }

    #[test]
    fn pictures_of_very_different_sizes_do_not_share_one_grid() -> TestResult {
        let mut pics: Vec<Picture> = (0..40)
            .map(|i| picture(&format!("s{i:02}.png"), 8, 8, i as u8))
            .collect();
        pics.push(picture("big.png", 300, 200, 9));
        let sheets = build(&pics)?;
        assert_eq!(sheets.len(), 2, "the big one has a grid of its own");
        let area: usize = sheets
            .iter()
            .map(|png| decode_png(png).map(|(w, h, _)| w * h))
            .sum::<Result<usize, _>>()?;
        let needed: usize = pics.iter().map(|p| p.w * p.h).sum();
        assert!(area <= WASTE * needed + 400, "{area} pixels for {needed}");
        assert_eq!(sorted(pictures_of(&sheets)?), sorted(pics));
        Ok(())
    }

    #[test]
    fn many_pictures_stay_in_sheets_of_bounded_size() -> TestResult {
        let pics: Vec<Picture> = (0..300)
            .map(|i| picture(&format!("p{i:03}.png"), 200, 150, i as u8))
            .collect();
        let sheets = build(&pics)?;
        assert!(sheets.len() > 1);
        for png in &sheets {
            let (w, h, _) = decode_png(png)?;
            assert!(w <= SHEET_WIDTH && w * h <= SHEET_PIXELS / 10 * 11);
        }
        assert_eq!(pictures_of(&sheets)?.len(), 300);
        Ok(())
    }

    #[test]
    fn a_sheet_file_name_is_told_from_a_loose_picture() {
        assert!(is_sheet("high_elf_spearmen.sheet.png"));
        assert!(is_sheet("x.sheet2.png"));
        assert!(!is_sheet("figure_0_00.png"));
        assert!(!is_sheet("sheet.png"));
    }

    #[test]
    fn a_tree_is_packed_replaced_and_unpacked_with_the_same_pictures() -> TestResult {
        let root = std::env::temp_dir().join(format!("f4x_pack_tree_{}", std::process::id()));
        let unit = root.join("art/units/high_elf_spearmen");
        let spell = root.join("art/combat/flames");
        fs::create_dir_all(&unit)?;
        fs::create_dir_all(&spell)?;
        fs::create_dir_all(root.join("screens/menu"))?;
        let a = picture("figure_0_00.png", 4, 3, 9);
        fs::write(unit.join(&a.name), encode_png(a.w, a.h, &a.rgba, None)?)?;
        let b = picture("000_00.png", 2, 2, 50);
        fs::write(spell.join(&b.name), encode_png(b.w, b.h, &b.rgba, None)?)?;
        fs::write(
            root.join("screens/menu/big_00.png"),
            encode_png(a.w, a.h, &a.rgba, None)?,
        )?;
        let rules = content_dir().join("../../f4x-def/content");

        let report = pack_tree(&root, &rules, false)?;
        assert_eq!((report.packs, report.pictures), (2, 2));
        assert!(root
            .join("art/units/high_elf/high_elf_spearmen.sheet.png")
            .exists());
        assert!(root.join("art/combat/flames.sheet.png").exists());
        assert!(!unit.exists() && !spell.exists());
        assert!(
            root.join("screens/menu/big_00.png").exists(),
            "screens stay loose"
        );

        // A picture made later is merged into the sheets, not lost, and the sheets are not taken
        // for loose pictures.
        fs::create_dir_all(&unit)?;
        let c = picture("figure_0_01.png", 3, 3, 70);
        fs::write(unit.join(&c.name), encode_png(c.w, c.h, &c.rgba, None)?)?;
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
            sheet_base("art/units/high_elf_spearmen", &folders),
            "art/units/high_elf/high_elf_spearmen"
        );
        assert_eq!(
            sheet_base("art/spells/fireball", &folders),
            "art/spells/fireball"
        );
        assert_eq!(sheet_base("art/cursors", &folders), "art/cursors/cursors");
        assert_eq!(
            sheet_file("art/spells/fireball", 1),
            "art/spells/fireball.sheet2.png"
        );
        Ok(())
    }
}
