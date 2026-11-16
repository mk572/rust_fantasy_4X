//! The first start: a player who has no pictures yet is offered the setup program
//! (`docs/installer-design.md` 6.7, step 9).
//!
//! The decision and the search for the setup are plain functions with tests; the question itself is
//! a macOS system dialog through `osascript` (Apple's own, nothing to ship). Other systems get a
//! hint on the standard error until they have a package.

use std::path::{Path, PathBuf};
use std::process::Command;

use f4x_extract::paths::Os;
use f4x_lang::text::{Args, Texts};

/// Present in the data folder when the player chose "don't ask again".
const DECLINED: &str = ".setup-declined";
/// Where the pictures are once the setup has run.
const ART: &str = "assets-local/art";
/// The setup app's name beside the game app, and its program name elsewhere.
const SETUP_APP: &str = "Fantasy 4X Setup.app";
const SETUP_PROGRAM: &str = "f4x-setup";

/// What the game knows when it starts.
pub struct Facts<'a> {
    pub game: &'a Path,
    pub data: &'a Path,
    /// A check or a screenshot run: nobody to ask.
    pub automated: bool,
}

/// What to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    Offer,
}

/// Ask only an installed game (its shipped files are not in the data folder) that has no pictures,
/// when a person is there and has not said "don't ask again".
pub fn decide(facts: &Facts) -> Decision {
    let installed = facts.game != facts.data;
    let has_art = facts.data.join(ART).is_dir();
    let declined = facts.data.join(DECLINED).exists();
    if installed && !has_art && !declined && !facts.automated {
        Decision::Offer
    } else {
        Decision::Nothing
    }
}

/// The setup program: the app beside the game app or in Applications on a Mac, the program beside
/// the game elsewhere.
pub fn find_setup(exe: &Path, os: Os, applications: &Path) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = match os {
        Os::Mac => {
            // exe is `<dir>/Fantasy4X.app/Contents/MacOS/f4x-client`.
            let beside = exe.ancestors().nth(4).map(|dir| dir.join(SETUP_APP));
            beside
                .into_iter()
                .chain([applications.join(SETUP_APP)])
                .collect()
        }
        Os::Windows | Os::Linux => exe
            .parent()
            .map(|dir| dir.join(SETUP_PROGRAM))
            .into_iter()
            .collect(),
    };
    candidates.into_iter().find(|p| p.exists())
}

/// The three answers of the dialog.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Setup,
    Later,
    Never,
}

/// The `osascript` output (`button returned:Set up now`) as an answer; anything else is "later".
pub fn parse_answer(output: &str, texts: &Texts) -> Answer {
    let button = output.trim().rsplit(':').next().unwrap_or("").trim();
    if button == texts.get("first_run.setup", &Args::new()) {
        Answer::Setup
    } else if button == texts.get("first_run.never", &Args::new()) {
        Answer::Never
    } else {
        Answer::Later
    }
}

/// Text inside an AppleScript string: the backslash and the quote are escaped.
fn script_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// The AppleScript that shows the question (the confirming button is on the right, the default).
pub fn dialog_script(texts: &Texts) -> String {
    let get = |key: &str| script_string(&texts.get(key, &Args::new()));
    format!(
        "display dialog \"{}\" with title \"{}\" buttons {{\"{}\", \"{}\", \"{}\"}} default button \"{}\"",
        get("first_run.ask"),
        get("first_run.title"),
        get("first_run.never"),
        get("first_run.later"),
        get("first_run.setup"),
        get("first_run.setup"),
    )
}

/// Run the first-start offer. Returns `true` when the setup was started and the game should close.
pub fn offer(facts: &Facts, exe: &Path, os: Os, texts: &Texts) -> bool {
    if decide(facts) != Decision::Offer {
        return false;
    }
    let applications = Path::new("/Applications");
    let Some(setup) = find_setup(exe, os, applications) else {
        eprintln!("{}", texts.get("first_run.no_setup", &Args::new()));
        return false;
    };
    if os != Os::Mac {
        eprintln!("{}", texts.get("first_run.hint", &Args::new()));
        return false;
    }
    // A closed dialog (Escape) makes osascript fail: that is "not now".
    let output = Command::new("osascript")
        .arg("-e")
        .arg(dialog_script(texts))
        .output();
    let answer = match output {
        Ok(out) if out.status.success() => {
            parse_answer(&String::from_utf8_lossy(&out.stdout), texts)
        }
        _ => Answer::Later,
    };
    match answer {
        Answer::Later => false,
        Answer::Never => {
            let _ = std::fs::write(facts.data.join(DECLINED), b"");
            false
        }
        Answer::Setup => match Command::new("open").arg(&setup).spawn() {
            Ok(_) => true,
            Err(e) => {
                let why = e.to_string();
                eprintln!(
                    "{}",
                    texts.get("first_run.err_start", &Args::new().text("why", why))
                );
                false
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f4x_lang::text::load_texts;
    use std::fs;

    fn texts() -> Texts {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-lang/content");
        load_texts(&[dir.as_path()], "en").expect("the shipped messages load")
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("f4x_first_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_an_installed_game_without_pictures_asks() {
        let (game, data) = (temp("game"), temp("data"));
        let ask = |g: &Path, automated| {
            decide(&Facts {
                game: g,
                data: &data,
                automated,
            })
        };
        assert_eq!(ask(&game, false), Decision::Offer);
        // A checkout works in one folder; a run for a screenshot has no one to ask.
        assert_eq!(ask(&data, false), Decision::Nothing);
        assert_eq!(ask(&game, true), Decision::Nothing);
        // Pictures there (a link to another folder counts as a folder), or "don't ask again".
        fs::create_dir_all(data.join(ART)).unwrap();
        assert_eq!(ask(&game, false), Decision::Nothing);
        fs::remove_dir_all(data.join("assets-local")).unwrap();
        fs::write(data.join(DECLINED), b"").unwrap();
        assert_eq!(ask(&game, false), Decision::Nothing);
    }

    #[test]
    fn the_setup_is_found_beside_the_game_app_or_in_applications() {
        let dir = temp("find");
        let exe = dir.join("Fantasy4X.app/Contents/MacOS/f4x-client");
        let apps = dir.join("Applications");
        assert_eq!(find_setup(&exe, Os::Mac, &apps), None);
        fs::create_dir_all(apps.join(SETUP_APP)).unwrap();
        assert_eq!(find_setup(&exe, Os::Mac, &apps), Some(apps.join(SETUP_APP)));
        fs::create_dir_all(dir.join(SETUP_APP)).unwrap();
        assert_eq!(find_setup(&exe, Os::Mac, &apps), Some(dir.join(SETUP_APP)));
        // Elsewhere: the program beside the game.
        let program = dir.join("f4x-client");
        assert_eq!(find_setup(&program, Os::Linux, &apps), None);
        fs::write(dir.join(SETUP_PROGRAM), b"").unwrap();
        assert_eq!(
            find_setup(&program, Os::Linux, &apps),
            Some(dir.join(SETUP_PROGRAM))
        );
    }

    #[test]
    fn the_answers_are_read_from_the_buttons() {
        let t = texts();
        assert_eq!(
            parse_answer("button returned:Set up now\n", &t),
            Answer::Setup
        );
        assert_eq!(parse_answer("button returned:Not now", &t), Answer::Later);
        assert_eq!(
            parse_answer("button returned:Don't ask again", &t),
            Answer::Never
        );
        assert_eq!(parse_answer("", &t), Answer::Later);
    }

    #[test]
    fn the_dialog_script_has_the_confirming_button_last_and_escapes_text() {
        let script = dialog_script(&texts());
        assert!(
            script.contains("buttons {\"Don't ask again\", \"Not now\", \"Set up now\"}"),
            "{script}"
        );
        assert!(script.contains("default button \"Set up now\""));
        assert!(!script.contains('\n'), "newlines are written as \\n");
        assert_eq!(script_string("a \"b\" \\ c\nd"), "a \\\"b\\\" \\\\ c\\nd");
    }
}
