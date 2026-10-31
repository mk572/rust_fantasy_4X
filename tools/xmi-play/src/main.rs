//! `xmi-play`: play one of the original's XMI songs on an emulated Roland MT-32.
//!
//! The song is read (`xmi`), the timbres its `TIMB` chunk names are looked up in the DOS sound
//! system's library (`fat.mt`, a Miles "GTL" file) and uploaded to the emulated unit by SysEx, as the
//! DOS driver did, then the song is rendered to a WAV file and handed to `afplay` (macOS).
//!
//! Tests: `wav_bytes` below; the rest renders with the MT-32 ROMs (not in the repository) and plays sound.

mod terminal;
mod xmi;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use mt32_rs::analog::AnalogMode;
use mt32_rs::engine::Engine;
use mt32_rs::midi::Parser;
use terminal::Key;

const USAGE: &str = "usage: xmi-play [options] SONG.xmi|FOLDER

Render an XMI song (the original's music, from `f4x-tools extract-audio`) on an emulated Roland MT-32
and play it with afplay. Given a folder, it shows a menu of its .xmi files instead:
Up/Down/PageUp/PageDown choose, Enter plays, Esc stops the song and returns to the menu,
Esc or q in the menu leaves.

options:
  -r, --roms DIR      folder with mt32_ctrl_2_07.rom and mt32_pcm.rom
                      (default: ../../reference/Roland MT-32/rom-new)
  -t, --timbres FILE  the timbre library (default: ../../assets-local/audio/timbres/fat.mt)
  -o, --out FILE      write the WAV here (default: a temporary file)
  -n, --no-play       render only, do not play
  -R, --render-all DIR  with a folder: render every song of it to DIR/NAME.ogg (needs oggenc or
                      ffmpeg) for the game, and do not show the menu
  -s, --sequence N    which song of the file (default 0)
  -v, --verbose       list the timbres and the events
  -h, --help          print this help";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage(msg)) => {
            eprintln!("{msg}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Error::Failed(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

enum Error {
    Usage(String),
    Failed(String),
}

fn run(args: &[String]) -> Result<(), Error> {
    let mut roms = PathBuf::from("../../reference/Roland MT-32/rom-new");
    let mut timbres = PathBuf::from("../../assets-local/audio/timbres/fat.mt");
    let mut out = None;
    let mut play = true;
    let mut sequence = 0;
    let mut verbose = false;
    let mut render_all = None;
    let mut song = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| Error::Usage(format!("{name} needs a value")))
        };
        match a.as_str() {
            "-r" | "--roms" => roms = PathBuf::from(value(a)?),
            "-t" | "--timbres" => timbres = PathBuf::from(value(a)?),
            "-o" | "--out" => out = Some(PathBuf::from(value(a)?)),
            "-s" | "--sequence" => {
                sequence = value(a)?
                    .parse()
                    .map_err(|_| Error::Usage("--sequence needs a number".into()))?;
            }
            "-R" | "--render-all" => render_all = Some(PathBuf::from(value(a)?)),
            "-n" | "--no-play" => play = false,
            "-v" | "--verbose" => verbose = true,
            s if s.starts_with('-') => return Err(Error::Usage(format!("unknown option {s}"))),
            s => song = Some(PathBuf::from(s)),
        }
    }
    let song = song.ok_or_else(|| Error::Usage("no song or folder given".into()))?;
    let read = |p: &PathBuf| {
        std::fs::read(p).map_err(|e| Error::Failed(format!("cannot read {}: {e}", p.display())))
    };
    let unit = Unit {
        control: read(&roms.join("mt32_ctrl_2_07.rom"))?,
        pcm: read(&roms.join("mt32_pcm.rom"))?,
        library: read(&timbres)?,
    };
    let wav = out.unwrap_or_else(|| std::env::temp_dir().join("xmi-play.wav"));
    if let Some(dir) = render_all {
        return render_folder(&unit, &song, &dir, sequence);
    }
    if song.is_dir() {
        return menu(&song, &unit, &wav, sequence, verbose);
    }
    render(&unit, &song, &wav, sequence, verbose)?;
    if play {
        play_wav(&wav, false)?;
    }
    Ok(())
}

/// The ROMs and the timbre library, read once.
struct Unit {
    control: Vec<u8>,
    pcm: Vec<u8>,
    library: Vec<u8>,
}

/// Render one song on a fresh unit into `wav`.
fn render(
    unit: &Unit,
    song: &Path,
    wav: &Path,
    sequence: usize,
    verbose: bool,
) -> Result<(), Error> {
    let bytes = std::fs::read(song)
        .map_err(|e| Error::Failed(format!("cannot read {}: {e}", song.display())))?;
    let file = xmi::parse(&bytes, sequence).map_err(Error::Failed)?;
    let mut engine = Engine::open_with_analog(&unit.control, &unit.pcm, AnalogMode::Accurate)
        .map_err(|e| Error::Failed(format!("the ROMs are not usable: {e}")))?;
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
    std::fs::write(wav, wav_bytes(&samples, rate))
        .map_err(|e| Error::Failed(format!("cannot write {}: {e}", wav.display())))?;
    println!(
        "{}: {:.1} s, {} events, wrote {}",
        song.display(),
        samples.len() as f64 / f64::from(rate),
        file.events.len(),
        wav.display()
    );
    Ok(())
}

/// Render every song of `from` to an OGG in `to`, named like the song (`104_main_menu.xmi` to
/// `104_main_menu.ogg`), which is where the game looks for it (`assets-local/audio/music`).
fn render_folder(unit: &Unit, from: &Path, to: &Path, sequence: usize) -> Result<(), Error> {
    let songs = xmi_files(from)?;
    std::fs::create_dir_all(to)
        .map_err(|e| Error::Failed(format!("cannot create {}: {e}", to.display())))?;
    let wav = std::env::temp_dir().join("xmi-play-render.wav");
    for song in &songs {
        let ogg = to.join(song.with_extension("ogg").file_name().unwrap_or_default());
        render(unit, song, &wav, sequence, false)?;
        encode_ogg(&wav, &ogg)?;
    }
    let _ = std::fs::remove_file(&wav);
    println!("{} songs in {}", songs.len(), to.display());
    Ok(())
}

/// WAV to OGG Vorbis with `oggenc`, or else `ffmpeg`.
fn encode_ogg(wav: &Path, ogg: &Path) -> Result<(), Error> {
    let done = |r: std::io::Result<std::process::ExitStatus>| r.is_ok_and(|s| s.success());
    let quiet = |c: &mut Command| {
        c.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
    };
    if done(quiet(
        Command::new("oggenc")
            .args(["-q", "5", "-o"])
            .arg(ogg)
            .arg(wav),
    )) || done(quiet(
        Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(wav)
            .args(["-c:a", "libvorbis", "-q:a", "5"])
            .arg(ogg),
    )) {
        Ok(())
    } else {
        Err(Error::Failed(
            "cannot encode OGG: install oggenc (vorbis-tools) or ffmpeg".into(),
        ))
    }
}

/// The `.xmi` files of a folder, sorted.
fn xmi_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut songs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Error::Failed(format!("cannot read {}: {e}", dir.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xmi")))
        .collect();
    songs.sort();
    if songs.is_empty() {
        return Err(Error::Failed(format!("no .xmi files in {}", dir.display())));
    }
    Ok(songs)
}

/// Play a WAV with afplay. With `interruptible`, Esc (or q) stops it and the result is false.
fn play_wav(wav: &Path, interruptible: bool) -> Result<bool, Error> {
    let mut child = Command::new("afplay")
        .arg(wav)
        .spawn()
        .map_err(|e| Error::Failed(format!("cannot run afplay: {e}")))?;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| Error::Failed(format!("afplay: {e}")))?
        {
            return if status.success() {
                Ok(true)
            } else {
                Err(Error::Failed("afplay failed".into()))
            };
        }
        if interruptible && matches!(terminal::key(), Key::Esc | Key::Quit) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        if !interruptible {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// The folder menu: Up and Down choose, Enter plays, Esc during a song stops it and returns here,
/// Esc or q in the menu leaves.
fn menu(dir: &Path, unit: &Unit, wav: &Path, sequence: usize, verbose: bool) -> Result<(), Error> {
    let songs = xmi_files(dir)?;
    let _raw = terminal::Raw::enter().map_err(Error::Failed)?;
    let (mut at, mut top) = (0usize, 0usize);
    loop {
        let rows = terminal::rows().saturating_sub(4).max(3);
        if at < top {
            top = at;
        } else if at >= top + rows {
            top = at + 1 - rows;
        }
        let mut screen = String::from("\x1b[2J\x1b[H");
        screen += &format!("{}  ({} songs)\r\n\r\n", dir.display(), songs.len());
        for (i, p) in songs.iter().enumerate().skip(top).take(rows) {
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            screen += &if i == at {
                format!("\x1b[7m> {name}\x1b[0m\r\n")
            } else {
                format!("  {name}\r\n")
            };
        }
        screen += "\r\nUp/Down choose, Enter plays, Esc stops a song, Esc or q leaves";
        print!("{screen}");
        let _ = std::io::stdout().flush();
        match terminal::key() {
            Key::Up => at = at.saturating_sub(1),
            Key::Down => at = (at + 1).min(songs.len() - 1),
            Key::PageUp => at = at.saturating_sub(rows),
            Key::PageDown => at = (at + rows).min(songs.len() - 1),
            Key::Enter => {
                print!("\x1b[2J\x1b[H\r\nrendering {} ...\r\n", songs[at].display());
                let _ = std::io::stdout().flush();
                render(unit, &songs[at], wav, sequence, verbose)?;
                print!("playing, Esc to stop\r\n");
                let _ = std::io::stdout().flush();
                play_wav(wav, true)?;
            }
            Key::Esc | Key::Quit => return Ok(()),
            Key::None | Key::Other => {}
        }
    }
}

/// A 16-bit stereo PCM WAV file.
fn wav_bytes(samples: &[(i16, i16)], rate: u32) -> Vec<u8> {
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
}
