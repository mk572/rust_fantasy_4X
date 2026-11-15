//! The order of things in the setup, apart from how it is shown: the player is asked through the
//! [`Ui`] trait (windows in the GUI, the terminal in `--cli`, a script in the tests).

use std::path::{Path, PathBuf};

use f4x_lang::text::Args;

use crate::install::{self, InstallError, Stage, Summary};
use crate::original;
use crate::roms::{self, Fetch};
use crate::say::Say;
use crate::sound;

/// What the setup asks of the player.
pub trait Ui {
    /// Say what is about to happen and where the pictures will go. `false`: the player stops.
    fn welcome(&mut self, folder: &Path) -> bool;
    /// Ask for the folder with the original; `start` is where to begin looking. `None`: cancelled.
    fn choose_folder(&mut self, start: Option<&Path>) -> Option<PathBuf>;
    /// Show a problem with the chosen folder and ask whether to choose another. `false`: stop.
    fn try_again(&mut self, problem: &str) -> bool;
    /// Show a problem and go on (nothing to choose).
    fn problem(&mut self, text: &str);
    /// Ask where the MT-32 ROMs for the music come from (the download is confirmed here).
    fn music(&mut self) -> MusicChoice;
}

/// Where the ROMs for the music come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicChoice {
    /// Make no music.
    Skip,
    /// Download them from the Internet Archive (the player agreed).
    Download,
    /// Use the files in this folder.
    Folder(PathBuf),
}

/// What the work does about the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicPlan {
    Skip,
    /// The checked ROMs are in the data folder already.
    Ready,
    /// Download them first.
    Download,
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

/// Settle where the music comes from. ROMs already in the data folder are used without asking.
/// `preset` is the answer given on the command line: it is not asked again, and a problem with it
/// ends the setup instead of asking.
pub fn choose_music(
    ui: &mut dyn Ui,
    say: &Say,
    data: &Path,
    preset: Option<MusicChoice>,
) -> Result<MusicPlan, Stopped> {
    let kept = roms::folder(data);
    if preset.is_none() && roms::check_folder(&kept).is_ok() {
        return Ok(MusicPlan::Ready);
    }
    let mut preset = preset;
    loop {
        let from_command_line = preset.is_some();
        match preset.take().unwrap_or_else(|| ui.music()) {
            MusicChoice::Skip => return Ok(MusicPlan::Skip),
            MusicChoice::Download => return Ok(MusicPlan::Download),
            MusicChoice::Folder(folder) => match roms::copy_into(&folder, &kept) {
                Ok(()) => return Ok(MusicPlan::Ready),
                Err(error) => {
                    let problem = error.explain(say);
                    ui.problem(&problem);
                    // A player is asked the question again; a command line has no one to ask.
                    if from_command_line {
                        return Err(Stopped::Declined(problem));
                    }
                }
            },
        }
    }
}

/// Everything the work needs.
pub struct Job<'a> {
    pub say: &'a Say,
    pub original: &'a Path,
    pub data: &'a Path,
    pub game: &'a Path,
    pub music: MusicPlan,
    pub fetch: &'a dyn Fetch,
    /// The running program, to find the music program beside it.
    pub exe: &'a Path,
}

/// Do the work. `report` gets a sentence when each stage starts (and as the music advances).
/// Returns the closing sentences, or the problem in plain words.
pub fn run(job: &Job, report: &mut dyn FnMut(&str)) -> Result<String, String> {
    let say = job.say;
    let explain = |error: InstallError| error.explain(say);
    let summary: Summary =
        install::install_art(job.original, job.data, job.game, &mut |stage: Stage| {
            report(&say.plain(stage.key()));
        })
        .map_err(explain)?;
    report(&say.plain("setup.stage_effects"));
    let effects = sound::install_effects(job.original, job.data).map_err(explain)?;
    let mut lines = vec![
        say.get(
            "setup.done",
            &Args::new()
                .count("pictures", summary.pictures)
                .count("packs", summary.packs)
                .text("folder", summary.folder.display().to_string()),
        ),
        say.get("setup.done_sounds", &Args::new().count("n", effects)),
    ];
    match make_music(job, report) {
        Ok(Some(songs)) => lines.push(say.get("setup.done_music", &Args::new().count("n", songs))),
        Ok(None) => lines.push(say.plain("setup.done_no_music")),
        Err(why) => {
            let problem = say.get("setup.err_music_failed", &Args::new().text("why", why));
            return Err(format!("{}\n\n{problem}", lines.join("\n")));
        }
    }
    Ok(lines.join("\n"))
}

/// The music step. `None` when none was asked for; otherwise how many songs.
fn make_music(job: &Job, report: &mut dyn FnMut(&str)) -> Result<Option<usize>, String> {
    let say = job.say;
    let roms_folder = roms::folder(job.data);
    match job.music {
        MusicPlan::Skip => return Ok(None),
        MusicPlan::Ready => {}
        MusicPlan::Download => {
            let mut last = u64::MAX;
            roms::download(job.fetch, &roms_folder, &mut |bytes| {
                let percent = (bytes * 100 / roms::ZIP_BYTES).min(100);
                if percent != last {
                    last = percent;
                    report(&say.get(
                        "setup.stage_download",
                        &Args::new().count("percent", percent),
                    ));
                }
            })
            .map_err(|error| error.explain(say))?;
        }
    }
    let tool = sound::locate_music_tool(job.exe, job.game)?;
    let songs = sound::install_music(
        job.original,
        job.data,
        &roms_folder,
        &tool,
        &mut |done, total| {
            report(&say.get(
                "setup.stage_music",
                &Args::new().count("done", done).count("total", total),
            ));
        },
    )
    .map_err(|error| error.explain(say))?;
    Ok(Some(songs))
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
        music: Vec<MusicChoice>,
        asked_music: usize,
    }

    impl Script {
        fn new(folders: Vec<Option<PathBuf>>) -> Self {
            Script {
                welcome: true,
                folders,
                again: true,
                problems: Vec::new(),
                started_at: Vec::new(),
                music: Vec::new(),
                asked_music: 0,
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
        fn problem(&mut self, text: &str) {
            self.problems.push(text.to_string());
        }
        fn music(&mut self) -> MusicChoice {
            self.asked_music += 1;
            self.music.remove(0)
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

    struct NoNetwork;
    impl Fetch for NoNetwork {
        fn get(&self, _: &str, _: u64, _: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
            Err("offline".into())
        }
    }

    #[test]
    fn the_work_reports_its_problem_in_plain_words() {
        let data = temp("link");
        fs::create_dir_all(data.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(data.join("elsewhere"), data.join("assets-local")).unwrap();
        let say = test_say();
        let job = Job {
            say: &say,
            original: Path::new("/x"),
            data: &data,
            game: Path::new("/y"),
            music: MusicPlan::Skip,
            fetch: &NoNetwork,
            exe: Path::new("/z"),
        };
        let mut told = Vec::new();
        let text = run(&job, &mut |s| told.push(s.to_string())).unwrap_err();
        assert!(text.contains("is a link to"), "{text}");
        assert!(told.is_empty());
    }

    #[test]
    fn no_music_and_a_download_are_taken_as_asked() {
        let data = temp("music_plain");
        let say = test_say();
        let mut ui = Script::new(Vec::new());
        ui.music = vec![MusicChoice::Skip];
        assert_eq!(
            choose_music(&mut ui, &say, &data, None),
            Ok(MusicPlan::Skip)
        );
        let mut ui = Script::new(Vec::new());
        ui.music = vec![MusicChoice::Download];
        assert_eq!(
            choose_music(&mut ui, &say, &data, None),
            Ok(MusicPlan::Download)
        );
        // A preset is not asked again.
        let mut ui = Script::new(Vec::new());
        assert_eq!(
            choose_music(&mut ui, &say, &data, Some(MusicChoice::Skip)),
            Ok(MusicPlan::Skip)
        );
        assert_eq!(ui.asked_music, 0);
    }

    #[test]
    fn a_wrong_rom_folder_is_explained_and_the_player_asked_again() {
        let data = temp("music_wrong");
        let wrong = temp("music_wrong_folder");
        let say = test_say();
        let mut ui = Script::new(Vec::new());
        ui.music = vec![MusicChoice::Folder(wrong.clone()), MusicChoice::Skip];
        assert_eq!(
            choose_music(&mut ui, &say, &data, None),
            Ok(MusicPlan::Skip)
        );
        assert_eq!(ui.asked_music, 2);
        assert!(
            ui.problems[0].contains("MT-32 ROM files are missing"),
            "{:?}",
            ui.problems
        );
        // The same from the command line ends the setup.
        let mut ui = Script::new(Vec::new());
        let stopped =
            choose_music(&mut ui, &say, &data, Some(MusicChoice::Folder(wrong))).unwrap_err();
        assert!(matches!(stopped, Stopped::Declined(_)), "{stopped:?}");
    }

    #[test]
    fn roms_already_in_the_data_folder_are_used_without_asking() {
        let real =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/Roland MT-32/rom-new");
        if roms::check_folder(&real).is_err() {
            eprintln!("no ROM files at reference/Roland MT-32/rom-new: skipped");
            return;
        }
        let data = temp("music_kept");
        roms::copy_into(&real, &roms::folder(&data)).unwrap();
        let mut ui = Script::new(Vec::new());
        assert_eq!(
            choose_music(&mut ui, &test_say(), &data, None),
            Ok(MusicPlan::Ready)
        );
        assert_eq!(ui.asked_music, 0);
    }
}
