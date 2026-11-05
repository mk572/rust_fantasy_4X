//! A reader for BDF (Glyph Bitmap Distribution Format) bitmap fonts, the text format of X11 and
//! of fonts such as Spleen. Only what a game font needs: the ascent and descent, and per glyph
//! its code point, size, offset, advance and bitmap. See `docs/fonts.md`.

use std::collections::BTreeMap;

/// One glyph: a bitmap `width` by `height` (alpha 0 or 255) and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdfGlyph {
    pub width: usize,
    pub height: usize,
    /// Left edge of the bitmap from the pen (`BBX` x offset).
    pub x_offset: i32,
    /// Bottom edge of the bitmap above the baseline (`BBX` y offset).
    pub y_offset: i32,
    pub advance: i32,
    pub alpha: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdfFont {
    pub name: String,
    pub ascent: i32,
    pub descent: i32,
    pub glyphs: BTreeMap<char, BdfGlyph>,
}

/// The numbers after a keyword, or an error naming the line.
fn numbers(line: &str, count: usize) -> Result<Vec<i32>, String> {
    let values: Vec<i32> = line
        .split_whitespace()
        .skip(1)
        .map_while(|v| v.parse().ok())
        .collect();
    if values.len() < count {
        return Err(format!("BDF line `{line}` needs {count} numbers"));
    }
    Ok(values)
}

/// One bitmap row: `width` bits, most significant first, in whole bytes of hex.
fn bitmap_row(hex: &str, width: usize) -> Result<Vec<u8>, String> {
    let bytes = width.div_ceil(8);
    if hex.len() != bytes * 2 {
        return Err(format!("BDF bitmap row `{hex}` is not {bytes} bytes"));
    }
    let mut row = Vec::with_capacity(width);
    for x in 0..width {
        let byte = u8::from_str_radix(&hex[x / 8 * 2..x / 8 * 2 + 2], 16)
            .map_err(|_| format!("BDF bitmap row `{hex}` is not hexadecimal"))?;
        row.push(if byte & (0x80 >> (x % 8)) != 0 {
            255
        } else {
            0
        });
    }
    Ok(row)
}

pub fn parse(text: &str) -> Result<BdfFont, String> {
    let mut font = BdfFont {
        name: String::new(),
        ascent: 0,
        descent: 0,
        glyphs: BTreeMap::new(),
    };
    let mut bounding_box = None;
    let mut lines = text.lines();
    if !lines.next().is_some_and(|l| l.starts_with("STARTFONT")) {
        return Err("not a BDF font (no STARTFONT)".to_string());
    }
    // The glyph being read: code point, size, offsets, advance.
    let mut current: Option<(i32, [i32; 4], i32)> = None;
    while let Some(line) = lines.next() {
        let key = line.split_whitespace().next().unwrap_or("");
        match key {
            "FONT" => font.name = line[4..].trim().to_string(),
            "FONT_ASCENT" => font.ascent = numbers(line, 1)?[0],
            "FONT_DESCENT" => font.descent = numbers(line, 1)?[0],
            "FONTBOUNDINGBOX" => bounding_box = Some(numbers(line, 4)?),
            "STARTCHAR" => current = Some((-1, [0; 4], 0)),
            "ENCODING" => {
                if let Some(glyph) = current.as_mut() {
                    glyph.0 = numbers(line, 1)?[0];
                }
            }
            "DWIDTH" => {
                if let Some(glyph) = current.as_mut() {
                    glyph.2 = numbers(line, 1)?[0];
                }
            }
            "BBX" => {
                if let Some(glyph) = current.as_mut() {
                    let b = numbers(line, 4)?;
                    glyph.1 = [b[0], b[1], b[2], b[3]];
                }
            }
            "BITMAP" => {
                let (code, [w, h, x_offset, y_offset], advance) =
                    current.ok_or("BDF BITMAP outside a glyph")?;
                let (width, height) = (w.max(0) as usize, h.max(0) as usize);
                let mut alpha = Vec::with_capacity(width * height);
                for _ in 0..height {
                    let row = lines.next().ok_or("BDF bitmap ends early")?.trim();
                    alpha.extend(bitmap_row(row, width)?);
                }
                // A code point of -1, or one that is no character, is left out.
                if let Some(ch) = u32::try_from(code).ok().and_then(char::from_u32) {
                    font.glyphs.insert(
                        ch,
                        BdfGlyph {
                            width,
                            height,
                            x_offset,
                            y_offset,
                            advance,
                            alpha,
                        },
                    );
                }
            }
            "ENDCHAR" => current = None,
            _ => {}
        }
    }
    if font.glyphs.is_empty() {
        return Err("BDF font has no glyphs".to_string());
    }
    // Without FONT_ASCENT and FONT_DESCENT the bounding box gives the line.
    if font.ascent == 0 && font.descent == 0 {
        let b = bounding_box.ok_or("BDF font has no ascent and no FONTBOUNDINGBOX")?;
        font.ascent = b[1] + b[3];
        font.descent = -b[3];
    }
    Ok(font)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = "STARTFONT 2.1
FONT -test-Tiny-medium-r-normal--8-80-75-75-c-40-iso10646-1
FONTBOUNDINGBOX 4 8 0 -2
STARTPROPERTIES 2
FONT_ASCENT 6
FONT_DESCENT 2
ENDPROPERTIES
CHARS 2
STARTCHAR A
ENCODING 65
SWIDTH 500 0
DWIDTH 4 0
BBX 3 2 1 -1
BITMAP
A0
40
ENDCHAR
STARTCHAR unused
ENCODING -1
DWIDTH 4 0
BBX 1 1 0 0
BITMAP
80
ENDCHAR
ENDFONT
";

    #[test]
    fn a_glyph_has_its_bitmap_offsets_and_advance() {
        let font = parse(FONT).unwrap();
        assert_eq!((font.ascent, font.descent), (6, 2));
        assert_eq!(
            font.glyphs.len(),
            1,
            "the glyph without a code point is left out"
        );
        let a = &font.glyphs[&'A'];
        assert_eq!(
            (a.width, a.height, a.x_offset, a.y_offset, a.advance),
            (3, 2, 1, -1, 4)
        );
        // 0xA0 = 101 and 0x40 = 010 in the first three bits.
        assert_eq!(a.alpha, vec![255, 0, 255, 0, 255, 0]);
    }

    #[test]
    fn the_bounding_box_gives_the_line_when_there_are_no_properties() {
        let text = FONT
            .replace("FONT_ASCENT 6\n", "")
            .replace("FONT_DESCENT 2\n", "");
        let font = parse(&text).unwrap();
        assert_eq!((font.ascent, font.descent), (6, 2));
    }

    #[test]
    fn rows_of_more_than_eight_pixels_span_two_bytes() {
        assert_eq!(
            bitmap_row("FF80", 9).unwrap(),
            vec![255, 255, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(bitmap_row("0080", 9).unwrap()[8], 255);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(parse("not a font").is_err());
        assert!(parse(&FONT.replace("A0\n40", "A0")).is_err());
        assert!(parse(&FONT.replace("A0", "ZZ")).is_err());
        assert!(parse("STARTFONT 2.1\nENDFONT\n").is_err());
    }
}
