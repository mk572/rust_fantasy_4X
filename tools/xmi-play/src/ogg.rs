//! OGG Vorbis in this process (libvorbis through `vorbis_rs`), quality 0.5 like `oggenc -q 5`.

use std::fs::File;
use std::io::BufWriter;
use std::num::{NonZeroU32, NonZeroU8};
use std::path::Path;

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

/// Frames handed to the encoder at a time.
const BLOCK: usize = 8192;

/// Write 16-bit stereo frames at `rate` to the OGG file `dest`.
pub fn write_stereo(samples: &[(i16, i16)], rate: u32, dest: &Path) -> Result<(), String> {
    let left: Vec<f32> = samples.iter().map(|s| f32::from(s.0) / 32768.0).collect();
    let right: Vec<f32> = samples.iter().map(|s| f32::from(s.1) / 32768.0).collect();
    let file = File::create(dest).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    let rate = NonZeroU32::new(rate).ok_or("the emulation reports no sample rate")?;
    let channels = NonZeroU8::new(2).ok_or("no channels")?;
    let mut encoder = VorbisEncoderBuilder::new(rate, channels, BufWriter::new(file))
        .map_err(|e| format!("encoder: {e}"))?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: 0.5,
        })
        .build()
        .map_err(|e| format!("encoder: {e}"))?;
    let mut at = 0;
    while at < left.len() {
        let end = (at + BLOCK).min(left.len());
        encoder
            .encode_audio_block([&left[at..end], &right[at..end]])
            .map_err(|e| format!("encoder: {e}"))?;
        at = end;
    }
    encoder.finish().map_err(|e| format!("encoder: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_frames_make_a_readable_ogg_file() {
        let path = std::env::temp_dir().join(format!("xmi_play_ogg_{}.ogg", std::process::id()));
        let tone: Vec<(i16, i16)> = (0..16_000)
            .map(|i| {
                let x = (f64::from(i) * 440.0 * std::f64::consts::TAU / 32_000.0).sin();
                ((x * 8000.0) as i16, (x * -8000.0) as i16)
            })
            .collect();
        write_stereo(&tone, 32_000, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"OggS");
        assert!(bytes.windows(7).any(|w| w == b"\x01vorbis"));
        assert!(bytes.len() > 1_000);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn no_sample_rate_is_refused() {
        let path = std::env::temp_dir().join(format!("xmi_play_ogg0_{}.ogg", std::process::id()));
        assert!(write_stereo(&[(0, 0)], 0, &path).is_err());
    }
}
