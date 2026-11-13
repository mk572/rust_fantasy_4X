//! The sprite pack: the pictures of a folder (a unit's frames, a spell's) as ordinary PNG sheets.
//! `f4x-extract pack` writes them (`pack_build`); the client reads them (`pack_set`) and still finds
//! loose PNGs where no sheet has the picture (`docs/local-assets.md`).
//!
//! A sheet is a plain PNG that any image tool opens. Its pictures sit on a uniform grid: every cell
//! is `cell_w` by `cell_h` pixels (the largest picture on the sheet), `cols` cells to a row, so the
//! offset of picture number `i` is `(i % cols * cell_w, i / cols * cell_h)`. Smaller pictures sit at
//! the top left of their cell, the rest is transparent. What the grid cannot say, the PNG says in
//! one text chunk, keyword `f4x-sheet`:
//!
//! ```text
//! cell <cell_w>x<cell_h>
//! <name> TAB <width> TAB <height>      (one line for each picture, in grid order)
//! ```
//!
//! `cols` is the image's width divided by `cell_w`. A folder can have several sheets (pictures of
//! very different sizes are not put on one grid); a sheet's file name is `<name>.sheet.png`, then
//! `<name>.sheet2.png`, and so on. This module needs no PNG code: it reads the chunk headers.

use std::collections::BTreeMap;

/// The PNG text chunk's keyword.
pub const KEYWORD: &str = "f4x-sheet";

/// Where one picture is: a rectangle of one of a pack's sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub sheet: usize,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// One sheet's grid and picture list, read from its PNG bytes (which stay with the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub cell_w: usize,
    pub cell_h: usize,
    pub cols: usize,
    /// Name and size of each picture, in grid order.
    pub pictures: Vec<(String, usize, usize)>,
}

fn be32(b: &[u8]) -> Option<usize> {
    let a: [u8; 4] = b.get(..4)?.try_into().ok()?;
    usize::try_from(u32::from_be_bytes(a)).ok()
}

/// The width of a PNG and the text of its chunk `keyword` (a `tEXt` chunk, Latin-1 read as UTF-8).
fn png_width_and_text(png: &[u8], keyword: &str) -> Result<(usize, Option<String>), String> {
    const SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if !png.starts_with(SIGNATURE) {
        return Err("sheet: not a PNG".to_string());
    }
    let (mut at, mut width, mut text) = (SIGNATURE.len(), None, None);
    while let (Some(len), Some(kind)) = (png.get(at..).and_then(be32), png.get(at + 4..at + 8)) {
        let data = png
            .get(at + 8..at + 8 + len)
            .ok_or("sheet: a chunk ends early")?;
        if kind == b"IHDR" {
            width = be32(data);
        } else if kind == b"tEXt" {
            if let Some(split) = data.iter().position(|&b| b == 0) {
                if &data[..split] == keyword.as_bytes() {
                    text = Some(String::from_utf8_lossy(&data[split + 1..]).into_owned());
                }
            }
        } else if kind == b"IDAT" {
            // The text chunk is written before the pixels.
            break;
        }
        at += 12 + len;
    }
    Ok((width.ok_or("sheet: no header")?, text))
}

impl Sheet {
    /// Read the grid and the picture list of a sheet's PNG bytes.
    pub fn parse(png: &[u8]) -> Result<Sheet, String> {
        let (width, text) = png_width_and_text(png, KEYWORD)?;
        let text = text.ok_or("sheet: no picture list")?;
        let mut lines = text.lines();
        let cell = lines
            .next()
            .and_then(|l| l.strip_prefix("cell "))
            .and_then(|l| l.split_once('x'))
            .and_then(|(w, h)| Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?)))
            .ok_or("sheet: no cell size")?;
        let (cell_w, cell_h) = cell;
        if cell_w == 0 || cell_h == 0 || width < cell_w {
            return Err("sheet: bad cell size".to_string());
        }
        let mut pictures = Vec::new();
        for line in lines {
            let mut f = line.split('\t');
            let (Some(name), Some(w), Some(h)) = (f.next(), f.next(), f.next()) else {
                return Err(format!("sheet: bad line {line:?}"));
            };
            let size = |s: &str| {
                s.parse::<usize>()
                    .map_err(|_| format!("sheet: bad size {s:?}"))
            };
            let (w, h) = (size(w)?, size(h)?);
            if w > cell_w || h > cell_h {
                return Err(format!("sheet: {name} is bigger than its cell"));
            }
            pictures.push((name.to_string(), w, h));
        }
        Ok(Sheet {
            cell_w,
            cell_h,
            cols: width / cell_w,
            pictures,
        })
    }

    /// The rectangle of picture number `index`.
    pub fn rect(&self, index: usize) -> Option<(usize, usize, usize, usize)> {
        let (_, w, h) = self.pictures.get(index)?;
        let x = index % self.cols * self.cell_w;
        let y = index / self.cols * self.cell_h;
        Some((x, y, *w, *h))
    }
}

/// All the sheets of one folder, to look a picture up by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pack {
    pub sheets: Vec<Sheet>,
    names: BTreeMap<String, (usize, usize)>,
}

impl Pack {
    pub fn new(sheets: Vec<Sheet>) -> Pack {
        let mut names = BTreeMap::new();
        for (s, sheet) in sheets.iter().enumerate() {
            for (i, (name, _, _)) in sheet.pictures.iter().enumerate() {
                names.insert(name.clone(), (s, i));
            }
        }
        Pack { sheets, names }
    }

    /// Where the picture called `name` (a file name such as `figure_0_00.png`) is.
    pub fn frame(&self, name: &str) -> Option<Frame> {
        let &(sheet, index) = self.names.get(name)?;
        let (x, y, w, h) = self.sheets.get(sheet)?.rect(index)?;
        Some(Frame { sheet, x, y, w, h })
    }

    /// Every picture name, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.keys().map(String::as_str)
    }
}

/// The text of the chunk for a sheet of cells `cell_w` by `cell_h` holding `pictures`.
pub fn sheet_text(cell_w: usize, cell_h: usize, pictures: &[(String, usize, usize)]) -> String {
    let mut text = format!("cell {cell_w}x{cell_h}");
    for (name, w, h) in pictures {
        text.push_str(&format!("\n{name}\t{w}\t{h}"));
    }
    text
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A PNG with only the chunks the reader looks at (no valid pixels, no checksums).
    pub fn fake_png(width: u32, text: Option<&str>) -> Vec<u8> {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut chunk = |kind: &[u8; 4], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&[0; 4]);
        };
        let mut header = width.to_be_bytes().to_vec();
        header.extend_from_slice(&[0; 9]);
        chunk(b"IHDR", &header);
        if let Some(text) = text {
            let mut data = KEYWORD.as_bytes().to_vec();
            data.push(0);
            data.extend_from_slice(text.as_bytes());
            chunk(b"tEXt", &data);
        }
        chunk(b"IDAT", &[]);
        out
    }

    #[test]
    fn offsets_follow_the_grid_and_names_find_their_cell() -> TestResult {
        let pictures = vec![
            ("a.png".to_string(), 10, 8),
            ("b.png".to_string(), 12, 6),
            ("c.png".to_string(), 3, 3),
        ];
        let png = fake_png(24, Some(&sheet_text(12, 8, &pictures)));
        let sheet = Sheet::parse(&png)?;
        assert_eq!(sheet.cols, 2);
        assert_eq!(sheet.pictures, pictures);
        let pack = Pack::new(vec![sheet]);
        let f = |n: &str| pack.frame(n).map(|f| (f.x, f.y, f.w, f.h));
        assert_eq!(f("a.png"), Some((0, 0, 10, 8)));
        assert_eq!(f("b.png"), Some((12, 0, 12, 6)));
        assert_eq!(f("c.png"), Some((0, 8, 3, 3)));
        assert_eq!(f("d.png"), None);
        assert_eq!(pack.names().count(), 3);
        Ok(())
    }

    #[test]
    fn a_sheet_without_its_list_or_with_a_bad_one_is_refused() {
        assert!(Sheet::parse(b"not a png").is_err());
        assert!(Sheet::parse(&fake_png(24, None)).is_err());
        assert!(Sheet::parse(&fake_png(24, Some("cell 0x8\na.png\t1\t1"))).is_err());
        assert!(Sheet::parse(&fake_png(24, Some("cell 12x8\na.png\t13\t1"))).is_err());
        assert!(Sheet::parse(&fake_png(24, Some("cell 12x8\na.png\tx\t1"))).is_err());
        assert!(Sheet::parse(&fake_png(4, Some("cell 12x8"))).is_err());
        let whole = fake_png(24, Some("cell 12x8\na.png\t1\t1"));
        for cut in 0..whole.len() - 16 {
            assert!(Sheet::parse(&whole[..cut]).is_err(), "cut at {cut}");
        }
    }
}
