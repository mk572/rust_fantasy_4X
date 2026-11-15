//! Where the game finds its files (`docs/installer-design.md` 3.5).
//!
//! Two folders: the **game folder** holds the shipped files (`crates/f4x-def/content`,
//! `crates/f4x-lang/content`, `crates/f4x-client/content`), the **data folder** holds what belongs to
//! the player (`assets-local/`, `saves/`). The program changes its working directory to the data
//! folder, so the relative `assets-local/...` and `saves/...` paths keep working, and reads the
//! shipped files through [`game_file`].
//!
//! [`locate`] decides, in this order:
//! 1. a checkout of the repository (the working directory has `crates/f4x-def/content`): both
//!    folders are the working directory, as before. `--data DIR` still moves the data folder;
//! 2. an installed game: the game folder is the one beside the program (macOS: the `Resources` of
//!    the `.app`; Windows and Linux: the folder of the program, then `../share/fantasy4x`), the data
//!    folder is the per-user one (macOS `~/Library/Application Support/Fantasy4X`, Windows
//!    `%APPDATA%\Fantasy4X`, Linux `$XDG_DATA_HOME/fantasy4x` or `~/.local/share/fantasy4x`), or
//!    `--data DIR`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A file of the rules, without which the game does not start; its presence marks a game folder.
const MARKER: &str = "crates/f4x-def/content";

/// The folders the program works with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    /// Where `crates/...` (the shipped files) are.
    pub game: PathBuf,
    /// Where `assets-local/` and `saves/` are.
    pub data: PathBuf,
}

/// The operating systems with a data-folder rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    /// The system this program was built for.
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// The inputs of [`locate`], so a test can give them without touching the real system.
pub struct Inputs<'a> {
    pub os: Os,
    /// The working directory.
    pub cwd: &'a Path,
    /// The path of the running program.
    pub exe: &'a Path,
    /// An environment variable.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// `--data DIR`.
    pub data_override: Option<&'a Path>,
}

/// The per-user data folder of `os`, or `None` when the variables it needs are not set.
pub fn user_data_dir(os: Os, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty());
    match os {
        Os::Mac => Some(PathBuf::from(var("HOME")?).join("Library/Application Support/Fantasy4X")),
        Os::Windows => Some(PathBuf::from(var("APPDATA")?).join("Fantasy4X")),
        Os::Linux => match var("XDG_DATA_HOME") {
            Some(x) => Some(PathBuf::from(x).join("fantasy4x")),
            None => Some(PathBuf::from(var("HOME")?).join(".local/share/fantasy4x")),
        },
    }
}

/// The folders of the shipped files that an installed program has, nearest first.
fn game_candidates(os: Os, exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if os == Os::Mac {
        // Fantasy4X.app/Contents/MacOS/<program> -> Fantasy4X.app/Contents/Resources
        found.push(dir.join("../Resources"));
    }
    found.push(dir.to_path_buf());
    found.push(dir.join("../share/fantasy4x"));
    found
}

/// Decide the two folders (see the module text). Fails with a sentence for the player when no
/// game folder or data folder can be found.
pub fn locate(inputs: &Inputs) -> Result<Roots, String> {
    let has_game = |dir: &Path| dir.join(MARKER).is_dir();
    if has_game(inputs.cwd) {
        let data = inputs
            .data_override
            .map_or_else(|| inputs.cwd.to_path_buf(), Path::to_path_buf);
        return Ok(Roots {
            game: inputs.cwd.to_path_buf(),
            data,
        });
    }
    let game = game_candidates(inputs.os, inputs.exe)
        .into_iter()
        .find(|d| has_game(d))
        .ok_or_else(|| {
            format!(
                "the game's own files ({MARKER}) are not beside the program ({}) or in the current folder",
                inputs.exe.display()
            )
        })?;
    let data = match inputs.data_override {
        Some(d) => d.to_path_buf(),
        None => user_data_dir(inputs.os, inputs.env)
            .ok_or("the folder for your saves and pictures is unknown: set HOME (or APPDATA on Windows), or give --data DIR")?,
    };
    Ok(Roots { game, data })
}

static GAME: OnceLock<PathBuf> = OnceLock::new();

/// Make `game` the folder [`game_file`] reads from. Call once, at start-up; later calls are ignored.
pub fn set_game_dir(game: PathBuf) {
    let _ = GAME.set(game);
}

/// The path of a shipped file (`crates/f4x-def/content`...): under the game folder once
/// [`set_game_dir`] was called, otherwise relative to the working directory.
pub fn game_file(relative: &str) -> PathBuf {
    GAME.get()
        .map_or_else(|| PathBuf::from(relative), |g| g.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    fn game_dir(root: &Path) {
        fs::create_dir_all(root.join(MARKER)).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f4x_paths_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn per_user_folders_of_each_system() {
        let mac = env_of(&[("HOME", "/Users/a")]);
        assert_eq!(
            user_data_dir(Os::Mac, &mac),
            Some(PathBuf::from(
                "/Users/a/Library/Application Support/Fantasy4X"
            ))
        );
        let win = env_of(&[("APPDATA", "C:/Users/a/AppData/Roaming")]);
        assert_eq!(
            user_data_dir(Os::Windows, &win),
            Some(PathBuf::from("C:/Users/a/AppData/Roaming/Fantasy4X"))
        );
        let xdg = env_of(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")]);
        assert_eq!(
            user_data_dir(Os::Linux, &xdg),
            Some(PathBuf::from("/data/fantasy4x"))
        );
        let home = env_of(&[("HOME", "/home/a")]);
        assert_eq!(
            user_data_dir(Os::Linux, &home),
            Some(PathBuf::from("/home/a/.local/share/fantasy4x"))
        );
        assert_eq!(user_data_dir(Os::Mac, &env_of(&[])), None);
        assert_eq!(user_data_dir(Os::Mac, &env_of(&[("HOME", "")])), None);
    }

    #[test]
    fn a_checkout_keeps_the_working_directory() {
        let cwd = temp("checkout");
        game_dir(&cwd);
        let env = env_of(&[("HOME", "/Users/a")]);
        let roots = locate(&Inputs {
            os: Os::Mac,
            cwd: &cwd,
            exe: Path::new("/elsewhere/f4x-client"),
            env: &env,
            data_override: None,
        })
        .unwrap();
        assert_eq!(
            roots,
            Roots {
                game: cwd.clone(),
                data: cwd.clone()
            }
        );
        let moved = locate(&Inputs {
            os: Os::Mac,
            cwd: &cwd,
            exe: Path::new("/elsewhere/f4x-client"),
            env: &env,
            data_override: Some(Path::new("/tmp/mydata")),
        })
        .unwrap();
        assert_eq!(moved.game, cwd);
        assert_eq!(moved.data, PathBuf::from("/tmp/mydata"));
    }

    #[test]
    fn an_app_bundle_uses_resources_and_the_user_folder() {
        let app = temp("bundle").join("Fantasy4X.app/Contents");
        game_dir(&app.join("Resources"));
        fs::create_dir_all(app.join("MacOS")).unwrap();
        let env = env_of(&[("HOME", "/Users/a")]);
        let roots = locate(&Inputs {
            os: Os::Mac,
            cwd: Path::new("/"),
            exe: &app.join("MacOS/f4x-client"),
            env: &env,
            data_override: None,
        })
        .unwrap();
        assert!(
            roots.game.ends_with("MacOS/../Resources"),
            "{:?}",
            roots.game
        );
        assert_eq!(
            roots.data,
            PathBuf::from("/Users/a/Library/Application Support/Fantasy4X")
        );
    }

    #[test]
    fn a_program_folder_is_the_game_folder_on_windows_and_linux() {
        let dir = temp("beside");
        game_dir(&dir);
        let env = env_of(&[("APPDATA", "C:/a")]);
        let roots = locate(&Inputs {
            os: Os::Windows,
            cwd: Path::new("/"),
            exe: &dir.join("f4x-client.exe"),
            env: &env,
            data_override: Some(Path::new("D:/saves")),
        })
        .unwrap();
        assert_eq!(roots.game, dir);
        assert_eq!(roots.data, PathBuf::from("D:/saves"));
    }

    #[test]
    fn missing_game_files_and_missing_home_say_what_is_wrong() {
        let empty = temp("empty");
        let env = env_of(&[("HOME", "/Users/a")]);
        let err = locate(&Inputs {
            os: Os::Linux,
            cwd: &empty,
            exe: &empty.join("f4x-client"),
            env: &env,
            data_override: None,
        })
        .unwrap_err();
        assert!(err.contains("crates/f4x-def/content"), "{err}");
        let dir = temp("nohome");
        game_dir(&dir);
        let err = locate(&Inputs {
            os: Os::Linux,
            cwd: Path::new("/"),
            exe: &dir.join("f4x-client"),
            env: &env_of(&[]),
            data_override: None,
        })
        .unwrap_err();
        assert!(err.contains("--data"), "{err}");
    }
}
