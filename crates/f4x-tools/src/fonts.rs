//! Decode the original's bitmap fonts (FONTS.LBX entry 0) into texture atlases.
//!
//! The file holds eight font styles of 96 glyphs (ASCII 32 to 127). Each glyph is stored by
//! columns: a byte `0x80` ends a column, `0x80 | n` skips `n` rows, and any other byte draws
//! `high nibble` pixels downward in the colour given by its `low nibble`; that colour index
//! maps to a palette index through one of 16 colour sets (entry 2 at offset `0x300`, 16 sets of
//! 16 bytes). An atlas per style stacks the glyph strip once per colour set, so a client can
//! draw in any of the original's text colours; `metrics.tsv` gives the glyph positions.
//! `fonts/bmfont/style{N}.fnt` and `.png` repeat each style as an AngelCode text BMFont: the glyphs on a
//! grid, the 16 colour sets stacked (the `.fnt` describes the first; the atlas is 16 times its `scaleH`).

use std::fs;
use std::path::Path;

use crate::image::{to_rgba, vga_to_rgb8};

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

/// Writes a PNG: path, width, height, RGBA bytes.
pub type PngWriter<'a> = &'a dyn Fn(&Path, usize, usize, &[u8]) -> Result<(), String>;

/// Write `fonts/style{N}.png` and `fonts/metrics.tsv` under `out_dir`.
pub fn extract_fonts(
    fonts: &[u8],
    palette_entry: &[u8],
    pal: &[u8],
    out_dir: &Path,
    write_png: PngWriter,
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
        let widths: Vec<usize> = glyphs.iter().map(|(_, w, _)| *w).collect();
        let xs: Vec<usize> = glyphs.iter().map(|(x, _, _)| *x).collect();
        let (atlas, fnt) = bmfont(style, height, (hspacing, vspacing), &xs, &widths, &rgba, aw);
        let bm_dir = dir.join("bmfont");
        fs::create_dir_all(&bm_dir).map_err(|e| e.to_string())?;
        write_png(
            &bm_dir.join(format!("style{style}.png")),
            atlas.0,
            atlas.1,
            &atlas.2,
        )?;
        fs::write(bm_dir.join(format!("style{style}.fnt")), fnt).map_err(|e| e.to_string())?;
        count += 1;
    }
    fs::write(dir.join("metrics.tsv"), metrics).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("colour_sets.ron"),
        colour_sets_ron(palette_entry, pal),
    )
    .map_err(|e| e.to_string())?;
    Ok(count)
}

/// The 16 colour sets as RON: for each set the RGB (0 to 255) of the glyph colour indexes 1 to
/// 15, by way of the colour set table and the base palette. This is the one part of the font
/// data that is only numbers, so `content/font_colour_sets.ron` is a committed copy of it.
pub fn colour_sets_ron(palette_entry: &[u8], pal: &[u8]) -> String {
    let mut out = String::from(
        "// The original's 16 font colour sets (FONTS.LBX entry 2 at 0x300, through its base palette;\n\
         // `f4x-tools extract-art` writes this file, `docs/extract-art.md` section 5). A glyph pixel\n\
         // has a colour index 1 to 15 (0 is clear); set n gives index i the colour sets[n][i - 1] (r, g, b).\n\
         (\n    sets: [\n",
    );
    for set in 0..SETS {
        let colours: Vec<String> = (1..16)
            .map(|c| {
                let pi = palette_entry
                    .get(OFS_COLOR_SETS + set * 16 + c)
                    .map_or(0, |&b| usize::from(b));
                let rgb = |k: usize| vga_to_rgb8(pal.get(pi * 3 + k).copied().unwrap_or(0));
                format!("({}, {}, {})", rgb(0), rgb(1), rgb(2))
            })
            .collect();
        out.push_str(&format!("        [{}],\n", colours.join(", ")));
    }
    out.push_str("    ],\n)\n");
    out
}

const BM_COLUMNS: usize = 16;

/// A style as an AngelCode text BMFont: the atlas `(width, height, RGBA)` and the `.fnt` text.
/// `strips` holds the glyph strip (`strip_width` wide) of each of the `SETS` colour sets one
/// below the other. The glyphs go into a grid with a one pixel gap, so nearest scaling does not
/// bleed, and the grid is stacked once per set. The `.fnt` describes the first grid. `xadvance` is
/// the glyph width plus the style's horizontal spacing, `lineHeight` the height plus the vertical.
fn bmfont(
    style: usize,
    height: usize,
    (hspacing, vspacing): (usize, usize),
    xs: &[usize],
    widths: &[usize],
    strips: &[u8],
    strip_width: usize,
) -> ((usize, usize, Vec<u8>), String) {
    let cell_w = widths.iter().copied().max().unwrap_or(1) + 1;
    let cell_h = height + 1;
    let rows = widths.len().div_ceil(BM_COLUMNS);
    let (aw, grid_h) = (cell_w * BM_COLUMNS, cell_h * rows);
    let mut rgba = vec![0u8; aw * grid_h * SETS * 4];
    let mut fnt = format!(
        "info face=\"FONTS.LBX style {style}\" size={height}\n\
         common lineHeight={} base={height} scaleW={aw} scaleH={grid_h} pages=1 packed=0\n\
         page id=0 file=\"style{style}.png\"\nchars count={}\n",
        height + vspacing,
        widths.len()
    );
    for (g, (&gx, &gw)) in xs.iter().zip(widths).enumerate() {
        let (cx, cy) = (g % BM_COLUMNS * cell_w, g / BM_COLUMNS * cell_h);
        for set in 0..SETS {
            for row in 0..height {
                let from = ((set * height + row) * strip_width + gx) * 4;
                let to = ((set * grid_h + cy + row) * aw + cx) * 4;
                rgba[to..to + gw * 4].copy_from_slice(&strips[from..from + gw * 4]);
            }
        }
        fnt.push_str(&format!(
            "char id={} x={cx} y={cy} width={gw} height={height} xoffset=0 yoffset=0 xadvance={} page=0 chnl=15\n",
            g + 32,
            gw + hspacing
        ));
    }
    ((aw, grid_h * SETS, rgba), fnt)
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

    #[test]
    fn a_style_becomes_a_bmfont_with_every_colour_set_stacked() {
        // Two glyphs (widths 2 and 1, height 2) on strips of width 3; set 0 is grey 200, set 1
        // is red, the other sets are empty.
        let strip = |c: [u8; 4]| {
            let mut s = vec![0u8; 3 * 2 * 4];
            for px in [0, 1, 3, 4, 5] {
                s[px * 4..px * 4 + 4].copy_from_slice(&c);
            }
            s
        };
        let mut strips = strip([200, 200, 200, 255]);
        strips.extend(strip([255, 0, 0, 255]));
        strips.extend(vec![0u8; 3 * 2 * 4 * (SETS - 2)]);
        let ((aw, ah, rgba), fnt) = bmfont(1, 2, (1, 3), &[0, 2], &[2, 1], &strips, 3);
        assert_eq!((aw, ah), (3 * BM_COLUMNS, 3 * SETS));
        assert!(fnt.contains("common lineHeight=5 base=2 scaleW=48 scaleH=3"));
        assert!(fnt.contains("char id=32 x=0 y=0 width=2 height=2 xoffset=0 yoffset=0 xadvance=3"));
        assert!(fnt.contains("char id=33 x=3 y=0 width=1 height=2 xoffset=0 yoffset=0 xadvance=2"));
        let pixel = |x: usize, y: usize| rgba[(y * aw + x) * 4..(y * aw + x) * 4 + 4].to_vec();
        // Glyph 33 has an empty top pixel and a drawn bottom one, in set 0 and in set 1 (3 down).
        assert_eq!(pixel(3, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(3, 1), [200, 200, 200, 255]);
        assert_eq!(pixel(3, 3 + 1), [255, 0, 0, 255]);
    }

    #[test]
    fn the_colour_sets_file_has_sixteen_sets_of_fifteen_colours() {
        // Colour set 1 maps index 3 to palette entry 2, which is (63, 0, 31) in 6-bit values.
        let mut entry = vec![0u8; OFS_COLOR_SETS + SETS * 16];
        entry[OFS_COLOR_SETS + 16 + 3] = 2;
        let mut pal = vec![0u8; 768];
        pal[6..9].copy_from_slice(&[63, 0, 31]);
        let ron = colour_sets_ron(&entry, &pal);
        let rows: Vec<&str> = ron
            .lines()
            .filter(|l| l.trim_start().starts_with('['))
            .collect();
        assert_eq!(rows.len(), 16);
        assert_eq!(rows[1].matches('(').count(), 15);
        assert!(rows[1].contains("(255, 0, 125)"), "{}", rows[1]);
    }
}
