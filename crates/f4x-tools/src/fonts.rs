//! Decode the original's bitmap fonts (FONTS.LBX entry 0) into texture atlases.
//!
//! The file holds eight font styles of 96 glyphs (ASCII 32 to 127). Each glyph is stored by
//! columns: a byte `0x80` ends a column, `0x80 | n` skips `n` rows, and any other byte draws
//! `high nibble` pixels downward in the colour given by its `low nibble`; that colour index
//! maps to a palette index through one of 16 colour sets (entry 2 at offset `0x300`, 16 sets of
//! 16 bytes). An atlas per style stacks the glyph strip once per colour set, so a client can
//! draw in any of the original's text colours; `metrics.tsv` gives the glyph positions.

use std::fs;
use std::path::Path;

use crate::image::to_rgba;

const STYLES: usize = 8;
const GLYPHS: usize = 96;
const SETS: usize = 16;
const OFS_BASE_HEIGHT: usize = 0x16A;
const OFS_HSPACING: usize = 0x17A;
const OFS_VSPACING: usize = 0x18A;
const OFS_WIDTHS: usize = 0x19A;
const OFS_DATA: usize = 0x49A;
const OFS_COLOR_SETS: usize = 0x300;

fn u16_at(d: &[u8], at: usize) -> usize {
    d.get(at..at + 2)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize)
}

/// One glyph as a `width * height` grid of colour indexes (0 = nothing drawn).
pub fn decode_glyph(data: &[u8], start: usize, width: usize, height: usize) -> Vec<u8> {
    let mut px = vec![0u8; width * height];
    let mut at = start;
    for col in 0..width {
        let mut row = 0usize;
        while let Some(&b) = data.get(at) {
            at += 1;
            if b == 0x80 {
                break;
            }
            if b & 0x80 != 0 {
                row += usize::from(b & 0x7F);
            } else {
                let (repeat, color) = (usize::from(b >> 4), b & 0x0F);
                for _ in 0..repeat {
                    if row < height {
                        px[row * width + col] = color;
                    }
                    row += 1;
                }
            }
        }
    }
    px
}

/// Write `fonts/style{N}.png` and `fonts/metrics.tsv` under `out_dir`.
pub fn extract_fonts(
    fonts: &[u8],
    palette_entry: &[u8],
    pal: &[u8],
    out_dir: &Path,
    write_png: &dyn Fn(&Path, usize, usize, &[u8]) -> Result<(), String>,
) -> Result<usize, String> {
    if fonts.len() < OFS_DATA + STYLES * GLYPHS * 2 {
        return Ok(0);
    }
    let dir = out_dir.join("fonts");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut metrics = String::new();
    let mut count = 0;
    for style in 0..STYLES {
        let height = u16_at(fonts, OFS_BASE_HEIGHT + style * 2);
        let hspacing = u16_at(fonts, OFS_HSPACING + style * 2);
        let vspacing = u16_at(fonts, OFS_VSPACING + style * 2);
        if height == 0 {
            continue;
        }
        metrics.push_str(&format!("S\t{style}\t{height}\t{hspacing}\t{vspacing}\n"));
        // Glyphs and where each goes in the strip.
        let mut glyphs: Vec<(usize, usize, Vec<u8>)> = Vec::new();
        let mut x = 0;
        for g in 0..GLYPHS {
            let width = usize::from(fonts[OFS_WIDTHS + style * GLYPHS + g]);
            let start = u16_at(fonts, OFS_DATA + (style * GLYPHS + g) * 2);
            let px = decode_glyph(fonts, start, width, height);
            metrics.push_str(&format!("G\t{style}\t{}\t{x}\t{width}\n", g + 32));
            glyphs.push((x, width, px));
            x += width.max(1);
        }
        let (aw, ah) = (x.max(1), height * SETS);
        let mut rgba = vec![0u8; aw * ah * 4];
        for set in 0..SETS {
            let mut indexed = vec![0u8; aw * height];
            for (gx, gw, px) in &glyphs {
                for row in 0..height {
                    for col in 0..*gw {
                        let c = px[row * gw + col];
                        if c != 0 {
                            let pi = palette_entry
                                .get(OFS_COLOR_SETS + set * 16 + usize::from(c))
                                .copied()
                                .unwrap_or(0);
                            indexed[row * aw + gx + col] = pi;
                        }
                    }
                }
            }
            let strip = to_rgba(&indexed, pal);
            let at = set * height * aw * 4;
            rgba[at..at + strip.len()].copy_from_slice(&strip);
        }
        write_png(&dir.join(format!("style{style}.png")), aw, ah, &rgba)?;
        count += 1;
    }
    fs::write(dir.join("metrics.tsv"), metrics).map_err(|e| e.to_string())?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_columns_decode_skips_runs_and_ends() {
        // Two columns, height 4. Column 0: skip 1 row, then 2 pixels of colour 5, end.
        // Column 1: 3 pixels of colour 2, end.
        let data = [0x81, 0x25, 0x80, 0x32, 0x80];
        let px = decode_glyph(&data, 0, 2, 4);
        assert_eq!(px, vec![0, 2, 5, 2, 5, 2, 0, 0]);
        // A glyph cannot draw below its height.
        let px = decode_glyph(&[0x75, 0x80], 0, 1, 3);
        assert_eq!(px, vec![5, 5, 5]);
    }
}
