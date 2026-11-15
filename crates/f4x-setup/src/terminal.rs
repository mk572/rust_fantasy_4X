//! The setup in a terminal: questions on the standard input, sentences on the standard output.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use f4x_extract::paths::Roots;
use f4x_lang::text::Args;
use f4x_setup::flow::{self, Stopped, Ui};
use f4x_setup::say::Say;

/// The quote marks a terminal puts around a dragged folder (the double quote is written as its
/// code so the text test, which pairs the quotes of the sources, is not confused).
const QUOTES: [char; 2] = ['\'', '\u{22}'];

struct Terminal<'a> {
    say: &'a Say,
}

impl Ui for Terminal<'_> {
    fn welcome(&mut self, _folder: &Path) -> bool {
        true
    }

    fn choose_folder(&mut self, _start: Option<&Path>) -> Option<PathBuf> {
        print!("{}", self.say.plain("setup.ask_folder"));
        io::stdout().flush().ok()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).ok()?;
        // A folder dragged into a terminal arrives quoted or with escaped spaces.
        let text = line.trim().trim_matches(QUOTES).replace("\\ ", " ");
        (!text.is_empty()).then(|| PathBuf::from(text))
    }

    fn try_again(&mut self, problem: &str) -> bool {
        eprintln!("{problem}");
        false
    }
}

/// Run the setup in the terminal. `given` is the folder of `--game`.
pub fn run(
    say: &Say,
    roots: &Roots,
    given: Option<PathBuf>,
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
    let target = flow::target(&roots.data);
    println!(
        "{}",
        say.get(
            "setup.target",
            &Args::new().text("folder", target.display().to_string())
        )
    );
    let done = flow::run(say, &folder, &roots.data, &roots.game, &mut |sentence| {
        println!("{sentence}");
    })?;
    println!("{done}");
    Ok(())
}
