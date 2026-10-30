//! Developer tooling: content validation and map preview.

mod art;
mod audio;
mod cache;
mod console_cmd;
mod contestant;
mod fonts;
mod image;
mod lbx;
mod map_art;
mod metrics;
mod refile;
mod screens;
mod soak;
mod versus;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use f4x_core::content::Content;
use f4x_core::feature::def::FeatureKind;
use f4x_core::map::mapgen::{generate, GenParams};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Plane;

const USAGE: &str = "usage:
  f4x-tools validate [content-dir]            check content files (default: content)
  f4x-tools mapgen <seed> [width height] [land%] [content-dir]
                                              print both planes as ASCII
  f4x-tools mapgen-stats [runs] [content-dir] count tiles, rivers, lakes, landmasses and features over
                                              many maps (seeds 1..=runs, default 100, standard size) made
                                              by our generator and by the original's, as a table
  f4x-tools start-stats [runs] [content-dir]  neutral cities and fortress distances of whole new games
                                              (seeds 1..=runs, default 100, 3 wizards, standard size)
                                              with our rules and with the original's, as a table
  f4x-tools extract-art <game-dir> [out-dir]  decode the original game's pictures to PNG
                                              (default out: assets-local/art)
  f4x-tools extract-audio <game-dir> [out-dir]
                                              the original's effects as OGG (needs oggenc or ffmpeg), its music as XMI
                                              (default out: assets-local/audio)
  f4x-tools refile-art [out-dir]              move already extracted pictures to where extract-art
                                              would put them now (default: assets-local/art)
  f4x-tools console [--seed N] [--load FILE] [--content DIR] [--access readonly|play|full] [--script FILE]
                                              run the game from text commands (stdin, or the script)
  f4x-tools soak [--seeds A..B] [--turns N] [--players 2..5] [--every N] [--refused-log FILE]
                                              AI wizards play whole games and report; with
                                              --refused-log, every refused command goes to FILE (TSV);
                                              --snapshots-every N --out DIR saves the game every N turns
  f4x-tools soak-versus [--seeds A..B] [--turns N] [--players 2..5] [--snapshots-every N] [--out DIR]
                                              the AI under work against the frozen reference (scripts/fork_ai.py):
                                              four games a seed (ref only, new in each half of the seats, new only)
  f4x-tools soak-diff SNAPSHOT_A SNAPSHOT_B   show how two saved games differ";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("validate") => validate(args.get(1)),
        Some("mapgen") => mapgen(&args[1..]),
        Some("mapgen-stats") => mapgen_stats(&args[1..]),
        Some("start-stats") => start_stats(&args[1..]),
        Some("console") => console_cmd::run(&args[1..]),
        Some("soak") => soak::run(&args[1..]),
        Some("soak-versus") => versus::run(&args[1..]),
        Some("soak-diff") => versus::diff(&args[1..]),
        Some("extract-art") => match args.get(1) {
            Some(game) => unit_art().and_then(|units| {
                art::extract(
                    &PathBuf::from(game),
                    &args
                        .get(2)
                        .map_or_else(|| PathBuf::from("assets-local/art"), PathBuf::from),
                    &units,
                    &spell_art()?,
                )
            }),
            None => Err(USAGE.to_string()),
        },
        Some("extract-audio") => match args.get(1) {
            Some(game) => audio::extract(
                &PathBuf::from(game),
                &args
                    .get(2)
                    .map_or_else(|| PathBuf::from("assets-local/audio"), PathBuf::from),
            ),
            None => Err(USAGE.to_string()),
        },
        Some("refile-art") => spell_art().and_then(|spell_art| {
            let art_dir = args
                .get(1)
                .map_or_else(|| PathBuf::from("assets-local/art"), PathBuf::from);
            let placer = screens::Placer::new(screens::rules()?, &unit_art()?, &spell_art);
            let moved = refile::refile(&art_dir, &placer)?;
            println!("moved {moved} pictures");
            Ok(())
        }),
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

/// The units' ids and original picture numbers, for filing their art under their names.
fn unit_art() -> Result<Vec<screens::UnitArt>, String> {
    let c = content(None)?;
    Ok(c.rules
        .units
        .defs()
        .iter()
        .map(|u| screens::UnitArt {
            id: u.id.clone(),
            art: u
                .art
                .as_ref()
                .map(|(lbx, e)| (lbx.clone(), usize::from(*e))),
            portrait: u.hero.as_ref().map(|h| h.portrait as usize),
        })
        .collect())
}

/// The table of which pictures belong to which spell, checked against the rules.
fn spell_art() -> Result<f4x_data::spell_art::SpellArt, String> {
    let c = content(None)?;
    f4x_data::spell_art::load_spell_art(&[Path::new("content")], &c.rules)
        .map_err(|e| e.to_string())
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
    let generated = generate(seed, &params, &c).map_err(|e| e.to_string())?;
    let (map, placements) = (generated.map, generated.placements);
    for plane in Plane::ALL {
        println!("{plane:?}");
        let grid = map.plane(plane);
        for y in 0..grid.height() as i32 {
            let row: String = (0..grid.width() as i32)
                .map(|x| tile_glyph(&c, grid, &placements, plane, x, y))
                .collect();
            println!("{row}");
        }
        println!();
    }
    Ok(())
}

/// Counts of one generated map, summed over both planes.
fn map_counts(c: &Content, g: &f4x_core::map::mapgen::Generated) -> BTreeMap<String, f64> {
    let mut n: BTreeMap<String, f64> = BTreeMap::new();
    let mut add = |k: String, v: f64| *n.entry(k).or_default() += v;
    for plane in Plane::ALL {
        let grid = g.map.plane(plane);
        let (w, h) = (grid.width() as i32, grid.height() as i32);
        let land = |x: i32, y: i32| {
            grid.get(x, y)
                .and_then(|t| c.terrains.get(t))
                .is_some_and(|d| d.class != TerrainClass::Ocean)
        };
        for y in 0..h {
            for x in 0..w {
                let class = grid
                    .get(x, y)
                    .and_then(|t| c.terrains.get(t))
                    .map(|d| d.class);
                if let Some(class) = class {
                    add(format!("tiles: {class:?}"), 1.0);
                }
                if let Some(m) = grid.mineral(x, y).and_then(|f| c.features.get(f)) {
                    add(format!("mineral: {}", m.name), 1.0);
                }
                add("rivers".into(), f64::from(u8::from(grid.has_river(x, y))));
                add("lakes".into(), f64::from(u8::from(grid.is_lake(x, y))));
            }
        }
        // Landmasses: 8-connected land, x wrapping.
        let mut seen = vec![false; (w * h) as usize];
        let (mut count, mut largest, mut total) = (0.0, 0.0f64, 0.0);
        for y in 0..h {
            for x in 0..w {
                if seen[(y * w + x) as usize] || !land(x, y) {
                    continue;
                }
                count += 1.0;
                let (mut size, mut stack) = (0.0, vec![(x, y)]);
                seen[(y * w + x) as usize] = true;
                while let Some((cx, cy)) = stack.pop() {
                    size += 1.0;
                    for (dx, dy) in (-1..=1).flat_map(|a| (-1..=1).map(move |b| (a, b))) {
                        let (nx, ny) = ((cx + dx).rem_euclid(w), cy + dy);
                        if (0..h).contains(&ny) && land(nx, ny) && !seen[(ny * w + nx) as usize] {
                            seen[(ny * w + nx) as usize] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                largest = largest.max(size);
                total += size;
            }
        }
        add("landmasses".into(), count);
        add(
            "largest landmass, % of land (mean of planes)".into(),
            50.0 * largest / total.max(1.0),
        );
    }
    for p in &g.placements {
        if let Some(d) = c.features.get(p.feature) {
            add(format!("{:?}: {}", d.kind, d.name), 1.0);
        }
    }
    n
}

/// Both generators over seeds `1..=runs`, standard size, as a markdown table of per-map means.
fn mapgen_stats(args: &[String]) -> Result<(), String> {
    let runs: u64 = match args.first() {
        Some(s) => s
            .parse()
            .map_err(|_| format!("runs must be a number\n{USAGE}"))?,
        None => 100,
    };
    let c = content(args.get(1))?;
    let mut means = Vec::new();
    for original_generator in [false, true] {
        let params = GenParams {
            original_generator,
            ..GenParams::default()
        };
        let mut sum: BTreeMap<String, f64> = BTreeMap::new();
        for seed in 1..=runs {
            let g = generate(seed, &params, &c).map_err(|e| e.to_string())?;
            for (k, v) in map_counts(&c, &g) {
                *sum.entry(k).or_default() += v / runs as f64;
            }
        }
        means.push(sum);
    }
    println!("Means per map (both planes, {runs} maps of 60 by 40 each; ours = noise, original = random walks)\n");
    println!("| count | ours | original | ours vs original |");
    println!("|---|---:|---:|---:|");
    let keys: std::collections::BTreeSet<_> =
        means.iter().flat_map(|m| m.keys().cloned()).collect();
    for k in keys {
        let (a, b) = (
            means[0].get(&k).copied().unwrap_or(0.0),
            means[1].get(&k).copied().unwrap_or(0.0),
        );
        let diff = if b == 0.0 {
            "n/a".to_string()
        } else {
            format!("{:+.0}%", 100.0 * (a - b) / b)
        };
        println!("| {k} | {a:.1} | {b:.1} | {diff} |");
    }
    Ok(())
}

/// New games with `original_map_generator` off and on: the neutral cities each plane gets (the
/// original's target is 15) and how far apart the wizards' fortresses stand.
fn start_stats(args: &[String]) -> Result<(), String> {
    use f4x_core::game::setup::{new_game, NewGame};
    let runs: u64 = match args.first() {
        Some(s) => s
            .parse()
            .map_err(|_| format!("runs must be a number\n{USAGE}"))?,
        None => 100,
    };
    let c = content(args.get(1))?;
    println!("{runs} games of 3 wizards on 60 by 40 maps\n");
    println!("| | ours | original |");
    println!("|---|---:|---:|");
    let mut rows: Vec<[f64; 2]> = vec![[0.0; 2]; 6];
    for (col, original) in [false, true].into_iter().enumerate() {
        for seed in 1..=runs {
            let mut spec = NewGame::with_wizards(3);
            spec.seed = seed;
            spec.settings.basic.original_map_generator = original;
            let g = new_game(&c, &spec)?;
            let cities = g.economy().cities();
            let mut per_plane = [0.0; 2];
            let mut forts = Vec::new();
            for (_, city, at) in &cities {
                if g.is_neutral_player(city.owner) {
                    per_plane[at.plane.index()] += 1.0;
                } else {
                    forts.push(*at);
                }
            }
            let mut nearest = f64::MAX;
            for (i, a) in forts.iter().enumerate() {
                for b in &forts[i + 1..] {
                    if a.plane == b.plane {
                        let dx = a.x.abs_diff(b.x);
                        let d = dx.min(60 - dx).max(a.y.abs_diff(b.y));
                        nearest = nearest.min(f64::from(d));
                    }
                }
            }
            let n = runs as f64;
            rows[0][col] += per_plane[0] / n;
            rows[1][col] += per_plane[1] / n;
            rows[2][col] += f64::from(u8::from(per_plane[0] >= 15.0)) * 100.0 / n;
            rows[3][col] += f64::from(u8::from(per_plane[1] >= 15.0)) * 100.0 / n;
            if nearest < f64::MAX {
                rows[4][col] += nearest / n;
            }
            rows[5][col] += f64::from(u8::from(nearest < 10.0)) * 100.0 / n;
        }
    }
    let names = [
        "neutral cities, Arcanus (target 15)",
        "neutral cities, Myrror (target 15)",
        "% of games with all 15 on Arcanus",
        "% of games with all 15 on Myrror",
        "nearest two fortresses on a plane (squares)",
        "% of games with fortresses under 10 apart",
    ];
    for (name, r) in names.iter().zip(&rows) {
        println!("| {name} | {:.1} | {:.1} |", r[0], r[1]);
    }
    Ok(())
}

/// Features win over terrain: `T` tower, `N` node, `!` encounter site, `*` mineral.
fn tile_glyph(
    c: &Content,
    grid: &f4x_core::map::Grid,
    placements: &[f4x_core::feature::entity::Placement],
    plane: Plane,
    x: i32,
    y: i32,
) -> char {
    // A tower stands on both planes, though it is placed on Arcanus.
    let placed = placements.iter().find(|p| {
        let tower = c
            .features
            .get(p.feature)
            .is_some_and(|d| d.kind == FeatureKind::Tower);
        (p.at.plane == plane || tower) && p.at.x as i32 == x && p.at.y as i32 == y
    });
    let feature = placed
        .map(|p| p.feature)
        .or_else(|| grid.mineral(x, y))
        .and_then(|f| c.features.get(f));
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
        Some(TerrainClass::SorceryNode | TerrainClass::NatureNode | TerrainClass::ChaosNode) => 'N',
        None => '?',
    }
}
