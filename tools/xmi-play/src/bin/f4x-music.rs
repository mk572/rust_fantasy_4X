//! `f4x-music`: render the original's songs to OGG with the MT-32 emulation. Started by `f4x-setup`;
//! it can also be run by hand. Several songs are rendered at once. Progress goes to the standard
//! output, one line for each finished song: `rendered N/M NAME`, and `done M` at the end. Problems go
//! to the standard error and the exit status is not zero.
//!
//! LGPL-2.1-or-later, like `mt32-rs` which it links (this folder is its own workspace, apart from
//! the game's: `docs/installer-design.md` 3.3).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use xmi_play::ogg;
use xmi_play::render::{self, Unit};

const USAGE: &str = "usage: f4x-music --songs DIR --out DIR --roms DIR --timbres FILE [--jobs N]

Render every .xmi song of --songs to an OGG file of the same name in --out.
  --roms DIR       the folder with mt32_ctrl_2_07.rom and mt32_pcm.rom
  --timbres FILE   the timbre library of the original (fat.mt)
  --jobs N         songs rendered at once (default: the number of processor cores, at most 8)";

struct Options {
    songs: PathBuf,
    out: PathBuf,
    roms: PathBuf,
    timbres: PathBuf,
    jobs: usize,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let (mut songs, mut out, mut roms, mut timbres, mut jobs) = (None, None, None, None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--songs" => songs = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--roms" => roms = Some(PathBuf::from(value()?)),
            "--timbres" => timbres = Some(PathBuf::from(value()?)),
            "--jobs" => {
                jobs = Some(
                    value()?
                        .parse::<usize>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("--jobs needs a number above 0")?,
                );
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    let need = |v: Option<PathBuf>, name: &str| v.ok_or(format!("{name} is missing"));
    let cores = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    Ok(Options {
        songs: need(songs, "--songs")?,
        out: need(out, "--out")?,
        roms: need(roms, "--roms")?,
        timbres: need(timbres, "--timbres")?,
        jobs: jobs.unwrap_or(cores),
    })
}

fn render_one(unit: &Unit, song: &Path, out: &Path) -> Result<(), String> {
    let rendered = render::render(unit, song, 0, false)?;
    let name = song.file_stem().unwrap_or_default();
    ogg::write_stereo(
        &rendered.samples,
        rendered.rate,
        &out.join(name).with_extension("ogg"),
    )
}

fn run(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    let unit = Unit::read(&options.roms, &options.timbres)?;
    let songs = render::xmi_files(&options.songs)?;
    std::fs::create_dir_all(&options.out)
        .map_err(|e| format!("cannot create {}: {e}", options.out.display()))?;
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let failure: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..options.jobs.min(songs.len()) {
            scope.spawn(|| loop {
                let at = next.fetch_add(1, Ordering::SeqCst);
                let Some(song) = songs.get(at) else { return };
                if failure.lock().is_ok_and(|f| f.is_some()) {
                    return;
                }
                match render_one(&unit, song, &options.out) {
                    Ok(()) => {
                        let n = finished.fetch_add(1, Ordering::SeqCst) + 1;
                        let name = song.file_stem().unwrap_or_default().to_string_lossy();
                        println!("rendered {n}/{} {name}", songs.len());
                    }
                    Err(why) => {
                        let name = song.file_name().unwrap_or_default().to_string_lossy();
                        if let Ok(mut slot) = failure.lock() {
                            slot.get_or_insert(format!("{name}: {why}"));
                        }
                        return;
                    }
                }
            });
        }
    });
    if let Some(why) = failure.into_inner().map_err(|e| e.to_string())? {
        return Err(why);
    }
    println!("done {}", songs.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
