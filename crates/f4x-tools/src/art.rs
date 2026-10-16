//! `extract-art`: decode every picture in the original game's `.lbx` files to PNG.
//!
//! The output is for local development only (it goes in the gitignored
//! `assets-local/`). A `manifest.tsv` lists `lbx  entry  frames  width  height`
//! so the client can find what it needs. A picture that belongs to one screen alone
//! (`screens.tsv`) goes to `assets-local/screens/<screen>/` instead of `art/<lbx>/`, unit art
//! to `art/units/<unit id>/`, and `assets-local/index.tsv` says where each such `lbx  entry` went.

use std::fs;
use std::path::Path;

use crate::image::{decode, to_rgba};
use crate::lbx::Lbx;
use crate::screens::{self, Placer, UnitArt};

/// FONTS.LBX entry 2 starts with the 256-colour base palette (6-bit RGB).
const PALETTE_ENTRY: usize = 2;

fn base_palette(game_dir: &Path) -> Result<Vec<u8>, String> {
    let path = find(game_dir, "FONTS.LBX").ok_or("FONTS.LBX not found (needed for the palette)")?;
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("FONTS.LBX: {e}"))?;
    if lbx.len() <= PALETTE_ENTRY || lbx.entry(PALETTE_ENTRY).len() < 768 {
        return Err("FONTS.LBX has no palette entry".into());
    }
    Ok(lbx.entry(PALETTE_ENTRY)[..768].to_vec())
}

/// `Load_Palette(n)` reads FONTS.LBX entry `n + 2`. The original loads these while it shows:
/// 0 the game, 1 the load / save screen, 2 the start menu, 3 the conquest animation, 4 the
/// Spell of Mastery, 5 the win animation and 6 the lose animation.
const PALETTE_LOAD_SAVE: usize = 3;
const PALETTE_MENU: usize = 4;
const PALETTE_CONQUEST: usize = 5;
const PALETTE_MASTERY: usize = 6;
const PALETTE_WIN: usize = 7;
const PALETTE_LOSE: usize = 8;

fn fonts_palette(game_dir: &Path, entry: usize) -> Result<Vec<u8>, String> {
    let path = find(game_dir, "FONTS.LBX").ok_or("FONTS.LBX not found (needed for the palette)")?;
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("FONTS.LBX: {e}"))?;
    if lbx.len() <= entry || lbx.entry(entry).len() < 768 {
        return Err(format!("FONTS.LBX has no palette entry {entry}"));
    }
    Ok(lbx.entry(entry)[..768].to_vec())
}

/// The FONTS.LBX palette entry the original has loaded while it shows pictures of this file,
/// if it is not the base palette. Pictures that carry their own colours (`Load_Palette_From_
/// Animation`) overlay them on whichever palette this gives.
fn palette_entry_for(stem: &str) -> Option<usize> {
    match stem {
        "load" => Some(PALETTE_LOAD_SAVE),
        "mainscrn" | "vortex" => Some(PALETTE_MENU),
        "conquest" => Some(PALETTE_CONQUEST),
        "splmastr" => Some(PALETTE_MASTERY),
        "win" => Some(PALETTE_WIN),
        "lose" => Some(PALETTE_LOSE),
        _ => None,
    }
}

/// Case-insensitive file lookup, since the game files are upper case on disk.
fn find(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
}

fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .map_err(|e| format!("{}: {e}", path.display()))
}

const TILE_W: usize = 20;
const TILE_H: usize = 18;
const TILE_HEADER: usize = 16;
/// Tiles are 16-byte header + 20x18 column-major pixels + 8 bytes padding.
const TILE_STRIDE: usize = 384;
/// TERRAIN.LBX entry 0 starts with 192 bytes before the first tile.
const TILE_START: usize = 192;

/// `types.tsv` rows from TERRAIN.LBX entry 1: for each terrain type (Arcanus types
/// first, then Myrror), the index of its first tile in entry 0 and whether it
/// animates (the following three tiles are the other frames).
///
/// Entry 1 holds a (page, index) byte pair per type. The tile sits at
/// `page * 16384 + index * 384 - 0x240` bytes into entry 0 (the game addressed the
/// whole file in 16 KiB expanded-memory pages), or `0xC0 + (index - 2) * 384` on page 0.
fn terrain_type_map(entry1: &[u8]) -> Vec<(usize, bool)> {
    entry1
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let (page, index) = ((pair[0] & 0x7F) as usize, pair[1] as usize);
            let offset = if page == 0 {
                TILE_START + index.saturating_sub(2) * TILE_STRIDE
            } else {
                (page * 16384 + index * TILE_STRIDE).saturating_sub(TILE_START + TILE_STRIDE)
            };
            (
                (offset.saturating_sub(TILE_START)) / TILE_STRIDE,
                pair[0] & 0x80 != 0,
            )
        })
        .collect()
}

/// Terrain tiles from TERRAIN.LBX entry 0, as column-major palette indices.
fn terrain_tiles(entry: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = TILE_START;
    while at + TILE_STRIDE <= entry.len() {
        let t = &entry[at..at + TILE_STRIDE];
        let (w, h) = (
            t[0] as usize | (t[1] as usize) << 8,
            t[2] as usize | (t[3] as usize) << 8,
        );
        if (w, h) != (TILE_W, TILE_H) {
            break;
        }
        let cols = &t[TILE_HEADER..TILE_HEADER + TILE_W * TILE_H];
        // column-major -> row-major
        let mut px = vec![0u8; TILE_W * TILE_H];
        for x in 0..TILE_W {
            for y in 0..TILE_H {
                px[y * TILE_W + x] = cols[x * TILE_H + y];
            }
        }
        out.push(px);
        at += TILE_STRIDE;
    }
    out
}

/// Terrain types per plane (`TerType_Count`); Myrror's types follow Arcanus's.
const TYPES_PER_PLANE: usize = 762;

/// What kind of ground a terrain type number is, from the ranges of `TerrType.h` (ReMoM): the
/// folder its tiles go to under `map/tiles/<plane>/`. Edge and shore pieces go with their kind.
fn terrain_kind(ty: usize) -> &'static str {
    match ty % TYPES_PER_PLANE {
        0 | 0x259 => "ocean",
        0x12 | 0xC5..=0xC8 => "lakes",
        0x2..=0xA1 | 0xC9..=0xE8 | 0x1C4..=0x1D3 | 0x1D9..=0x258 => "shore",
        0x1 | 0xA2 | 0xAC | 0xAD | 0xB4 => "grassland",
        0xA3 | 0xB7 | 0xB8 => "forest",
        0xA4 | 0x103..=0x112 => "mountains",
        0xA5 | 0xAE..=0xB0 | 0x124..=0x1C3 => "desert",
        0xA6 | 0xB1 | 0xB2 => "swamp",
        0xA7 | 0xB5 | 0xB6 | 0x25A..=0x2F9 => "tundra",
        0xA8..=0xAA => "nodes",
        0xAB | 0x113..=0x123 => "hills",
        0xB3 => "volcano",
        0xB9..=0xC4 | 0xE9..=0x102 | 0x1D4..=0x1D8 => "rivers",
        _ => "other",
    }
}

/// The folder (`arcanus/shore`) of every tile a terrain type uses (an animated type's four
/// tiles); a tile two types share stays with the first.
fn tile_folders(map: &[(usize, bool)]) -> std::collections::HashMap<usize, String> {
    let mut folders = std::collections::HashMap::new();
    for (ty, &(tile, animated)) in map.iter().enumerate() {
        let plane = if ty >= TYPES_PER_PLANE {
            "myrror"
        } else {
            "arcanus"
        };
        let folder = format!("{plane}/{}", terrain_kind(ty));
        for frame in 0..if animated { 4 } else { 1 } {
            folders
                .entry(tile + frame)
                .or_insert_with(|| folder.clone());
        }
    }
    folders
}

fn extract_terrain(game_dir: &Path, out_dir: &Path, pal: &[u8]) -> Result<usize, String> {
    let Some(path) = find(game_dir, "TERRAIN.LBX") else {
        return Ok(0);
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("TERRAIN.LBX: {e}"))?;
    if lbx.len() == 0 {
        return Ok(0);
    }
    let dir = out_dir.join("map/tiles");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tiles = terrain_tiles(lbx.entry(0));
    let map = if lbx.len() > 1 {
        terrain_type_map(lbx.entry(1))
    } else {
        Vec::new()
    };
    let folders = tile_folders(&map);
    if !map.is_empty() {
        // `type  first tile  animated  folder under map/tiles/`
        let rows: String = map
            .iter()
            .enumerate()
            .map(|(t, (tile, anim))| {
                let folder = folders.get(tile).map_or("other", String::as_str);
                format!("{t}\t{tile}\t{}\t{folder}\n", u8::from(*anim))
            })
            .collect();
        fs::write(dir.join("types.tsv"), rows).map_err(|e| e.to_string())?;
    }
    for (i, px) in tiles.iter().enumerate() {
        // Terrain tiles are opaque: colour 0 is a real colour here.
        let mut rgba = to_rgba(px, pal);
        for (k, &v) in px.iter().enumerate() {
            if v == 0 {
                rgba[k * 4 + 3] = 255;
            }
        }
        let folder = dir.join(folders.get(&i).map_or("other", String::as_str));
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        write_png(
            &folder.join(format!("tile_{i:04}.png")),
            TILE_W,
            TILE_H,
            &rgba,
        )?;
    }
    // One sheet of every tile (40 per row) for quick browsing.
    let cols = 40;
    let rows = tiles.len().div_ceil(cols);
    let (sw, sh) = (cols * TILE_W, rows * TILE_H);
    let mut sheet = vec![0u8; sw * sh * 4];
    for (i, px) in tiles.iter().enumerate() {
        let (ox, oy) = ((i % cols) * TILE_W, (i / cols) * TILE_H);
        let rgba = to_rgba(px, pal);
        for y in 0..TILE_H {
            let dst = ((oy + y) * sw + ox) * 4;
            sheet[dst..dst + TILE_W * 4]
                .copy_from_slice(&rgba[y * TILE_W * 4..(y + 1) * TILE_W * 4]);
        }
    }
    write_png(&dir.join("sheet.png"), sw, sh, &sheet)?;
    extract_autotile(game_dir, &dir)?;
    Ok(tiles.len())
}

/// `autotile.tsv` rows `table, mask, terrain type` from TERRTYPE.LBX: five tables of 256
/// entries, indexed by a bit mask of the eight neighbours (NW 1, N 2, NE 4, E 8, SE 16, S 32,
/// SW 64, W 128). Table 0 turns the land neighbours of an ocean tile into its shore type.
fn extract_autotile(game_dir: &Path, dir: &Path) -> Result<(), String> {
    let Some(path) = find(game_dir, "TERRTYPE.LBX") else {
        return Ok(());
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("TERRTYPE.LBX: {e}"))?;
    if lbx.len() == 0 {
        return Ok(());
    }
    // A 4-byte header (record count, record size) precedes the tables.
    let body = lbx.entry(0).get(4..).unwrap_or(&[]);
    let rows: String = body
        .as_chunks::<2>()
        .0
        .iter()
        .take(5 * 256)
        .enumerate()
        .map(|(i, b)| format!("{}\t{}\t{}\n", i / 256, i % 256, u16::from_le_bytes(*b)))
        .collect();
    fs::write(dir.join("autotile.tsv"), rows).map_err(|e| e.to_string())
}

fn extract_map_sprites(game_dir: &Path, out_dir: &Path, pal: &[u8]) -> Result<(), String> {
    let Some(path) = find(game_dir, "MAPBACK.LBX") else {
        return Ok(());
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let n = crate::map_art::extract_map_art(&data, pal, out_dir, &|p, w, h, rgba| {
        write_png(p, w, h, rgba)
    })?;
    println!("catalogued {n} map sprites");
    Ok(())
}

fn extract_font_atlases(game_dir: &Path, out_dir: &Path, pal: &[u8]) -> Result<(), String> {
    let Some(path) = find(game_dir, "FONTS.LBX") else {
        return Ok(());
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("FONTS.LBX: {e}"))?;
    if lbx.len() <= PALETTE_ENTRY {
        return Ok(());
    }
    let n = crate::fonts::extract_fonts(
        lbx.entry(0),
        lbx.entry(PALETTE_ENTRY),
        pal,
        out_dir,
        &|p, w, h, rgba| write_png(p, w, h, rgba),
    )?;
    println!("extracted {n} font styles");
    Ok(())
}

/// First palette index the original recolours to a banner's shades on combat figures
/// (`Combat_Figure_Banner_Color`: indexes 214 to 218).
const FIGURE_BANNER_FIRST: u8 = 214;

/// For a figure frame, a picture the size of the frame that marks the banner-coloured pixels:
/// red is the shade (1 to 5), alpha 255; every other pixel is transparent. The client paints
/// those pixels in the owner's banner shades (`map/banners.tsv`).
fn banner_mask(pixels: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; pixels.len() * 4];
    for (k, &p) in pixels.iter().enumerate() {
        if (FIGURE_BANNER_FIRST..FIGURE_BANNER_FIRST + 5).contains(&p) {
            out[k * 4] = p - FIGURE_BANNER_FIRST + 1;
            out[k * 4 + 3] = 255;
        }
    }
    out
}

pub fn extract(game_dir: &Path, out_dir: &Path, units: &[UnitArt]) -> Result<(), String> {
    let base = base_palette(game_dir)?;
    let mut lbx_files: Vec<_> = fs::read_dir(game_dir)
        .map_err(|e| format!("{}: {e}", game_dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("lbx"))
        })
        .collect();
    lbx_files.sort();
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let placer = Placer::new(screens::rules()?, units);
    let root = out_dir.with_file_name("");
    // An earlier version kept the index in `screens/`.
    let _ = fs::remove_file(root.join("screens/index.tsv"));
    let mut index = String::new();
    let mut manifest = String::new();
    let (mut pictures, mut frames) = (0usize, 0usize);
    for path in &lbx_files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_lowercase();
        let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let Ok(lbx) = Lbx::parse(&data) else { continue };
        let dir = out_dir.join(&stem);
        let file_palette = match palette_entry_for(&stem) {
            Some(entry) => fonts_palette(game_dir, entry)?,
            None => base.clone(),
        };
        for i in 0..lbx.len() {
            let Some(pic) = decode(lbx.entry(i)) else {
                continue;
            };
            // A picture with a place of its own goes there; a copy left in `art/` by an earlier
            // run is removed.
            let placed = placer.place(&stem, i);
            let (target, name) = match &placed {
                Some(place) => {
                    let (folder, name) = place.rsplit_once('/').unwrap_or(("", place));
                    index.push_str(&format!("{stem}\t{i}\t{place}\n"));
                    (root.join(folder), name.to_string())
                }
                None => (dir.clone(), format!("{i:03}")),
            };
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            let mut pal = file_palette.clone();
            if let Some((start, colours)) = &pic.palette {
                pal[start * 3..start * 3 + colours.len()].copy_from_slice(colours);
            }
            for (f, frame) in pic.frames.iter().enumerate() {
                let rgba = to_rgba(&frame.pixels, &pal);
                write_png(
                    &target.join(format!("{name}_{f:02}.png")),
                    pic.width,
                    pic.height,
                    &rgba,
                )?;
                if placed.is_some() {
                    let _ = fs::remove_file(dir.join(format!("{i:03}_{f:02}.png")));
                }
                frames += 1;
                if stem.starts_with("figure") {
                    let mask = banner_mask(&frame.pixels);
                    if placed.is_some() {
                        let _ = fs::remove_file(dir.join(format!("{i:03}_{f:02}_banner.png")));
                    }
                    write_png(
                        &target.join(format!("{name}_{f:02}_banner.png")),
                        pic.width,
                        pic.height,
                        &mask,
                    )?;
                }
            }
            manifest.push_str(&format!(
                "{stem}\t{i}\t{}\t{}\t{}\n",
                pic.frames.len(),
                pic.width,
                pic.height
            ));
            pictures += 1;
        }
    }
    for path in &lbx_files {
        // Drop `art/<lbx>/` when every picture in it has moved out.
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let _ = fs::remove_dir(out_dir.join(stem.to_lowercase()));
        }
    }
    fs::write(root.join("index.tsv"), index).map_err(|e| e.to_string())?;
    let tiles = extract_terrain(game_dir, out_dir, &base)?;
    println!("extracted {tiles} terrain tiles");
    extract_font_atlases(game_dir, out_dir, &base)?;
    extract_map_sprites(game_dir, out_dir, &base)?;
    fs::write(out_dir.join("manifest.tsv"), manifest).map_err(|e| e.to_string())?;
    println!(
        "extracted {pictures} pictures ({frames} frames) from {} lbx files",
        lbx_files.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_that_load_their_own_palette_are_listed() {
        assert_eq!(palette_entry_for("load"), Some(3));
        assert_eq!(palette_entry_for("mainscrn"), Some(4));
        assert_eq!(palette_entry_for("vortex"), Some(4));
        assert_eq!(palette_entry_for("lose"), Some(8));
        assert_eq!(palette_entry_for("cityscap"), None);
    }

    #[test]
    fn terrain_types_are_sorted_into_kinds_on_both_planes() {
        assert_eq!(terrain_kind(0), "ocean");
        assert_eq!(terrain_kind(0x12), "lakes");
        assert_eq!(terrain_kind(0x40), "shore");
        assert_eq!(terrain_kind(0xA4), "mountains");
        assert_eq!(terrain_kind(0x124), "desert");
        assert_eq!(terrain_kind(0x2F9), "tundra");
        // Myrror's types are Arcanus's plus 762.
        assert_eq!(terrain_kind(762 + 0xA3), "forest");
        let folders = tile_folders(&[(5, true), (9, false), (5, false)]);
        assert_eq!(folders[&5], "arcanus/ocean");
        assert_eq!(folders[&8], "arcanus/ocean");
        assert_eq!(folders[&9], "arcanus/grassland");
    }

    #[test]
    fn the_banner_mask_marks_only_the_five_banner_shades() {
        let mask = banner_mask(&[0, 213, 214, 216, 218, 219]);
        let alpha: Vec<u8> = mask.chunks(4).map(|c| c[3]).collect();
        let shade: Vec<u8> = mask.chunks(4).map(|c| c[0]).collect();
        assert_eq!(alpha, vec![0, 0, 255, 255, 255, 0]);
        assert_eq!(shade, vec![0, 0, 1, 3, 5, 0]);
    }

    #[test]
    fn type_map_matches_the_games_formula() {
        // type 0: page 0, index 2 -> first tile; type 1: page 6, index 19 -> (6*16384 + 19*384 - 576 - 192) / 384
        let entry = [0u8, 2, 6, 19, 0x80 | 6, 19];
        let map = terrain_type_map(&entry);
        assert_eq!(map[0], (0, false));
        assert_eq!(map[1], (273, false));
        assert_eq!(map[2], (273, true));
    }
}
