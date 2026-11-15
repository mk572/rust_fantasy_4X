//! The order of things in the setup, apart from how it is shown: the player is asked through the
//! [`Ui`] trait (windows in the GUI, the terminal in `--cli`, a script in the tests).

use std::path::{Path, PathBuf};

use f4x_lang::text::Args;

use crate::install::{self, Stage, Summary};
use crate::original;
use crate::say::Say;

/// What the setup asks of the player.
pub trait Ui {
    /// Say what is about to happen and where the pictures will go. `false`: the player stops.
    fn welcome(&mut self, folder: &Path) -> bool;
    /// Ask for the folder with the original; `start` is where to begin looking. `None`: cancelled.
    fn choose_folder(&mut self, start: Option<&Path>) -> Option<PathBuf>;
    /// Show a problem with the chosen folder and ask whether to choose another. `false`: stop.
    fn try_again(&mut self, problem: &str) -> bool;
}

/// Why the setup did not go on to the work.
#[derive(Debug, PartialEq, Eq)]
pub enum Stopped {
    /// The player cancelled a dialog.
    Cancelled,
    /// A problem was shown and the player did not want another try (the text is what was shown).
    Declined(String),
}

/// The pictures go here, inside the data folder.
pub fn target(data: &Path) -> PathBuf {
    data.join(install::ASSETS)
}

/// Welcome the player, then find a folder of the original that is complete. `first` is a folder
/// given on the command line; later folders are asked for through `ui`.
pub fn choose_original(
    ui: &mut dyn Ui,
    say: &Say,
    data: &Path,
    first: Option<PathBuf>,
) -> Result<PathBuf, Stopped> {
    if first.is_none() && !ui.welcome(&target(data)) {
        return Err(Stopped::Cancelled);
    }
    let mut given = first;
    let mut start: Option<PathBuf> = None;
    loop {
        let chosen = match given.take() {
            Some(path) => path,
            None => ui
                .choose_folder(start.as_deref())
                .ok_or(Stopped::Cancelled)?,
        };
        let problem = match original::find(&chosen) {
            Err(error) => error.explain(say),
            Ok(folder) => match original::check(&folder) {
                Err(error) => error.explain(say),
                Ok(check) if check.is_complete() => return Ok(folder),
                Ok(check) => format!("{}\n{}", check.explain(say), say.plain("setup.incomplete")),
            },
        };
        start = Some(chosen);
        if !ui.try_again(&problem) {
            return Err(Stopped::Declined(problem));
        }
    }
}

/// Do the work. `report` gets a sentence when each stage starts. Returns the closing sentence, or
/// the problem in plain words.
pub fn run(
    say: &Say,
    original: &Path,
    data: &Path,
    game: &Path,
    report: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let summary: Summary = install::install_art(original, data, game, &mut |stage: Stage| {
        report(&say.plain(stage.key()));
    })
    .map_err(|error| error.explain(say))?;
    Ok(say.get(
        "setup.done",
        &Args::new()
            .count("pictures", summary.pictures)
            .count("packs", summary.packs)
            .text("folder", summary.folder.display().to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::say::test_say;
    use std::fs;

    /// A player who gives the folders in order and answers the questions as told.
    struct Script {
        welcome: bool,
        folders: Vec<Option<PathBuf>>,
        again: bool,
        problems: Vec<String>,
        started_at: Vec<Option<PathBuf>>,
    }

    impl Script {
        fn new(folders: Vec<Option<PathBuf>>) -> Self {
            Script {
                welcome: true,
                folders,
                again: true,
                problems: Vec::new(),
                started_at: Vec::new(),
            }
        }
    }

    impl Ui for Script {
        fn welcome(&mut self, _folder: &Path) -> bool {
            self.welcome
        }
        fn choose_folder(&mut self, start: Option<&Path>) -> Option<PathBuf> {
            self.started_at.push(start.map(Path::to_path_buf));
            self.folders.remove(0)
        }
        fn try_again(&mut self, problem: &str) -> bool {
            self.problems.push(problem.to_string());
            self.again
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("f4x_flow_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn complete_copy(dir: &Path) {
        let names = include_str!("../original_files.txt");
        for name in names.lines().filter(|l| !l.is_empty()) {
            fs::write(dir.join(name), [0, 0, 0xAD, 0xFE, 0, 0, 0, 0]).unwrap();
        }
    }

    #[test]
    fn a_wrong_folder_is_explained_and_the_next_one_is_taken() {
        let wrong = temp("wrong");
        let right = temp("right");
        complete_copy(&right);
        let mut ui = Script::new(vec![Some(wrong.clone()), Some(right.clone())]);
        let found = choose_original(&mut ui, &test_say(), Path::new("/data"), None).unwrap();
        assert_eq!(found, right);
        assert_eq!(ui.problems.len(), 1);
        assert!(
            ui.problems[0].contains("does not look like the original"),
            "{:?}",
            ui.problems
        );
        // The picker starts where the player looked last.
        assert_eq!(ui.started_at, [None, Some(wrong)]);
    }

    #[test]
    fn an_incomplete_copy_says_what_is_missing_and_that_nothing_was_extracted() {
        let partial = temp("partial");
        complete_copy(&partial);
        fs::remove_file(partial.join("BOOK.LBX")).unwrap();
        let mut ui = Script::new(Vec::new());
        ui.again = false;
        let stopped =
            choose_original(&mut ui, &test_say(), Path::new("/data"), Some(partial)).unwrap_err();
        let Stopped::Declined(text) = stopped else {
            panic!("{stopped:?}")
        };
        assert!(text.contains("BOOK.LBX"), "{text}");
        assert!(text.contains("Nothing was extracted"), "{text}");
        assert_eq!(ui.problems, [text]);
    }

    #[test]
    fn cancelling_stops_quietly_at_each_dialog() {
        let mut ui = Script::new(vec![None]);
        ui.welcome = false;
        assert_eq!(
            choose_original(&mut ui, &test_say(), Path::new("/data"), None),
            Err(Stopped::Cancelled)
        );
        let mut ui = Script::new(vec![None]);
        assert_eq!(
            choose_original(&mut ui, &test_say(), Path::new("/data"), None),
            Err(Stopped::Cancelled)
        );
        assert!(ui.problems.is_empty());
    }

    #[test]
    fn a_folder_from_the_command_line_skips_the_welcome() {
        let right = temp("given");
        complete_copy(&right);
        let mut ui = Script::new(Vec::new());
        ui.welcome = false;
        assert_eq!(
            choose_original(
                &mut ui,
                &test_say(),
                Path::new("/data"),
                Some(right.clone())
            ),
            Ok(right)
        );
    }

    #[test]
    fn the_work_reports_its_problem_in_plain_words() {
        let data = temp("link");
        fs::create_dir_all(data.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(data.join("elsewhere"), data.join("assets-local")).unwrap();
        let mut told = Vec::new();
        let text = run(
            &test_say(),
            Path::new("/x"),
            &data,
            Path::new("/y"),
            &mut |s| {
                told.push(s.to_string());
            },
        )
        .unwrap_err();
        assert!(text.contains("is a link to"), "{text}");
        assert!(told.is_empty());
    }
}
