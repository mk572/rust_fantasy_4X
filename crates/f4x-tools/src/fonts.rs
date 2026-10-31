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
        // The original's fonts are ASCII only: German letters are drawn from their base letters.
        if style < GERMAN_STYLES {
            bm_glyphs.extend(german_glyphs(&bm_glyphs, if height > 12 { 2 } else { 1 }));
        }
        let colours = set_colours(palette_entry, pal);
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

const BM_COLUMNS: usize = 16;

/// The styles that are letters (0 to 5); 6 and 7 are rune fonts and get no German letters.
const GERMAN_STYLES: usize = 6;

/// A glyph of the BMFont export: colour indexes (0 is clear), `width` by `height`, drawn
/// `y_offset` rows below the top of the line (negative: above it).
#[derive(Debug, Clone, PartialEq, Eq)]
struct BmGlyph {
    ch: char,
    width: usize,
    height: usize,
    y_offset: i32,
    advance: usize,
    px: Vec<u8>,
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

/// A glyph with a diaeresis (two dots) over a copy of `base`, `dot` pixels square, one clear row
/// between them and the letter. If the letter leaves no room above it, the glyph grows upward
/// and is drawn above the top of the line.
fn with_diaeresis(base: &BmGlyph, ch: char, dot: usize) -> BmGlyph {
    let Some(top) = (0..base.height).find(|&r| {
        base.px[r * base.width..(r + 1) * base.width]
            .iter()
            .any(|&c| c != 0)
    }) else {
        return BmGlyph { ch, ..base.clone() };
    };
    let room = dot + 1;
    let extra = room.saturating_sub(top);
    let mut px = vec![0u8; base.width * (base.height + extra)];
    px[extra * base.width..].copy_from_slice(&base.px);
    // The colour most of the letter has.
    let mut counts = [0usize; 16];
    for &c in &base.px {
        counts[usize::from(c & 0x0F)] += 1;
    }
    let colour = (1..16u8)
        .max_by_key(|&c| counts[usize::from(c)])
        .unwrap_or(1);
    let left = base.width.saturating_sub(dot) / 4;
    let right = base.width.saturating_sub(dot + left);
    let first_row = top + extra - room;
    for row in first_row..first_row + dot {
        for x in (left..left + dot).chain(right..right + dot) {
            if x < base.width {
                px[row * base.width + x] = colour;
            }
        }
    }
    BmGlyph {
        ch,
        height: base.height + extra,
        y_offset: base.y_offset - extra as i32,
        px,
        ..base.clone()
    }
}

/// An eszett: the stem and lower bowl of a small `b`, with the upper curve of a capital `B`
/// above the height of a small `o`, and the bowl cut loose from the stem along the bottom row.
fn eszett(capital_b: &BmGlyph, small_b: &BmGlyph, small_o: &BmGlyph) -> BmGlyph {
    let width = capital_b.width.max(small_b.width);
    let height = small_b.height;
    let cell = |g: &BmGlyph, row: usize, col: usize| {
        if row < g.height && col < g.width {
            g.px[row * g.width + col]
        } else {
            0
        }
    };
    let row_is_empty = |g: &BmGlyph, row: usize| (0..g.width).all(|c| cell(g, row, c) == 0);
    // Rows above the small o are only the stem in a `b`: the capital supplies the curve there.
    let x_height = (0..small_o.height)
        .find(|&r| !row_is_empty(small_o, r))
        .unwrap_or(0);
    let mut px = vec![0u8; width * height];
    for row in 0..height {
        for col in 0..width {
            let own = cell(small_b, row, col);
            px[row * width + col] = if row < x_height {
                own.max(cell(capital_b, row, col))
            } else {
                own
            };
        }
    }
    // The stem is the first run of the row under the top; the bowl's end stops short of it.
    if let Some(bottom) = (0..height)
        .rev()
        .find(|&r| px[r * width..(r + 1) * width].iter().any(|&c| c != 0))
    {
        let probe = (x_height.saturating_sub(1)).min(bottom);
        let line = &px[probe * width..(probe + 1) * width];
        let start = line.iter().position(|&c| c != 0).unwrap_or(0);
        let stem = line[start..].iter().take_while(|&&c| c != 0).count();
        let cut = start + stem;
        let filled = px[bottom * width..(bottom + 1) * width]
            .iter()
            .filter(|&&c| c != 0)
            .count();
        if cut < width && filled > stem + 1 {
            px[bottom * width + cut] = 0;
        }
    }
    BmGlyph {
        ch: 'ß',
        width,
        height,
        y_offset: small_b.y_offset,
        advance: small_b.advance.max(capital_b.advance),
        px,
    }
}

/// Ä, Ö, Ü, ä, ö, ü and ß from the ASCII glyphs, for German.
fn german_glyphs(ascii: &[BmGlyph], dot: usize) -> Vec<BmGlyph> {
    let find = |c: char| ascii.iter().find(|g| g.ch == c);
    let mut out = Vec::new();
    for (base, umlaut) in [
        ('A', 'Ä'),
        ('O', 'Ö'),
        ('U', 'Ü'),
        ('a', 'ä'),
        ('o', 'ö'),
        ('u', 'ü'),
    ] {
        if let Some(glyph) = find(base) {
            out.push(with_diaeresis(glyph, umlaut, dot));
        }
    }
    if let (Some(capital), Some(small), Some(o)) = (find('B'), find('b'), find('o')) {
        out.push(eszett(capital, small, o));
    }
    out
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
        assert!(fnt.contains("common lineHeight=5 base=2 scaleW=64 scaleH=3"));
        assert!(fnt.contains("char id=65 x=0 y=0 width=2 height=2 xoffset=0 yoffset=0 xadvance=3"));
        assert!(fnt.contains("char id=86 x=4 y=0 width=3 height=2 xoffset=0 yoffset=0 xadvance=4"));
        let pixel = |x: usize, y: usize| rgba[(y * aw + x) * 4..(y * aw + x) * 4 + 4].to_vec();
        // 'V' draws its top row in set 0 and the same pixels in set 1 (3 rows down), in red.
        assert_eq!(pixel(4, 0), [200, 200, 200, 255]);
        assert_eq!(pixel(4, 1), [0, 0, 0, 0]);
        assert_eq!(pixel(4, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn dots_go_over_a_letter_that_has_room_and_make_the_glyph_grow_when_it_has_none() {
        // A lowercase letter starting at row 2 of 4: dots at row 0, a clear row between.
        let a = glyph('a', 5, 4, &[".....", ".....", ".111.", "1...1"]);
        let umlaut = with_diaeresis(&a, 'ä', 1);
        assert_eq!((umlaut.height, umlaut.y_offset), (4, 0));
        assert_eq!(&umlaut.px[0..5], &[0, 1, 0, 1, 0]);
        assert_eq!(&umlaut.px[5..10], &[0; 5], "one clear row under the dots");
        assert_eq!(&umlaut.px[10..], &a.px[10..]);
        // A capital at the top of the cell: two rows are added above it.
        let cap = glyph('A', 3, 3, &[".1.", "1.1", "111"]);
        let umlaut = with_diaeresis(&cap, 'Ä', 1);
        assert_eq!((umlaut.height, umlaut.y_offset), (5, -2));
        assert_eq!(&umlaut.px[0..3], &[1, 0, 1]);
        assert_eq!(&umlaut.px[3..6], &[0; 3]);
        assert_eq!(&umlaut.px[6..], &cap.px[..]);
    }

    #[test]
    fn dots_of_two_pixels_are_square_and_symmetric() {
        let wide = glyph(
            'o',
            10,
            6,
            &[
                "..........",
                "..........",
                "..........",
                "..........",
                ".11111111.",
                ".11111111.",
            ],
        );
        let umlaut = with_diaeresis(&wide, 'ö', 2);
        let rows: Vec<&[u8]> = umlaut.px.chunks(10).collect();
        // Room: dot 2 + gap 1 = 3 rows above the letter at row 4: starts at row 1.
        assert_eq!(rows[1], &[0, 0, 1, 1, 0, 0, 1, 1, 0, 0]);
        assert_eq!(rows[2], rows[1]);
        assert!(rows[3].iter().all(|&c| c == 0));
    }

    #[test]
    fn the_eszett_has_the_curve_of_b_over_the_bowl_of_small_b_and_a_gap_at_the_bottom() {
        let capital = glyph('B', 4, 6, &["111.", "1..1", "111.", "1..1", "1..1", "111."]);
        let small = glyph('b', 4, 6, &["1...", "1...", "111.", "1..1", "1..1", "111."]);
        let o = glyph('o', 4, 6, &["....", "....", ".11.", "1..1", "1..1", ".11."]);
        let eszett = eszett(&capital, &small, &o);
        assert_eq!(eszett.ch, 'ß');
        let rows: Vec<&[u8]> = eszett.px.chunks(4).collect();
        assert_eq!(rows[0], &[1, 1, 1, 0], "the top curve is the capital's");
        assert_eq!(rows[1], &[1, 0, 0, 1]);
        assert_eq!(
            rows[2],
            &[1, 1, 1, 0],
            "from the small o's height the bowl is the small b's"
        );
        assert_eq!(
            rows[5],
            &[1, 0, 1, 0],
            "the bowl stops short of the stem at the bottom"
        );
    }

    #[test]
    fn german_gets_six_umlauts_and_an_eszett_from_the_ascii_letters() {
        let ascii: Vec<BmGlyph> = "AOUaoubB"
            .chars()
            .map(|c| glyph(c, 3, 3, &["111", "1.1", "1.1"]))
            .collect();
        let german = german_glyphs(&ascii, 1);
        let letters: String = german.iter().map(|g| g.ch).collect();
        assert_eq!(letters, "ÄÖÜäöüß");
        assert!(german_glyphs(&[], 1).is_empty());
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
