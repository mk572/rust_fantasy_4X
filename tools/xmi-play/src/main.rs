//! `xmi-play`: play one of the original's XMI songs on an emulated Roland MT-32.
//!
//! The song is read (`xmi`), the timbres its `TIMB` chunk names are looked up in the DOS sound
//! system's library (`fat.mt`, a Miles "GTL" file) and uploaded to the emulated unit by SysEx, as the
//! DOS driver did, then the song is rendered to a WAV file and handed to `afplay` (macOS).
//!
//! Tests: in the library (`wav_bytes`, the OGG writer); this renders with the MT-32 ROMs (not in the
//! repository) and plays sound, so it is tried by hand.

mod terminal;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use terminal::Key;
use xmi_play::ogg;
use xmi_play::render::{self, wav_bytes, Unit};

const USAGE: &str = "usage: xmi-play [options] SONG.xmi|FOLDER

Render an XMI song (the original's music, from `f4x-extract audio`) on an emulated Roland MT-32
and play it with afplay. Given a folder, it shows a menu of its .xmi files instead:
Up/Down/PageUp/PageDown choose, Enter plays, Esc stops the song and returns to the menu,
Esc or q in the menu leaves.

options:
  -r, --roms DIR      folder with mt32_ctrl_2_07.rom and mt32_pcm.rom
                      (default: ../../reference/Roland MT-32/rom-new)
  -t, --timbres FILE  the timbre library (default: ../../assets-local/audio/timbres/fat.mt)
  -o, --out FILE      write the WAV here (default: a temporary file)
  -n, --no-play       render only, do not play
  -R, --render-all DIR  with a folder: render every song of it to DIR/NAME.ogg for the game, and do
                      not show the menu (`f4x-music` does this several songs at once)
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
    let unit = Unit::read(&roms, &timbres).map_err(Error::Failed)?;
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

/// Render one song on a fresh unit into `wav`.
fn render(
    unit: &Unit,
    song: &Path,
    wav: &Path,
    sequence: usize,
    verbose: bool,
) -> Result<(), Error> {
    let rendered = render::render(unit, song, sequence, verbose).map_err(Error::Failed)?;
    std::fs::write(wav, wav_bytes(&rendered.samples, rendered.rate))
        .map_err(|e| Error::Failed(format!("cannot write {}: {e}", wav.display())))?;
    println!(
        "{}: {:.1} s, {} events, wrote {}",
        song.display(),
        rendered.samples.len() as f64 / f64::from(rendered.rate),
        rendered.events,
        wav.display()
    );
    Ok(())
}

/// Render every song of `from` to an OGG in `to`, named like the song (`104_main_menu.xmi` to
/// `104_main_menu.ogg`), which is where the game looks for it (`assets-local/audio/music`).
/// `f4x-music` does the same for the setup, several songs at once.
fn render_folder(unit: &Unit, from: &Path, to: &Path, sequence: usize) -> Result<(), Error> {
    let songs = xmi_files(from)?;
    std::fs::create_dir_all(to)
        .map_err(|e| Error::Failed(format!("cannot create {}: {e}", to.display())))?;
    for song in &songs {
        let rendered = render::render(unit, song, sequence, false).map_err(Error::Failed)?;
        let ogg = to.join(song.with_extension("ogg").file_name().unwrap_or_default());
        ogg::write_stereo(&rendered.samples, rendered.rate, &ogg).map_err(Error::Failed)?;
        println!("{}", ogg.display());
    }
    println!("{} songs in {}", songs.len(), to.display());
    Ok(())
}

/// The `.xmi` files of a folder, sorted.
fn xmi_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    render::xmi_files(dir).map_err(Error::Failed)
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
