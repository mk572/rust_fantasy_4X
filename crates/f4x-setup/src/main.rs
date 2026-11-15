//! `f4x-setup`: set the game up from the player's own copy of the original.
//!
//! Without options it opens windows (a folder picker and a progress window); `--cli` asks and
//! answers in the terminal. Nothing from the original leaves this machine: the pictures are written
//! to the player's data folder and nowhere else. See `docs/installer-design.md`.
//!
//! Tests: the steps and the flow are in the library; `gui.rs` is tried by hand.

mod gui;
mod terminal;

use std::path::PathBuf;
use std::process::ExitCode;

use f4x_extract::paths;
use f4x_lang::text::Args;
use f4x_setup::say::Say;

/// What the command line asked for.
struct Args_ {
    cli: bool,
    game: Option<PathBuf>,
    data: Option<PathBuf>,
    check_only: bool,
}

/// What went wrong with the command line (a key and its option), or that help was asked for.
enum CommandLine {
    Help,
    Unknown(String),
    NeedsFolder(String),
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args_, CommandLine> {
    let mut parsed = Args_ {
        cli: false,
        game: None,
        data: None,
        check_only: false,
    };
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cli" => parsed.cli = true,
            "--check" => parsed.check_only = true,
            "--game" | "--data" => {
                let folder = args.next().ok_or(CommandLine::NeedsFolder(arg.clone()))?;
                if arg == "--game" {
                    parsed.game = Some(folder.into());
                } else {
                    parsed.data = Some(folder.into());
                }
            }
            "-h" | "--help" => return Err(CommandLine::Help),
            _ => return Err(CommandLine::Unknown(arg)),
        }
    }
    Ok(parsed)
}

/// The game's own files and the player's data folder, as the game finds them.
fn roots(data: Option<&std::path::Path>) -> Result<paths::Roots, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let env = |name: &str| std::env::var(name).ok();
    let mut roots = paths::locate(&paths::Inputs {
        os: paths::Os::current(),
        cwd: &cwd,
        exe: &exe,
        env: &env,
        data_override: data,
    })?;
    roots.game = std::fs::canonicalize(&roots.game).unwrap_or(roots.game);
    Ok(roots)
}

fn run() -> Result<(), String> {
    let parsed = parse(std::env::args().skip(1));
    // The messages come from the game's own files, so the folders come first.
    let roots_result = roots(parsed.as_ref().ok().and_then(|a| a.data.as_deref()));
    let roots = roots_result?;
    let say = Say::load(&roots.game)?;
    let args = match parsed {
        Ok(args) => args,
        Err(CommandLine::Help) => return Err(say.plain("setup.usage")),
        Err(CommandLine::Unknown(option)) => {
            let sentence = say.get("setup.err_option", &Args::new().text("option", option));
            return Err(format!("{sentence}\n\n{}", say.plain("setup.usage")));
        }
        Err(CommandLine::NeedsFolder(option)) => {
            let sentence = say.get("setup.err_value", &Args::new().text("option", option));
            return Err(format!("{sentence}\n\n{}", say.plain("setup.usage")));
        }
    };
    if args.cli || args.game.is_some() || args.check_only {
        terminal::run(&say, &roots, args.game, args.check_only)
    } else {
        gui::run(say, roots)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            // An empty message: the problem was already shown.
            if !message.is_empty() {
                eprintln!("{message}");
            }
            ExitCode::FAILURE
        }
    }
}
