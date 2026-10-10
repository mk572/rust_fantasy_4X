//! Developer tooling: content validation, content listings and map preview.

use std::path::PathBuf;
use std::process::ExitCode;

use f4x_core::content::{Content, FeatureKind, TerrainClass};
use f4x_core::map::Plane;
use f4x_core::mapgen::{generate, GenParams};
use f4x_core::rules::Stat;

const USAGE: &str = "usage:
  f4x-tools validate [pack-dir...]          check content packs, in load order
  f4x-tools list <kind> [pack-dir...]       kind: units, buildings, spells, items, races, abilities
  f4x-tools mapgen <seed> [width height land%] [pack-dir...]
                                            print both planes as ASCII
Pack directories default to content/base; later packs override earlier ones.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate") => validate(&args[1..]),
        Some("list") => list(&args[1..]),
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

fn content(dirs: &[String]) -> Result<Content, String> {
    let dirs: Vec<PathBuf> = if dirs.is_empty() {
        vec![PathBuf::from("content/base")]
    } else {
        dirs.iter().map(PathBuf::from).collect()
    };
    f4x_data::load_packs(&dirs).map_err(|e| e.to_string())
}

fn validate(dirs: &[String]) -> Result<(), String> {
    let c = content(dirs)?;
    for p in &c.packs {
        println!("pack {} {} ({}): ok", p.id, p.version, p.dir);
    }
    println!(
        "terrain: {}, features: {}",
        c.terrains.defs().len(),
        c.features.defs().len()
    );
    let r = &c.rules;
    println!(
        "rules: {} races, {} units, {} abilities, {} buildings, {} realms, {} spells, {} items, {} level paths",
        r.races.len(),
        r.units.len(),
        r.abilities.len(),
        r.buildings.len(),
        r.realms.len(),
        r.spells.len(),
        r.items.len(),
        r.level_paths.len()
    );
    let uncovered = f4x_data::defs_without_art(&c);
    println!(
        "art: {} entries; {} definitions use the default placeholder",
        c.art.len(),
        uncovered.len()
    );
    for m in f4x_data::missing_art_files(&c) {
        println!("warning: missing art file {m}");
    }
    Ok(())
}

fn list(args: &[String]) -> Result<(), String> {
    let kind = args.first().ok_or(USAGE)?;
    let c = content(&args[1..])?;
    let r = &c.rules;
    let stat = |u: &f4x_core::rules::UnitDef, s: Stat| u.stats.get(&s).copied().unwrap_or(0);
    match kind.as_str() {
        "units" => {
            println!(
                "{:<14} {:<10} {:<9} {:>4} {:>3} {:>3} {:>3} {:>3} {:>3} {:>4} {:>3}",
                "id", "race", "kind", "cost", "Mel", "Rng", "Def", "Res", "HP", "Fig", "Mov"
            );
            for u in r.units.defs() {
                println!(
                    "{:<14} {:<10} {:<9} {:>4} {:>3} {:>3} {:>3} {:>3} {:>3} {:>4} {:>3}",
                    u.id,
                    u.race.as_deref().unwrap_or("-"),
                    format!("{:?}", u.kind),
                    u.cost,
                    stat(u, Stat::Melee),
                    stat(u, Stat::Ranged),
                    stat(u, Stat::Defense),
                    stat(u, Stat::Resistance),
                    stat(u, Stat::MaxHp),
                    stat(u, Stat::Figures),
                    stat(u, Stat::Moves)
                );
            }
        }
        "buildings" => {
            println!(
                "{:<12} {:>5} {:>6}  {:<22} races",
                "id", "cost", "upkeep", "requires"
            );
            for b in r.buildings.defs() {
                let races = if b.races.is_empty() {
                    "all".to_string()
                } else {
                    b.races.join(",")
                };
                println!(
                    "{:<12} {:>5} {:>6}  {:<22} {}",
                    b.id,
                    b.cost,
                    b.upkeep_gold,
                    b.requires.join(","),
                    races
                );
            }
        }
        "spells" => {
            println!(
                "{:<20} {:<8} {:<9} {:>4} {:>8}",
                "id", "realm", "rarity", "cost", "research"
            );
            for s in r.spells.defs() {
                println!(
                    "{:<20} {:<8} {:<9} {:>4} {:>8}",
                    s.id,
                    s.realm,
                    format!("{:?}", s.rarity),
                    s.cost,
                    s.research_cost
                );
            }
        }
        "items" => r
            .items
            .defs()
            .iter()
            .for_each(|i| println!("{:<20} {:?} cost {}", i.id, i.slot, i.cost)),
        "races" => r
            .races
            .defs()
            .iter()
            .for_each(|x| println!("{:<12} {}", x.id, x.name)),
        "abilities" => r
            .abilities
            .defs()
            .iter()
            .for_each(|x| println!("{:<16} {}", x.id, x.description)),
        other => return Err(format!("unknown kind {other:?}\n{USAGE}")),
    }
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
    let c = content(args.get(4..).unwrap_or(&[]))?;
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
