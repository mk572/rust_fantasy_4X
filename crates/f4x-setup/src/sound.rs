//! The sound effects and the music, into `assets-local/audio/` in the data folder.
//!
//! The effects are decoded and encoded to OGG here (`f4x_extract::audio`). The music is rendered by
//! the separate program `f4x-music` (LGPL, linked with the MT-32 emulation: `docs/installer-design.md`
//! 3.3), which this module starts as a child process and follows by the progress lines it prints.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use f4x_extract::audio;

use crate::install::{checked_assets, InstallError};

/// The sound folder inside `assets-local/`.
const AUDIO: &str = "audio";
/// Where the work is done before it replaces the old files.
const STAGING: &str = ".audio.new";
/// The archive of the songs the game plays.
const SONGS: &str = "MUSIC";
/// The name of the music program.
pub const MUSIC_TOOL: &str = "f4x-music";

/// Replace `target` by the folder `new` (the old one is removed first).
fn replace(new: &Path, target: &Path) -> Result<(), InstallError> {
    if target.exists() {
        fs::remove_dir_all(target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    fs::create_dir_all(target.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("{}: {e}", target.display()))?;
    fs::rename(new, target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(())
}

fn fresh(dir: &Path) -> Result<(), InstallError> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(())
}

/// Extract the effects the game plays into `data/assets-local/audio/<archive>/`. Returns how many.
pub fn install_effects(original: &Path, data: &Path) -> Result<usize, InstallError> {
    let assets = checked_assets(data)?;
    let staging = data.join(STAGING);
    fresh(&staging)?;
    let report = audio::extract_effects(original, &staging, &audio::PLAYED_EFFECTS, None)?;
    for stem in audio::PLAYED_EFFECTS {
        let folder = stem.to_lowercase();
        if staging.join(&folder).is_dir() {
            replace(&staging.join(&folder), &assets.join(AUDIO).join(&folder))?;
        }
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(report.written)
}

/// The music program: beside the running program (an installed setup), or built in the checkout.
pub fn locate_music_tool(exe: &Path, game: &Path) -> Result<PathBuf, String> {
    let candidates = [
        exe.parent().map(|d| d.join(MUSIC_TOOL)),
        Some(game.join("tools/xmi-play/target/release").join(MUSIC_TOOL)),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .ok_or_else(|| format!("the music program {MUSIC_TOOL} was not found beside the setup"))
}

/// Run the music program on `songs`, writing OGG files to `out`. `progress(done, total)` is called
/// for each finished song. The program's own problem text is the error.
pub fn run_music_tool(
    tool: &Path,
    songs: &Path,
    out: &Path,
    roms: &Path,
    timbres: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(), InstallError> {
    let mut child = Command::new(tool)
        .arg("--songs")
        .arg(songs)
        .arg("--out")
        .arg(out)
        .arg("--roms")
        .arg(roms)
        .arg("--timbres")
        .arg(timbres)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", tool.display()))?;
    let stderr = child.stderr.take();
    let reader = thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((done, total)) = parse_progress(&line) {
                progress(done, total);
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let problem = reader.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        let first = problem.lines().next().unwrap_or("").trim();
        Err(InstallError::Failed(if first.is_empty() {
            format!("{MUSIC_TOOL} stopped with {status}")
        } else {
            first.to_string()
        }))
    }
}

/// `rendered 12/116 name` gives `(12, 116)`.
fn parse_progress(line: &str) -> Option<(usize, usize)> {
    let counts = line.strip_prefix("rendered ")?.split_whitespace().next()?;
    let (done, total) = counts.split_once('/')?;
    Some((done.parse().ok()?, total.parse().ok()?))
}

/// Make the music: extract the songs and the timbre library of the original, render them with the
/// ROMs in `roms` and put the OGG files in `data/assets-local/audio/music/`. Returns how many songs.
pub fn install_music(
    original: &Path,
    data: &Path,
    roms: &Path,
    tool: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<usize, InstallError> {
    let assets = checked_assets(data)?;
    let staging = data.join(STAGING);
    fresh(&staging)?;
    let songs = staging.join("songs");
    let report = audio::extract_songs(original, SONGS, &songs, None)?;
    if report.written == 0 {
        return Err(InstallError::Failed(format!(
            "no songs found in {SONGS}.LBX"
        )));
    }
    if audio::extract_timbres(original, &staging.join("timbres"))? == 0 {
        return Err(InstallError::Failed(
            "the timbre files (FAT.MT) were not found".into(),
        ));
    }
    let rendered = staging.join("music");
    run_music_tool(
        tool,
        &songs,
        &rendered,
        roms,
        &staging.join("timbres/fat.mt"),
        progress,
    )?;
    replace(&rendered, &assets.join(AUDIO).join("music"))?;
    let _ = fs::remove_dir_all(&staging);
    Ok(report.written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("f4x_sound_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A shell script standing in for the music program.
    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake-music");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn progress_lines_are_read_and_others_ignored() {
        assert_eq!(
            parse_progress("rendered 12/116 104_main_menu"),
            Some((12, 116))
        );
        assert_eq!(parse_progress("done 116"), None);
        assert_eq!(parse_progress("rendered x/y z"), None);
        assert_eq!(parse_progress(""), None);
    }

    #[test]
    fn the_tool_is_followed_by_its_progress_lines() {
        let dir = temp("follow");
        let tool = script(
            &dir,
            "echo 'rendered 1/2 a'; echo 'rendered 2/2 b'; echo 'done 2'",
        );
        let mut seen = Vec::new();
        run_music_tool(&tool, &dir, &dir, &dir, &dir, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(seen, [(1, 2), (2, 2)]);
    }

    #[test]
    fn the_tools_own_problem_is_the_error() {
        let dir = temp("fail");
        let tool = script(&dir, "echo 'a.xmi: the ROMs are not usable' >&2; exit 1");
        let error = run_music_tool(&tool, &dir, &dir, &dir, &dir, &mut |_, _| {}).unwrap_err();
        assert_eq!(
            error,
            InstallError::Failed("a.xmi: the ROMs are not usable".into())
        );
        let silent = script(&dir, "exit 3");
        let error = run_music_tool(&silent, &dir, &dir, &dir, &dir, &mut |_, _| {}).unwrap_err();
        let InstallError::Failed(text) = error else {
            panic!()
        };
        assert!(text.contains("stopped with"), "{text}");
        let missing = run_music_tool(&dir.join("none"), &dir, &dir, &dir, &dir, &mut |_, _| {});
        assert!(matches!(missing, Err(InstallError::Failed(_))));
    }

    #[test]
    fn the_tool_is_found_beside_the_program_or_in_the_checkout() {
        let dir = temp("locate");
        let exe = dir.join("f4x-setup");
        assert!(locate_music_tool(&exe, &dir).is_err());
        let built = dir.join("tools/xmi-play/target/release");
        fs::create_dir_all(&built).unwrap();
        fs::write(built.join(MUSIC_TOOL), b"").unwrap();
        assert_eq!(
            locate_music_tool(&exe, &dir).unwrap(),
            built.join(MUSIC_TOOL)
        );
        fs::write(dir.join(MUSIC_TOOL), b"").unwrap();
        assert_eq!(locate_music_tool(&exe, &dir).unwrap(), dir.join(MUSIC_TOOL));
    }

    #[test]
    fn nothing_is_written_through_a_link() {
        let data = temp("link");
        fs::create_dir_all(data.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(data.join("elsewhere"), data.join("assets-local")).unwrap();
        let error = install_effects(Path::new("/nowhere"), &data).unwrap_err();
        assert!(matches!(error, InstallError::Link { .. }), "{error:?}");
        assert!(data.join("elsewhere").read_dir().unwrap().next().is_none());
    }
}
