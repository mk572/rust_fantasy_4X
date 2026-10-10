//! Decoder for the picture/animation entries inside `.lbx` files.
//!
//! An entry is a 16-byte header, an optional palette block, a table of frame
//! offsets and column-oriented run-length data. Pixels are 8-bit palette
//! indices; index 0 is transparent. Entries that are not pictures (text, data
//! tables, sound) fail [`decode`], which is how the extractor tells them apart.

pub struct Frame {
    /// `width * height` palette indices, row-major, 0 = transparent.
    pub pixels: Vec<u8>,
}

pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub frames: Vec<Frame>,
    /// Palette entries this picture sets itself: `(first colour, 6-bit RGB triples)`.
    pub palette: Option<(usize, Vec<u8>)>,
}

fn u16_at(d: &[u8], i: usize) -> Option<usize> {
    d.get(i..i + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
}

fn u32_at(d: &[u8], i: usize) -> Option<usize> {
    d.get(i..i + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// Decode one column-RLE frame into `buf` (`width * height`, row-major).
/// Returns `None` if the data is malformed.
fn draw_frame(data: &[u8], width: usize, height: usize, buf: &mut [u8]) -> Option<()> {
    let mut p = 0;
    for x in 0..width {
        let op = *data.get(p)?;
        p += 1;
        if op == 0xFF {
            continue;
        }
        let mut packet = *data.get(p)? as i32;
        p += 1;
        let rle = match op {
            0x80 => true,
            0x00 => false,
            _ => return None,
        };
        let mut y = 0usize;
        loop {
            let seq = *data.get(p)? as usize;
            let skip = *data.get(p + 1)? as usize;
            p += 2;
            y += skip;
            packet -= seq as i32 + 2;
            let mut left = seq as i32;
            while left > 0 {
                let b = *data.get(p)?;
                p += 1;
                left -= 1;
                if rle && b >= 224 {
                    let n = (b - 224) as usize + 1;
                    let v = *data.get(p)?;
                    p += 1;
                    left -= 1;
                    for _ in 0..n {
                        if y >= height {
                            return None;
                        }
                        buf[y * width + x] = v;
                        y += 1;
                    }
                } else {
                    if y >= height {
                        return None;
                    }
                    buf[y * width + x] = b;
                    y += 1;
                }
            }
            if packet < 1 {
                break;
            }
        }
    }
    Some(())
}

/// Try to decode an entry as a picture; `None` if it does not look like one.
pub fn decode(entry: &[u8]) -> Option<Picture> {
    let width = u16_at(entry, 0)?;
    let height = u16_at(entry, 2)?;
    let count = u16_at(entry, 6)?;
    let pal_off = u16_at(entry, 14)?;
    if !(1..=640).contains(&width) || !(1..=480).contains(&height) || !(1..=256).contains(&count) {
        return None;
    }
    let frame_type = *entry.get(16)?;
    let table = 18;
    // Offsets for count frames (plus an end marker the format keeps in the table).
    let offs: Vec<usize> = (0..count)
        .map(|i| u32_at(entry, table + 4 * i))
        .collect::<Option<_>>()?;
    if offs.iter().any(|&o| o + 1 >= entry.len()) || offs.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    let palette = if pal_off != 0 {
        let data_off = u16_at(entry, pal_off)?;
        let start = u16_at(entry, pal_off + 2)?;
        let n = u16_at(entry, pal_off + 4)?;
        if start + n > 256 {
            return None;
        }
        Some((start, entry.get(data_off..data_off + 3 * n)?.to_vec()))
    } else {
        None
    };
    let mut frames = Vec::with_capacity(count);
    let mut buf = vec![0u8; width * height];
    for &o in &offs {
        // Frame type 0: each frame is a delta drawn over the previous one.
        if frame_type != 0 {
            buf.iter_mut().for_each(|b| *b = 0);
        }
        draw_frame(&entry[o + 1..], width, height, &mut buf)?;
        frames.push(Frame {
            pixels: buf.clone(),
        });
    }
    Some(Picture {
        width,
        height,
        frames,
        palette,
    })
}

/// Expand a 6-bit VGA colour (0..=63) to 8 bits.
pub fn vga_to_rgb8(v: u8) -> u8 {
    let v = v & 0x3F;
    (v << 2) | (v >> 4)
}

/// Turn palette indices into RGBA using `palette` (256 6-bit RGB triples).
pub fn to_rgba(pixels: &[u8], palette: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 4);
    for &i in pixels {
        if i == 0 {
            out.extend([0, 0, 0, 0]);
        } else {
            let c = i as usize * 3;
            out.extend([
                vga_to_rgb8(palette[c]),
                vga_to_rgb8(palette[c + 1]),
                vga_to_rgb8(palette[c + 2]),
                255,
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 picture, one frame, copy packets: column 0 = [1,2], column 1 skips
    /// the first row then writes [3].
    fn tiny() -> Vec<u8> {
        let mut e = vec![0u8; 18 + 4];
        e[0..2].copy_from_slice(&2u16.to_le_bytes());
        e[2..4].copy_from_slice(&2u16.to_le_bytes());
        e[6..8].copy_from_slice(&1u16.to_le_bytes());
        e[16] = 1; // full-store frames
        let frame_at = (18 + 4) as u32;
        e[18..22].copy_from_slice(&frame_at.to_le_bytes());
        e.push(1); // frame lead byte
                   // column 0: op copy, packet length 4 (2 + 2 header), seq 2, skip 0, data 1 2
        e.extend([0x00, 4, 2, 0, 1, 2]);
        // column 1: op copy, packet 3, seq 1, skip 1, data 3
        e.extend([0x00, 3, 1, 1, 3]);
        e
    }

    #[test]
    fn decodes_copy_packets() {
        let pic = decode(&tiny()).expect("valid picture");
        assert_eq!((pic.width, pic.height, pic.frames.len()), (2, 2, 1));
        assert_eq!(pic.frames[0].pixels, vec![1, 0, 2, 3]);
        assert!(pic.palette.is_none());
    }

    #[test]
    fn decodes_run_length_packets() {
        let mut e = tiny();
        // Replace column 0 with an RLE run of three (op 0x80): 225 -> repeat 2, value 9.
        let at = 22 + 1;
        e.truncate(at);
        e.extend([0x80, 4, 2, 0, 225, 9]);
        e.extend([0xFF]);
        let pic = decode(&e).expect("valid picture");
        assert_eq!(pic.frames[0].pixels, vec![9, 0, 9, 0]);
    }

    #[test]
    fn rejects_non_pictures() {
        assert!(decode(b"Some text entry, not a picture at all.").is_none());
        assert!(decode(&[0; 64]).is_none());
        let mut bad = tiny();
        bad.truncate(bad.len() - 3);
        assert!(decode(&bad).is_none());
    }

    #[test]
    fn colour_conversion() {
        assert_eq!(vga_to_rgb8(0), 0);
        assert_eq!(vga_to_rgb8(63), 255);
        let rgba = to_rgba(&[0, 1], &[0, 0, 0, 63, 0, 32]);
        assert_eq!(&rgba[..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..], &[255, 0, 130, 255]);
    }
}
