//! Developer tooling: content validation and map preview.

mod art;
mod console_cmd;
mod fonts;
mod image;
mod lbx;
mod map_art;
mod soak;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use f4x_core::content::{Content, FeatureKind, TerrainClass};
use f4x_core::map::Plane;
use f4x_core::mapgen::{generate, GenParams};

const USAGE: &str = "usage:
  f4x-tools validate [content-dir]            check content files (default: content)
  f4x-tools mapgen <seed> [width height] [land%] [content-dir]
                                              print both planes as ASCII
  f4x-tools extract-art <game-dir> [out-dir]  decode the original game's pictures to PNG
                                              (default out: assets-local/art)
  f4x-tools console [--seed N] [--load FILE] [--content DIR] [--access readonly|play|full] [--script FILE]
                                              run the game from text commands (stdin, or the script)
  f4x-tools soak [--seeds A..B] [--turns N] [--players 2..5] [--every N] [--refused-log FILE]
                                              AI wizards play whole games and report; with
                                              --refused-log, every refused command goes to FILE (TSV)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate") => validate(args.get(1)),
        Some("mapgen") => mapgen(&args[1..]),
        Some("console") => console_cmd::run(&args[1..]),
        Some("soak") => soak::run(&args[1..]),
        Some("extract-art") => match args.get(1) {
            Some(game) => art::extract(
                &PathBuf::from(game),
                &args
                    .get(2)
                    .map_or_else(|| PathBuf::from("assets-local/art"), PathBuf::from),
            ),
            None => Err(USAGE.to_string()),
        },
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

/// The content in `dir` (default `content`), over the committed `content/` for any file it lacks.
fn content(dir: Option<&String>) -> Result<Content, String> {
    let dir = dir.map_or_else(|| PathBuf::from("content"), PathBuf::from);
    f4x_data::load_content_layers(&[&dir, Path::new("content")]).map_err(|e| e.to_string())
}

fn validate(dir: Option<&String>) -> Result<(), String> {
    let c = content(dir)?;
    let help_dir = dir.map_or_else(|| PathBuf::from("content"), PathBuf::from);
    let help = f4x_data::load_help(&help_dir).map_err(|e| e.to_string())?;
    if !help.topics.is_empty() {
        println!(
            "help: ok ({} topics, {} screens)",
            help.topics.len(),
            help.screens.len()
        );
    }
    println!("terrain.ron: ok ({} definitions)", c.terrains.defs().len());
    println!("features.ron: ok ({} definitions)", c.features.defs().len());
    let r = &c.rules;
    println!(
        "rules: ok ({} races, {} units, {} abilities, {} buildings, {} realms, {} spells, {} items, {} level paths)",
        r.races.len(),
        r.units.len(),
        r.abilities.len(),
        r.buildings.len(),
        r.realms.len(),
        r.spells.len(),
        r.items.len(),
        r.level_paths.len()
    );
    Ok(())
}

fn mapgen(args: &[String]) -> Result<(), String> {
    let num = |i: usize, name: &str| -> Result<Option<u64>, String> {
        args.get(i)
            .map(|s| {
                s.parse()
                    .map_err(|_| format!("{name} must be a number, got {s:?}\n{USAGE}"))
            })
            .transpose()
    };
    let seed = num(0, "seed")?.ok_or(USAGE)?;
    let d = GenParams::default();
    let params = GenParams {
        width: num(1, "width")?.map_or(d.width, |v| v as u32),
        height: num(2, "height")?.map_or(d.height, |v| v as u32),
        land_percent: num(3, "land%")?.map_or(d.land_percent, |v| v.min(255) as u8),
        ..d
    };
    let c = content(args.get(4))?;
    let map = generate(seed, &params, &c).map_err(|e| e.to_string())?;
    for plane in Plane::ALL {
        println!("{plane:?}");
        let grid = map.plane(plane);
        for y in 0..grid.height() as i32 {
            let row: String = (0..grid.width() as i32)
                .map(|x| tile_glyph(&c, grid, x, y))
                .collect();
            println!("{row}");
        }
        println!();
    }
    Ok(())
}

/// Features win over terrain: `T` tower, `N` node, `!` encounter site, `*` mineral.
fn tile_glyph(c: &Content, grid: &f4x_core::map::Grid, x: i32, y: i32) -> char {
    let feature = grid.feature(x, y).and_then(|f| c.features.get(f));
    match feature.map(|f| f.kind) {
        Some(FeatureKind::Tower) => 'T',
        Some(FeatureKind::Node) => 'N',
        Some(FeatureKind::Encounter) => '!',
        Some(FeatureKind::Mineral) => '*',
        None => glyph(
            grid.get(x, y)
                .and_then(|t| c.terrains.get(t))
                .map(|d| d.class),
        ),
    }
}

fn glyph(class: Option<TerrainClass>) -> char {
    match class {
        Some(TerrainClass::Ocean) => '~',
        Some(TerrainClass::Grassland) => '.',
        Some(TerrainClass::Forest) => 'f',
        Some(TerrainClass::Hills) => 'h',
        Some(TerrainClass::Mountains) => '^',
        Some(TerrainClass::Swamp) => 's',
        Some(TerrainClass::Desert) => 'd',
        Some(TerrainClass::Tundra) => 't',
        None => '?',
    }
}
