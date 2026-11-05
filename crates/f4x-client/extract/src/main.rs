//! `f4x-extract`: read the original game's files and write the client's local assets.
//!
//! Tests: in the modules (`art`, `audio`, `image`, `lbx`, `refile`, `screens`); this file only
//! dispatches and prints.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use f4x_extract::{art, audio, font_tool, refile, screens, spell_art, unit_art, wizard_art};

/// The units', spells' and wizards' pictures.
type TableSet = (
    Vec<unit_art::UnitArt>,
    spell_art::SpellArt,
    Vec<wizard_art::WizardArt>,
);

const USAGE: &str = "usage:
  f4x-extract art <game-dir> [out-dir]    decode the original game's pictures to PNG
                                          (default out: assets-local/art)
  f4x-extract audio <game-dir> [out-dir]  the original's effects as OGG (needs oggenc or ffmpeg), its music
                                          as XMI (default out: assets-local/audio)
  f4x-extract refile [out-dir]            move already extracted pictures to where `art` would put them
                                          now (default: assets-local/art)
  f4x-extract font-chars | font-coverage | font-bmfont
                                          the characters the text uses, what a font lacks, a TrueType font
                                          as a BMFont (run one without arguments for its usage; docs/fonts.md)
The tables of which game element uses which asset are read from content/ (spell_art.ron, unit_art.ron).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = |i: usize, default: &str| {
        args.get(i)
            .map_or_else(|| PathBuf::from(default), PathBuf::from)
    };
    let result = match args.first().map(String::as_str) {
        Some("art") => match args.get(1) {
            Some(game) => tables().and_then(|(units, spells, wizards)| {
                art::extract(
                    &PathBuf::from(game),
                    &dir(2, "assets-local/art"),
                    &units,
                    &spells,
                    &wizards,
                )
            }),
            None => Err(USAGE.to_string()),
        },
        Some("audio") => match args.get(1) {
            Some(game) => audio::extract(&PathBuf::from(game), &dir(2, "assets-local/audio")),
            None => Err(USAGE.to_string()),
        },
        Some("refile") => tables().and_then(|(units, spells, wizards)| {
            let placer = screens::Placer::new(screens::rules()?, &units, &spells, &wizards);
            let moved = refile::refile(&dir(1, "assets-local/art"), &placer)?;
            println!("moved {moved} pictures");
            Ok(())
        }),
        Some(command @ ("font-chars" | "font-coverage" | "font-bmfont")) => {
            font_tool::run(command, &args[1..])
        }
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

/// The tables of which game element uses which picture, from `content/`.
fn tables() -> Result<TableSet, String> {
    let content = [Path::new("content")];
    Ok((
        unit_art::load_unit_art(&content)?,
        spell_art::load_spell_art(&content)?,
        wizard_art::load_wizard_art(&content)?,
    ))
}
