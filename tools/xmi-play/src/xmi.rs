//! XMI (AIL Extended MIDI) reading, and the Miles timbre library (`fat.mt`, "GTL").
//!
//! XMI is an IFF file: `FORM XDIR` (the song count), then `CAT XMID` with one `FORM XMID` per song, each
//! with a `TIMB` chunk (the timbres the song needs, as patch and bank pairs) and an `EVNT` chunk. In
//! `EVNT` a byte below 0x80 is a delay in ticks (consecutive ones add up); a note-on carries its duration
//! as a variable-length number after the velocity, so there are no note-offs. One tick is 1/120 s at the
//! default tempo of 500 000 us per quarter note, which is MIDI with 60 ticks per quarter note.

/// One MIDI message at a time in seconds.
pub struct Event {
    pub seconds: f64,
    pub bytes: Vec<u8>,
}

pub struct Song {
    /// The `TIMB` pairs: (patch, bank).
    pub timbres: Vec<(u8, u8)>,
    pub events: Vec<Event>,
}

const TICKS_PER_QUARTER: f64 = 60.0;

fn be32(b: &[u8], at: usize) -> Result<usize, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
        .ok_or_else(|| "the file ends inside a chunk header".to_string())
}

/// The `FORM XMID` bodies of the file.
fn songs(file: &[u8]) -> Result<Vec<&[u8]>, String> {
    if file.get(..4) != Some(b"FORM") || file.get(8..12) != Some(b"XDIR") {
        return Err("not an XMI file".into());
    }
    let mut at = 8 + be32(file, 4)?; // after the XDIR form
    if file.get(at..at + 4) != Some(b"CAT ") {
        return Err("no CAT chunk".into());
    }
    let end = (at + 8 + be32(file, at + 4)?).min(file.len());
    at += 12; // the "XMID" type
    let mut out = Vec::new();
    while at + 12 <= end {
        let size = be32(file, at + 4)?;
        if file.get(at..at + 4) == Some(b"FORM") {
            out.push(
                file.get(at + 12..at + 8 + size)
                    .ok_or("a song is cut short")?,
            );
        }
        at += 8 + size + (size & 1);
    }
    Ok(out)
}

pub fn parse(file: &[u8], sequence: usize) -> Result<Song, String> {
    let all = songs(file)?;
    let body = all
        .get(sequence)
        .ok_or_else(|| format!("the file has {} songs, no song {sequence}", all.len()))?;
    let (mut timb, mut evnt) = (&body[..0], &body[..0]);
    let mut at = 0;
    while at + 8 <= body.len() {
        let size = be32(body, at + 4)?;
        let data = body
            .get(at + 8..at + 8 + size)
            .ok_or("a chunk is cut short")?;
        match &body[at..at + 4] {
            b"TIMB" => timb = data,
            b"EVNT" => evnt = data,
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    let count = timb
        .get(..2)
        .map_or(0, |c| usize::from(u16::from_le_bytes([c[0], c[1]])));
    let timbres = timb
        .get(2..)
        .unwrap_or(&[])
        .chunks_exact(2)
        .take(count)
        .map(|p| (p[0], p[1]))
        .collect();
    Ok(Song {
        timbres,
        events: events(evnt)?,
    })
}

fn variable(b: &[u8], at: &mut usize) -> Result<u32, String> {
    let mut v = 0u32;
    loop {
        let byte = *b.get(*at).ok_or("the song ends inside a number")?;
        *at += 1;
        v = (v << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
}

fn events(b: &[u8]) -> Result<Vec<Event>, String> {
    // (tick, order, bytes): note-offs are made from the durations, then everything is sorted by tick.
    let mut raw: Vec<(u64, usize, Vec<u8>)> = Vec::new();
    let mut tempos: Vec<(u64, f64)> = vec![(0, 500_000.0)];
    let (mut at, mut tick) = (0, 0u64);
    while at < b.len() {
        let status = b[at];
        if status < 0x80 {
            tick += u64::from(status);
            at += 1;
            continue;
        }
        at += 1;
        let order = raw.len();
        match status {
            0xff => {
                let kind = *b.get(at).ok_or("cut short")?;
                at += 1;
                let len = variable(b, &mut at)? as usize;
                let data = b.get(at..at + len).ok_or("a meta event is cut short")?;
                at += len;
                if kind == 0x51 && len == 3 {
                    let us = f64::from(u32::from_be_bytes([0, data[0], data[1], data[2]]));
                    tempos.push((tick, us));
                } else if kind == 0x2f {
                    break;
                }
            }
            0xf0 => {
                let len = variable(b, &mut at)? as usize;
                let data = b.get(at..at + len).ok_or("a SysEx is cut short")?;
                at += len;
                let mut m = vec![0xf0];
                m.extend(data);
                raw.push((tick, order, m));
            }
            0x80..=0xef => {
                let two = matches!(status & 0xf0, 0xc0 | 0xd0);
                let data = b.get(at..at + if two { 1 } else { 2 }).ok_or("cut short")?;
                at += data.len();
                let mut m = vec![status];
                m.extend(data);
                if status & 0xf0 == 0x90 {
                    let length = u64::from(variable(b, &mut at)?);
                    raw.push((tick + length, order, vec![0x80 | (status & 15), data[0], 0]));
                }
                // The XMI loop and branch controllers (110 to 120) mean nothing to the unit.
                if !(status & 0xf0 == 0xb0 && (110..=120).contains(&data[0])) {
                    raw.push((tick, order, m));
                }
            }
            other => return Err(format!("unknown status byte {other:#04x} at {at}")),
        }
    }
    // Note-offs sort before the note-ons of the same tick, so that a repeated note is heard twice.
    raw.sort_by_key(|(t, order, m)| (*t, u8::from(m[0] & 0xf0 != 0x80), *order));
    let seconds = |tick: u64| {
        let (mut s, mut last, mut us) = (0.0, 0u64, 500_000.0);
        for &(t, new) in &tempos {
            if t >= tick {
                break;
            }
            s += (t - last) as f64 * us / TICKS_PER_QUARTER / 1e6;
            (last, us) = (t, new);
        }
        s + (tick - last) as f64 * us / TICKS_PER_QUARTER / 1e6
    };
    Ok(raw
        .into_iter()
        .map(|(t, _, bytes)| Event {
            seconds: seconds(t),
            bytes,
        })
        .collect())
}

/// A Roland SysEx data-set message (DT1) for the MT-32 (device 0x10, model 0x16).
fn dt1(address: usize, data: &[u8]) -> Vec<u8> {
    let addr = [
        ((address >> 14) & 0x7f) as u8,
        ((address >> 7) & 0x7f) as u8,
        (address & 0x7f) as u8,
    ];
    let sum: u32 = addr.iter().chain(data).map(|&b| u32::from(b)).sum();
    let mut m = vec![0xf0, 0x41, 0x10, 0x16, 0x12];
    m.extend(addr);
    m.extend(data);
    m.push(((128 - sum % 128) % 128) as u8);
    m.push(0xf7);
    m
}

/// The timbres of a GTL library: the directory is (patch, bank, 4-byte offset) entries ended by patch
/// 0xff; each timbre there is a 2-byte size that counts itself, then the 246 bytes of an MT-32 timbre.
fn library_timbre(library: &[u8], patch: u8, bank: u8) -> Option<&[u8]> {
    for e in library.chunks_exact(6) {
        if e[0] == 0xff {
            return None;
        }
        if e[0] == patch && e[1] == bank {
            let at = u32::from_le_bytes([e[2], e[3], e[4], e[5]]) as usize;
            return library.get(at + 2..at + 2 + 246);
        }
    }
    None
}

/// What the DOS driver did before a song: put each timbre of the `TIMB` chunk into the unit's timbre
/// memory (64 slots of 256 address units, from 0x080000) and point the patch of that number at it
/// (patch memory from 0x050000, 8 bytes per patch: group 2 = memory, slot, key shift 24, fine tune 50,
/// bender range 12, assign mode 0, reverb 1, dummy).
pub fn timbre_setup(timbres: &[(u8, u8)], library: &[u8], verbose: bool) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut slot = 0;
    for &(patch, bank) in timbres {
        let Some(timbre) = library_timbre(library, patch, bank).filter(|_| slot < 64) else {
            if verbose {
                println!(
                    "timbre patch {patch} bank {bank}: not in the library, the unit's own is used"
                );
            }
            continue;
        };
        if verbose {
            println!(
                "timbre patch {patch} bank {bank}: {:?} in slot {slot}",
                String::from_utf8_lossy(&timbre[..10])
            );
        }
        out.push(dt1(timbre_address(slot), timbre));
        out.push(dt1(
            patch_address(patch),
            &[2, slot as u8, 24, 50, 12, 0, 1, 0],
        ));
        slot += 1;
    }
    out
}

/// Timbre memory is 7-bit addressed: slot n is at 08 (2n) 00.
fn timbre_address(slot: usize) -> usize {
    0x08_0000 + (slot * 2) * 128
}

/// Patch memory: patch n is at 05 00 00 + 8n in 7-bit addressing.
fn patch_address(patch: u8) -> usize {
    let n = usize::from(patch) * 8;
    0x05_0000 + (n / 128) * 128 + n % 128
}
