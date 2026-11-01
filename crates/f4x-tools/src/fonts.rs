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

use crate::accent::{extra_glyphs, BmGlyph};
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
    ramps: &[Ramp],
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
        let mut bm_glyphs: Vec<BmGlyph> = glyphs
            .iter()
            .enumerate()
            .map(|(g, (_, width, px))| BmGlyph {
                ch: char::from(32 + g as u8),
                width: *width,
                height,
                y_offset: 0,
                advance: width + hspacing,
                px: px.clone(),
            })
            .collect();
        // The original's fonts are ASCII only: the other European letters are drawn from them.
        if style < LETTER_STYLES {
            bm_glyphs.extend(extra_glyphs(&bm_glyphs, if height > 12 { 2 } else { 1 }));
        }
        let mut colours = set_colours(palette_entry, pal);
        if style < RAMP_STYLES {
            colours.extend(ramps.iter().map(Ramp::set));
        }
        let (atlas, fnt) = bmfont(style, height, vspacing, &bm_glyphs, &colours);
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

/// Glyphs per row of a colour set in the atlas: wide, so that the stacked sets stay under the
/// 16384 pixel texture limit of the graphics cards.
const BM_COLUMNS: usize = 48;

/// The styles that are letters (0 to 5); 6 and 7 are rune fonts and get no extra letters.
const LETTER_STYLES: usize = 6;

/// The styles (0 to 5) the screens draw with a ramp; the runes get none.
const RAMP_STYLES: usize = 6;

/// A colour set of its own that follows the 16 of the original in the BMFont atlases: what a screen
/// makes with `Set_Font_Colors_15` (`content/font_colour_ramps.ron`, written by
/// `scripts/font_ramps.py`), the colours of glyph colour indexes 1 to 15.
#[derive(Debug, serde::Deserialize)]
pub struct Ramp {
    pub name: String,
    pub colours: Vec<(u8, u8, u8)>,
}

impl Ramp {
    fn set(&self) -> [[u8; 4]; 16] {
        let mut row = [[0u8; 4]; 16];
        for (slot, &(r, g, b)) in row.iter_mut().skip(1).zip(&self.colours) {
            *slot = [r, g, b, 255];
        }
        row
    }
}

/// The ramps in `content/font_colour_ramps.ron`: `(ramps: [(name: "...", colours: [(r, g, b) x 15])])`.
pub fn read_ramps(text: &str) -> Result<Vec<Ramp>, String> {
    #[derive(serde::Deserialize)]
    struct File {
        ramps: Vec<Ramp>,
    }
    let file: File = ron::from_str(text).map_err(|e| format!("font_colour_ramps.ron: {e}"))?;
    match file.ramps.iter().find(|r| r.colours.len() != 15) {
        Some(bad) => Err(format!("ramp {}: needs 15 colours", bad.name)),
        None => Ok(file.ramps),
    }
}

/// The RGBA of each colour index 1 to 15 in each of the 16 colour sets (index 0 is clear).
fn set_colours(palette_entry: &[u8], pal: &[u8]) -> Vec<[[u8; 4]; 16]> {
    (0..SETS)
        .map(|set| {
            let mut row = [[0u8; 4]; 16];
            for (c, slot) in row.iter_mut().enumerate().skip(1) {
                let pi = palette_entry
                    .get(OFS_COLOR_SETS + set * 16 + c)
                    .copied()
                    .unwrap_or(0);
                slot.copy_from_slice(&to_rgba(&[pi], pal));
            }
            row
        })
        .collect()
}

/// A style as an AngelCode text BMFont: the atlas `(width, height, RGBA)` and the `.fnt` text.
/// The glyphs go into a grid with a one pixel gap, so nearest scaling does not bleed, and the
/// grid is stacked once per colour set. The `.fnt` describes the first grid. `xadvance` is the
/// glyph's advance (its width plus the style's horizontal spacing), `lineHeight` the style's
/// height plus the vertical spacing.
fn bmfont(
    style: usize,
    height: usize,
    vspacing: usize,
    glyphs: &[BmGlyph],
    colours: &[[[u8; 4]; 16]],
) -> ((usize, usize, Vec<u8>), String) {
    let cell_w = glyphs.iter().map(|g| g.width).max().unwrap_or(0) + 1;
    let cell_h = glyphs.iter().map(|g| g.height).max().unwrap_or(0) + 1;
    let rows = glyphs.len().div_ceil(BM_COLUMNS);
    let (aw, grid_h) = (cell_w * BM_COLUMNS, cell_h * rows);
    let mut rgba = vec![0u8; aw * grid_h * colours.len() * 4];
    let mut fnt = format!(
        "info face=\"FONTS.LBX style {style}\" size={height}\n\
         common lineHeight={} base={height} scaleW={aw} scaleH={grid_h} pages=1 packed=0\n\
         page id=0 file=\"style{style}.png\"\nchars count={}\n",
        height + vspacing,
        glyphs.len()
    );
    for (n, glyph) in glyphs.iter().enumerate() {
        let (cx, cy) = (n % BM_COLUMNS * cell_w, n / BM_COLUMNS * cell_h);
        for (set, palette) in colours.iter().enumerate() {
            for row in 0..glyph.height {
                for col in 0..glyph.width {
                    let index = glyph.px[row * glyph.width + col];
                    let at = ((set * grid_h + cy + row) * aw + cx + col) * 4;
                    rgba[at..at + 4].copy_from_slice(&palette[usize::from(index & 0x0F)]);
                }
            }
        }
        fnt.push_str(&format!(
            "char id={} x={cx} y={cy} width={} height={} xoffset=0 yoffset={} xadvance={} page=0 chnl=15\n",
            glyph.ch as u32, glyph.width, glyph.height, glyph.y_offset, glyph.advance
        ));
    }
    ((aw, grid_h * colours.len(), rgba), fnt)
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

    fn glyph(ch: char, width: usize, height: usize, rows: &[&str]) -> BmGlyph {
        // Each row of `rows` is a string of colour indexes, `.` for clear.
        let px = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c.to_digit(10).map_or(0, |d| d as u8)))
            .collect();
        BmGlyph {
            ch,
            width,
            height,
            y_offset: 0,
            advance: width + 1,
            px,
        }
    }

    #[test]
    fn a_style_becomes_a_bmfont_with_every_colour_set_stacked() {
        let a = glyph('A', 2, 2, &["1.", ".1"]);
        let v = glyph('V', 3, 2, &["111", "..1"]);
        let mut colours = vec![[[0u8; 4]; 16]; 2];
        colours[0][1] = [200, 200, 200, 255];
        colours[1][1] = [255, 0, 0, 255];
        let ((aw, ah, rgba), fnt) = bmfont(1, 2, 3, &[a, v], &colours);
        assert_eq!((aw, ah), (4 * BM_COLUMNS, 3 * 2));
        assert!(fnt.contains(&format!(
            "common lineHeight=5 base=2 scaleW={} scaleH=3",
            4 * BM_COLUMNS
        )));
        assert!(fnt.contains("char id=65 x=0 y=0 width=2 height=2 xoffset=0 yoffset=0 xadvance=3"));
        assert!(fnt.contains("char id=86 x=4 y=0 width=3 height=2 xoffset=0 yoffset=0 xadvance=4"));
        let pixel = |x: usize, y: usize| rgba[(y * aw + x) * 4..(y * aw + x) * 4 + 4].to_vec();
        // 'V' draws its top row in set 0 and the same pixels in set 1 (3 rows down), in red.
        assert_eq!(pixel(4, 0), [200, 200, 200, 255]);
        assert_eq!(pixel(4, 1), [0, 0, 0, 0]);
        assert_eq!(pixel(4, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn a_ramp_becomes_a_colour_set_after_the_original_sixteen() {
        let text = format!(
            "(ramps: [(name: \"gold\", colours: [{}])])",
            vec!["(255, 182, 44)"; 15].join(", ")
        );
        let ramps = read_ramps(&text).expect("parses");
        assert_eq!(ramps[0].name, "gold");
        let set = ramps[0].set();
        assert_eq!(set[0], [0, 0, 0, 0]);
        assert_eq!(set[1], [255, 182, 44, 255]);
        assert!(read_ramps("(ramps: [(name: \"short\", colours: [(1, 2, 3)])])").is_err());
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
