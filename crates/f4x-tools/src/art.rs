//! `extract-art`: decode every picture in the original game's `.lbx` files to PNG.
//!
//! The output is for local development only (it goes in the gitignored
//! `assets-local/`). A `manifest.tsv` lists `lbx  entry  frames  width  height`
//! so the client can find what it needs.

use std::fs;
use std::path::Path;

use crate::image::{decode, to_rgba};
use crate::lbx::Lbx;

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

fn extract_terrain(game_dir: &Path, out_dir: &Path, pal: &[u8]) -> Result<usize, String> {
    let Some(path) = find(game_dir, "TERRAIN.LBX") else {
        return Ok(0);
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lbx = Lbx::parse(&data).map_err(|e| format!("TERRAIN.LBX: {e}"))?;
    if lbx.len() == 0 {
        return Ok(0);
    }
    let dir = out_dir.join("terrain");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tiles = terrain_tiles(lbx.entry(0));
    if lbx.len() > 1 {
        let map = terrain_type_map(lbx.entry(1));
        let rows: String = map
            .iter()
            .enumerate()
            .map(|(t, (tile, anim))| format!("{t}\t{tile}\t{}\n", u8::from(*anim)))
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
        write_png(&dir.join(format!("tile_{i:04}.png")), TILE_W, TILE_H, &rgba)?;
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
    Ok(tiles.len())
}

pub fn extract(game_dir: &Path, out_dir: &Path) -> Result<(), String> {
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
        for i in 0..lbx.len() {
            let Some(pic) = decode(lbx.entry(i)) else {
                continue;
            };
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut pal = base.clone();
            if let Some((start, colours)) = &pic.palette {
                pal[start * 3..start * 3 + colours.len()].copy_from_slice(colours);
            }
            for (f, frame) in pic.frames.iter().enumerate() {
                let rgba = to_rgba(&frame.pixels, &pal);
                write_png(
                    &dir.join(format!("{i:03}_{f:02}.png")),
                    pic.width,
                    pic.height,
                    &rgba,
                )?;
                frames += 1;
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
    let tiles = extract_terrain(game_dir, out_dir, &base)?;
    println!("extracted {tiles} terrain tiles");
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
    fn type_map_matches_the_games_formula() {
        // type 0: page 0, index 2 -> first tile; type 1: page 6, index 19 -> (6*16384 + 19*384 - 576 - 192) / 384
        let entry = [0u8, 2, 6, 19, 0x80 | 6, 19];
        let map = terrain_type_map(&entry);
        assert_eq!(map[0], (0, false));
        assert_eq!(map[1], (273, false));
        assert_eq!(map[2], (273, true));
    }
}
