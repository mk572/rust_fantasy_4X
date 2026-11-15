//! Rendering one song on a fresh emulated unit.

use std::path::{Path, PathBuf};

use mt32_rs::analog::AnalogMode;
use mt32_rs::engine::Engine;
use mt32_rs::midi::Parser;

use crate::xmi;

/// The two ROM files the renderer reads (the new revision).
pub const CONTROL_ROM: &str = "mt32_ctrl_2_07.rom";
pub const PCM_ROM: &str = "mt32_pcm.rom";

/// The ROMs and the timbre library, read once.
pub struct Unit {
    pub control: Vec<u8>,
    pub pcm: Vec<u8>,
    pub library: Vec<u8>,
}

impl Unit {
    /// Read the ROMs from `roms` and the timbre library from `timbres`.
    pub fn read(roms: &Path, timbres: &Path) -> Result<Unit, String> {
        let read =
            |p: PathBuf| std::fs::read(&p).map_err(|e| format!("cannot read {}: {e}", p.display()));
        Ok(Unit {
            control: read(roms.join(CONTROL_ROM))?,
            pcm: read(roms.join(PCM_ROM))?,
            library: read(timbres.to_path_buf())?,
        })
    }
}

/// A rendered song.
pub struct Rendered {
    pub samples: Vec<(i16, i16)>,
    pub rate: u32,
    pub events: usize,
}

/// Render `sequence` of the song file `song` on a fresh unit.
pub fn render(
    unit: &Unit,
    song: &Path,
    sequence: usize,
    verbose: bool,
) -> Result<Rendered, String> {
    let bytes = std::fs::read(song).map_err(|e| format!("cannot read {}: {e}", song.display()))?;
    let file = xmi::parse(&bytes, sequence)?;
    let mut engine = Engine::open_with_analog(&unit.control, &unit.pcm, AnalogMode::Accurate)
        .map_err(|e| format!("the ROMs are not usable: {e}"))?;
    let mut parser = Parser::new();
    for sysex in xmi::timbre_setup(&file.timbres, &unit.library, verbose) {
        parser.parse(&sysex, &mut engine);
    }
    let rate = engine.output_sample_rate();
    let mut samples: Vec<(i16, i16)> = Vec::new();
    let render = |engine: &mut Engine, frames: usize, samples: &mut Vec<(i16, i16)>| {
        let at = samples.len();
        samples.resize(at + frames, (0, 0));
        engine.render(&mut samples[at..]);
    };
    for event in &file.events {
        let due = (event.seconds * f64::from(rate)) as usize;
        if due > samples.len() {
            render(&mut engine, due - samples.len(), &mut samples);
        }
        if verbose {
            println!("{:8.3}s {:02x?}", event.seconds, event.bytes);
        }
        parser.parse(&event.bytes, &mut engine);
    }
    render(&mut engine, 3 * rate as usize, &mut samples); // the release and the reverb
    Ok(Rendered {
        samples,
        rate,
        events: file.events.len(),
    })
}

/// The `.xmi` files of a folder, sorted.
pub fn xmi_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut songs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xmi")))
        .collect();
    songs.sort();
    if songs.is_empty() {
        return Err(format!("no .xmi files in {}", dir.display()));
    }
    Ok(songs)
}

/// A 16-bit stereo PCM WAV file.
pub fn wav_bytes(samples: &[(i16, i16)], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 4) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes()); // PCM
    b.extend(2u16.to_le_bytes()); // channels
    b.extend(rate.to_le_bytes());
    b.extend((rate * 4).to_le_bytes());
    b.extend(4u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    for (l, r) in samples {
        b.extend(l.to_le_bytes());
        b.extend(r.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wav_header_describes_stereo_16_bit_pcm() -> Result<(), Box<dyn std::error::Error>> {
        let wav = wav_bytes(&[(1, -1), (2, -2)], 32_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into()?), 36 + 8);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into()?), 2, "channels");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into()?), 32_000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into()?), 128_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into()?), 8);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[44..48], &[1, 0, 255, 255]);
        Ok(())
    }

    #[test]
    fn songs_are_listed_sorted_and_an_empty_folder_is_an_error() {
        let dir = std::env::temp_dir().join(format!("xmi_play_list_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(xmi_files(&dir).is_err());
        for name in ["b.xmi", "a.XMI", "c.txt"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let names: Vec<String> = xmi_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.XMI", "b.xmi"]);
    }
}
