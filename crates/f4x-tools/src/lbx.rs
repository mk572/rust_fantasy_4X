//! Reader for the `.lbx` archive format: a header with an entry count and a
//! table of byte offsets, followed by the entries back to back.
//!
//! Layout (little endian): `u16` entry count, `u16` magic `0xFEAD`, `u32`
//! (unused/type), then `count + 1` `u32` offsets; entry `i` spans
//! `offsets[i]..offsets[i + 1]`.

use std::fmt;

const MAGIC: u16 = 0xFEAD;

#[derive(Debug, PartialEq, Eq)]
pub enum LbxError {
    TooShort,
    BadMagic(u16),
    BadOffsets,
}

impl fmt::Display for LbxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LbxError::TooShort => write!(f, "file too short for an lbx header"),
            LbxError::BadMagic(m) => write!(f, "bad lbx magic {m:#06x}"),
            LbxError::BadOffsets => {
                write!(f, "entry offsets are not increasing or run past the file")
            }
        }
    }
}

impl std::error::Error for LbxError {}

pub struct Lbx<'a> {
    data: &'a [u8],
    offsets: Vec<usize>,
}

fn u16_at(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([d[i], d[i + 1]])
}

fn u32_at(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

impl<'a> Lbx<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, LbxError> {
        if data.len() < 8 {
            return Err(LbxError::TooShort);
        }
        let count = u16_at(data, 0) as usize;
        let magic = u16_at(data, 2);
        if magic != MAGIC {
            return Err(LbxError::BadMagic(magic));
        }
        let table_end = 8 + 4 * (count + 1);
        if data.len() < table_end {
            return Err(LbxError::TooShort);
        }
        let offsets: Vec<usize> = (0..=count)
            .map(|i| u32_at(data, 8 + 4 * i) as usize)
            .collect();
        // The final offset is the file length; entries must not run backwards or past it.
        let sane = offsets.windows(2).all(|w| w[0] <= w[1])
            && offsets.first().is_some_and(|&first| first >= table_end)
            && offsets.last().is_some_and(|&last| last <= data.len());
        if !sane {
            return Err(LbxError::BadOffsets);
        }
        Ok(Self { data, offsets })
    }

    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn entry(&self, i: usize) -> &'a [u8] {
        &self.data[self.offsets[i]..self.offsets[i + 1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend(MAGIC.to_le_bytes());
        out.extend([0u8; 4]);
        let mut pos = 8 + 4 * (entries.len() + 1);
        for e in entries {
            out.extend((pos as u32).to_le_bytes());
            pos += e.len();
        }
        out.extend((pos as u32).to_le_bytes());
        for e in entries {
            out.extend(*e);
        }
        out
    }

    #[test]
    fn reads_entries() {
        let file = build(&[b"abc", b"", b"de"]);
        let lbx = Lbx::parse(&file).unwrap();
        assert_eq!(lbx.len(), 3);
        assert_eq!(lbx.entry(0), b"abc");
        assert_eq!(lbx.entry(1), b"");
        assert_eq!(lbx.entry(2), b"de");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Lbx::parse(b"xx").err(), Some(LbxError::TooShort));
        let mut f = build(&[b"abc"]);
        f[2] = 0;
        assert!(matches!(Lbx::parse(&f).err(), Some(LbxError::BadMagic(_))));
        let mut f = build(&[b"abc"]);
        f.truncate(f.len() - 1);
        assert_eq!(Lbx::parse(&f).err(), Some(LbxError::BadOffsets));
    }
}
