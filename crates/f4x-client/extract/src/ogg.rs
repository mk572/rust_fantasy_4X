//! Writing OGG Vorbis in this process (libvorbis through `vorbis_rs`), so that nobody needs
//! `oggenc` or `ffmpeg`. The quality is `oggenc`'s `-q 5` (0.5).

use std::fs::File;
use std::io::BufWriter;
use std::num::{NonZeroU32, NonZeroU8};
use std::path::Path;

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

/// Frames handed to the encoder at a time.
const BLOCK: usize = 8192;

/// Encode 8-bit unsigned mono PCM (the original's effects) to the OGG file `dest`.
pub fn encode_mono_u8(rate: u32, samples: &[u8], dest: &Path) -> Result<(), String> {
    let pcm: Vec<f32> = samples
        .iter()
        .map(|&b| (f32::from(b) - 128.0) / 128.0)
        .collect();
    encode(rate, &[pcm], dest)
}

/// Encode planar `f32` channels (each in -1 to 1) to the OGG file `dest`.
pub fn encode(rate: u32, channels: &[Vec<f32>], dest: &Path) -> Result<(), String> {
    let rate = NonZeroU32::new(rate).ok_or("a sound with no sample rate")?;
    let count = u8::try_from(channels.len())
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or("a sound with no channels")?;
    let frames = channels.first().map_or(0, Vec::len);
    let file = File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut encoder = VorbisEncoderBuilder::new(rate, count, BufWriter::new(file))
        .map_err(|e| format!("encoder: {e}"))?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: 0.5,
        })
        .build()
        .map_err(|e| format!("encoder: {e}"))?;
    let mut at = 0;
    while at < frames {
        let end = (at + BLOCK).min(frames);
        let block: Vec<&[f32]> = channels.iter().map(|c| &c[at..end]).collect();
        encoder
            .encode_audio_block(block)
            .map_err(|e| format!("encoder: {e}"))?;
        at = end;
    }
    encoder.finish().map_err(|e| format!("encoder: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("f4x_ogg_{name}_{}.ogg", std::process::id()))
    }

    /// Decode a file with `lewton`, the decoder family the game's sound library uses:
    /// `(rate, channels, samples per channel, peak)`.
    fn decode(path: &Path) -> (u32, u8, usize, i16) {
        let file = File::open(path).unwrap();
        let mut reader = lewton::inside_ogg::OggStreamReader::new(file).unwrap();
        let channels = reader.ident_hdr.audio_channels;
        let (mut frames, mut peak) = (0usize, 0i16);
        while let Some(packet) = reader.read_dec_packet().unwrap() {
            frames += packet[0].len();
            peak = peak.max(packet.iter().flatten().copied().max().unwrap_or(0));
        }
        (reader.ident_hdr.audio_sample_rate, channels, frames, peak)
    }

    #[test]
    fn a_tone_comes_back_with_its_rate_length_and_level() {
        let rate = 22_050u32;
        let tone: Vec<u8> = (0..rate)
            .map(|i| {
                let x = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
                (128.0 + x * 64.0) as u8
            })
            .collect();
        let path = temp("tone");
        encode_mono_u8(rate, &tone, &path).unwrap();
        let (got_rate, channels, frames, peak) = decode(&path);
        assert_eq!((got_rate, channels), (rate, 1));
        assert!(frames.abs_diff(tone.len()) < 2_048, "{frames}");
        // Half scale is 16384 of 32768, give or take what the encoder does.
        assert!((13_000..20_000).contains(&peak), "{peak}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn two_channels_and_silence_are_written() {
        let path = temp("stereo");
        encode(32_000, &[vec![0.0; 4_000], vec![0.0; 4_000]], &path).unwrap();
        let (rate, channels, frames, peak) = decode(&path);
        assert_eq!((rate, channels, peak), (32_000, 2, 0));
        assert!(frames.abs_diff(4_000) < 2_048, "{frames}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn no_rate_and_no_channels_are_refused() {
        let path = temp("bad");
        assert!(encode(0, &[vec![0.0]], &path).is_err());
        assert!(encode(22_050, &[], &path).is_err());
    }
}
