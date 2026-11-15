//! The setup in a terminal: questions on the standard input, sentences on the standard output.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use f4x_extract::paths::Roots;
use f4x_lang::text::Args;
use f4x_setup::flow::{self, Job, MusicChoice, Stopped, Ui};
use f4x_setup::roms;
use f4x_setup::say::Say;

use crate::net::Http;

/// The quote marks a terminal puts around a dragged folder (the double quote is written as its
/// code so the text test, which pairs the quotes of the sources, is not confused).
const QUOTES: [char; 2] = ['\'', '\u{22}'];

struct Terminal<'a> {
    say: &'a Say,
}

impl Terminal<'_> {
    /// One line from the player, with the quotes of a dragged folder taken off.
    fn line(&self, prompt: String) -> Option<String> {
        print!("{prompt}");
        io::stdout().flush().ok()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).ok()?;
        Some(line.trim().trim_matches(QUOTES).replace("\\ ", " "))
    }
}

impl Ui for Terminal<'_> {
    fn welcome(&mut self, _folder: &Path) -> bool {
        true
    }

    fn choose_folder(&mut self, _start: Option<&Path>) -> Option<PathBuf> {
        let text = self.line(self.say.plain("setup.ask_folder"))?;
        (!text.is_empty()).then(|| PathBuf::from(text))
    }

    fn try_again(&mut self, problem: &str) -> bool {
        eprintln!("{problem}");
        false
    }

    fn problem(&mut self, text: &str) {
        eprintln!("{text}");
    }

    fn music(&mut self) -> MusicChoice {
        let args = Args::new()
            .text("size", megabytes(roms::ZIP_BYTES))
            .text("page", roms::SOURCE_PAGE);
        match self.line(self.say.get("setup.ask_music", &args)) {
            Some(answer) if answer.eq_ignore_ascii_case("d") => MusicChoice::Download,
            Some(answer) if !answer.is_empty() => MusicChoice::Folder(PathBuf::from(answer)),
            _ => MusicChoice::Skip,
        }
    }
}

/// A size like "2.9 MB".
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Run the setup in the terminal. `given` is the folder of `--game`, `music` the answer about the
/// music from the command line, if any.
pub fn run(
    say: &Say,
    roots: &Roots,
    given: Option<PathBuf>,
    music: Option<MusicChoice>,
    check_only: bool,
) -> Result<(), String> {
    let mut ui = Terminal { say };
    let folder = match flow::choose_original(&mut ui, say, &roots.data, given) {
        Ok(folder) => folder,
        Err(Stopped::Cancelled) => return Err(say.plain("setup.no_folder")),
        // The problem was printed when it was found.
        Err(Stopped::Declined(_)) => return Err(String::new()),
    };
    println!(
        "{}",
        say.get(
            "setup.found",
            &Args::new().text("path", folder.display().to_string())
        )
    );
    println!("{}", say.plain("setup.complete"));
    if check_only {
        return Ok(());
    }
    let plan = match flow::choose_music(&mut ui, say, &roots.data, music) {
        Ok(plan) => plan,
        Err(Stopped::Cancelled) => return Err(say.plain("setup.no_folder")),
        Err(Stopped::Declined(_)) => return Err(String::new()),
    };
    let target = flow::target(&roots.data);
    println!(
        "{}",
        say.get(
            "setup.target",
            &Args::new().text("folder", target.display().to_string())
        )
    );
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let job = Job {
        say,
        original: &folder,
        data: &roots.data,
        game: &roots.game,
        music: plan,
        fetch: &Http,
        exe: &exe,
    };
    let done = flow::run(&job, &mut |sentence| println!("{sentence}"))?;
    println!("{done}");
    Ok(())
}
