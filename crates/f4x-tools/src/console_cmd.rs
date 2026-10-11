//! `f4x-tools console`: play the game from text, without a window. Lines come from standard
//! input (a terminal, a pipe or a script file) and each reply is printed.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use f4x_console::setup::{new_game, NewGame, HUMAN};
use f4x_console::{Access, Console};

const USAGE: &str = "usage: f4x-tools console [--seed N] [--load FILE] [--content DIR] [--access readonly|play|full] [--script FILE]";

pub fn run(args: &[String]) -> Result<(), String> {
    let mut seed = 1u64;
    let mut load: Option<PathBuf> = None;
    let mut content_dir: Option<PathBuf> = None;
    let mut access = Access::Full;
    let mut script: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--seed" => seed = value("--seed")?.parse().map_err(|_| "bad seed")?,
            "--load" => load = Some(value("--load")?.into()),
            "--content" => content_dir = Some(value("--content")?.into()),
            "--script" => script = Some(value("--script")?.into()),
            "--access" => {
                access = match value("--access")?.as_str() {
                    "readonly" => Access::ReadOnly,
                    "play" => Access::Play,
                    "full" => Access::Full,
                    other => return Err(format!("unknown access {other:?}")),
                }
            }
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    let dir = content_dir.unwrap_or_else(|| {
        if Path::new("content-local").exists() {
            "content-local".into()
        } else {
            "content".into()
        }
    });
    let content =
        f4x_data::load_content_layers(&[&dir, Path::new("content")]).map_err(|e| e.to_string())?;
    let mut game = match load {
        Some(path) => {
            let mut g = f4x_data::save::load_game(&path, content).map_err(|e| e.to_string())?;
            g.human.get_or_insert(HUMAN);
            g
        }
        None => new_game(
            &content,
            &NewGame {
                seed,
                ..NewGame::default()
            },
        )?,
    };
    let mut console = Console::new(HUMAN, access);
    let input: Box<dyn BufRead> = match &script {
        Some(path) => Box::new(std::io::BufReader::new(
            std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
        )),
        None => Box::new(std::io::BufReader::new(std::io::stdin())),
    };
    let interactive = script.is_none() && std::io::stdin().is_terminal();
    let mut out = std::io::stdout();
    for line in input.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !interactive {
            println!("> {trimmed}");
        }
        let reply = console.execute(&mut game, trimmed);
        let text = reply.text();
        if !text.is_empty() {
            println!("{text}");
        }
        if console.session.quit {
            break;
        }
        if interactive {
            print!("> ");
            let _ = out.flush();
        }
    }
    Ok(())
}
