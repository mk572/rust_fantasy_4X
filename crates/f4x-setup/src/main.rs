//! `f4x-setup`: set the game up from the player's own copy of the original.
//!
//! Nothing from the original leaves this machine: the pictures are written to the player's data
//! folder and nowhere else. See `docs/installer-design.md`.
//!
//! Tests: the logic is in the library (`original`, `install`); this file only asks and prints.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use f4x_extract::paths;
use f4x_setup::{install, original};

const USAGE: &str =
    "f4x-setup: extract the pictures of your own copy of Master of Magic for the game.

usage: f4x-setup [--game DIR] [--data DIR] [--check]
  --game DIR   the folder with your copy of the original (asked for when left out)
  --data DIR   where the game keeps its pictures and saves (default: the game's own folder for
               your user; on a Mac ~/Library/Application Support/Fantasy4X)
  --check      only check that the copy is complete; extract nothing
Nothing from the original is uploaded or copied anywhere but into the data folder.";

struct Args {
    game: Option<PathBuf>,
    data: Option<PathBuf>,
    check_only: bool,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        game: None,
        data: None,
        check_only: false,
    };
    let mut args = args;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a folder\n\n{USAGE}"))
        };
        match arg.as_str() {
            "--game" => parsed.game = Some(value("--game")?.into()),
            "--data" => parsed.data = Some(value("--data")?.into()),
            "--check" => parsed.check_only = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    Ok(parsed)
}

fn ask_for_folder() -> Result<PathBuf, String> {
    print!("Folder with your copy of Master of Magic (you can drag it here): ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    // A folder dragged into a terminal arrives quoted or with escaped spaces.
    let text = line.trim().trim_matches(['\'', '"']).replace("\\ ", " ");
    if text.is_empty() {
        return Err("no folder given.".into());
    }
    Ok(PathBuf::from(text))
}

fn run() -> Result<(), String> {
    let args = parse(std::env::args().skip(1))?;
    let chosen = match args.game {
        Some(dir) => dir,
        None => ask_for_folder()?,
    };
    let folder = original::find(Path::new(&chosen))?;
    println!("Found the game's files in {}", folder.display());
    let result = original::check(&folder)?;
    if !result.is_complete() {
        return Err(format!(
            "{}\nIs this a complete copy of the original game? Nothing was extracted.",
            result.explain()
        ));
    }
    println!("The copy is complete.");
    if args.check_only {
        return Ok(());
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let env = |name: &str| std::env::var(name).ok();
    let roots = paths::locate(&paths::Inputs {
        os: paths::Os::current(),
        cwd: &cwd,
        exe: &exe,
        env: &env,
        data_override: args.data.as_deref(),
    })?;
    let game = std::fs::canonicalize(&roots.game).unwrap_or(roots.game);
    println!(
        "The pictures go to {}",
        roots.data.join(install::ASSETS).display()
    );
    let summary = install::install_art(&folder, &roots.data, &game, &mut |sentence| {
        println!("{sentence}");
    })?;
    println!(
        "Done: {} pictures in {} files in {}",
        summary.pictures,
        summary.packs,
        summary.folder.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
