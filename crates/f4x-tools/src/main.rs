//! Developer tooling: content validation and map preview.

use std::path::PathBuf;
use std::process::ExitCode;

use f4x_core::content::{TerrainClass, Terrains};
use f4x_core::map::Plane;
use f4x_core::mapgen::{generate, GenParams};

const USAGE: &str = "usage:
  f4x-tools validate [content-dir]            check content files (default: content)
  f4x-tools mapgen <seed> [width height] [land%] [content-dir]
                                              print both planes as ASCII";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate") => validate(args.get(1)),
        Some("mapgen") => mapgen(&args[1..]),
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

fn terrains(dir: Option<&String>) -> Result<Terrains, String> {
    let dir = dir.map_or_else(|| PathBuf::from("content"), PathBuf::from);
    f4x_data::load_terrains(&dir.join("terrain.ron")).map_err(|e| e.to_string())
}

fn validate(dir: Option<&String>) -> Result<(), String> {
    let t = terrains(dir)?;
    println!("terrain.ron: ok ({} definitions)", t.defs().len());
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
    };
    let t = terrains(args.get(4))?;
    let map = generate(seed, &params, &t).map_err(|e| e.to_string())?;
    for plane in Plane::ALL {
        println!("{plane:?}");
        let grid = map.plane(plane);
        for y in 0..grid.height() as i32 {
            let row: String = (0..grid.width() as i32)
                .map(|x| glyph(t.get(grid.get(x, y).expect("in bounds")).map(|d| d.class)))
                .collect();
            println!("{row}");
        }
        println!();
    }
    Ok(())
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
