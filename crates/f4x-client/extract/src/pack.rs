//! The sprite pack: one file that holds all the pictures of a folder (a unit's frames, a spell's)
//! as one or more PNG sheets and a table of where each picture is on them. `f4x-extract pack`
//! writes them (`pack_build`); the client reads them (`pack_set`) and still finds loose PNGs where
//! no pack has the picture (`docs/local-assets.md`).
//!
//! Layout, little-endian:
//!
//! ```text
//! "F4XP"  version u8 (1)  sheets u16  frames u32
//! frames * ( name_len u16, name utf-8, sheet u16, x u16, y u16, w u16, h u16 )
//! sheets * ( len u32, PNG bytes )
//! ```
//!
//! This module needs no PNG code: a pack is a table and the bytes of its sheets.

use std::collections::BTreeMap;
use std::ops::Range;

const MAGIC: &[u8; 4] = b"F4XP";
const VERSION: u8 = 1;

/// Where one picture is: a rectangle of one of the pack's sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub sheet: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// A parsed pack: the table and the byte ranges of the sheets inside the file's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    bytes: Vec<u8>,
    frames: BTreeMap<String, Frame>,
    sheets: Vec<Range<usize>>,
}

/// A cursor over the file's bytes that never reads past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("pack: length overflow")?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or("pack: file ends early")?;
        self.at = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

impl Pack {
    /// Read a pack file's bytes; an error says what is wrong with them.
    pub fn parse(bytes: Vec<u8>) -> Result<Pack, String> {
        let mut r = Reader {
            bytes: &bytes,
            at: 0,
        };
        if r.take(4)? != MAGIC {
            return Err("pack: not a pack file".to_string());
        }
        let version = r.take(1)?[0];
        if version != VERSION {
            return Err(format!("pack: version {version}, expected {VERSION}"));
        }
        let sheet_count = usize::from(r.u16()?);
        let frame_count = r.u32()?;
        let mut frames = BTreeMap::new();
        for _ in 0..frame_count {
            let len = usize::from(r.u16()?);
            let name = std::str::from_utf8(r.take(len)?)
                .map_err(|_| "pack: a name is not text".to_string())?
                .to_string();
            let frame = Frame {
                sheet: r.u16()?,
                x: r.u16()?,
                y: r.u16()?,
                w: r.u16()?,
                h: r.u16()?,
            };
            if usize::from(frame.sheet) >= sheet_count {
                return Err(format!(
                    "pack: {name} is on sheet {}, which is missing",
                    frame.sheet
                ));
            }
            frames.insert(name, frame);
        }
        let mut sheets = Vec::with_capacity(sheet_count);
        for _ in 0..sheet_count {
            let len = usize::try_from(r.u32()?).map_err(|_| "pack: sheet too long")?;
            let start = r.at;
            r.take(len)?;
            sheets.push(start..start + len);
        }
        Ok(Pack {
            bytes,
            frames,
            sheets,
        })
    }

    /// The picture called `name` (a file name such as `figure_0_00.png`).
    pub fn frame(&self, name: &str) -> Option<Frame> {
        self.frames.get(name).copied()
    }

    /// Every picture name, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.frames.keys().map(String::as_str)
    }

    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    /// The PNG bytes of sheet `index`.
    pub fn sheet_png(&self, index: u16) -> Option<&[u8]> {
        self.bytes.get(self.sheets.get(usize::from(index))?.clone())
    }
}

/// The file for pictures placed on sheets: `frames` are `(name, frame)` and `sheets` the PNG
/// bytes of each sheet.
pub fn encode(frames: &[(String, Frame)], sheets: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    let sheet_count = u16::try_from(sheets.len()).map_err(|_| "pack: too many sheets")?;
    out.extend_from_slice(&sheet_count.to_le_bytes());
    let frame_count = u32::try_from(frames.len()).map_err(|_| "pack: too many pictures")?;
    out.extend_from_slice(&frame_count.to_le_bytes());
    for (name, f) in frames {
        let len = u16::try_from(name.len()).map_err(|_| format!("pack: name too long: {name}"))?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        for v in [f.sheet, f.x, f.y, f.w, f.h] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for sheet in sheets {
        let len = u32::try_from(sheet.len()).map_err(|_| "pack: sheet too long")?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(sheet);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn frame(sheet: u16, x: u16) -> Frame {
        Frame {
            sheet,
            x,
            y: 2,
            w: 3,
            h: 4,
        }
    }

    #[test]
    fn a_pack_reads_back_what_was_written() -> TestResult {
        let frames = vec![
            ("figure_0_00.png".to_string(), frame(0, 0)),
            ("figure_0_01.png".to_string(), frame(1, 5)),
        ];
        let bytes = encode(&frames, &[vec![1, 2, 3], vec![9; 10]])?;
        let pack = Pack::parse(bytes)?;
        assert_eq!(pack.frame("figure_0_01.png"), Some(frame(1, 5)));
        assert_eq!(pack.frame("missing.png"), None);
        assert_eq!(pack.sheet_count(), 2);
        assert_eq!(pack.sheet_png(0), Some(&[1u8, 2, 3][..]));
        assert_eq!(pack.sheet_png(1).map(<[u8]>::len), Some(10));
        assert_eq!(pack.sheet_png(2), None);
        assert_eq!(pack.names().count(), 2);
        Ok(())
    }

    #[test]
    fn a_damaged_file_is_refused_not_read_past_its_end() -> TestResult {
        let frames = vec![("a.png".to_string(), frame(0, 0))];
        let bytes = encode(&frames, &[vec![7; 20]])?;
        assert!(Pack::parse(b"nope".to_vec()).is_err());
        let mut wrong = bytes.clone();
        wrong[4] = 9;
        assert!(Pack::parse(wrong).is_err());
        for cut in 0..bytes.len() {
            assert!(Pack::parse(bytes[..cut].to_vec()).is_err(), "cut at {cut}");
        }
        let on_missing_sheet = encode(&[("a.png".to_string(), frame(3, 0))], &[vec![1]])?;
        assert!(Pack::parse(on_missing_sheet).is_err());
        Ok(())
    }
}
