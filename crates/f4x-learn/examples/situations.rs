//! Fights the scripted battle AI (`Game::next_battle_command`, both sides) in the situations the
//! generated scenarios do not hold: each terrain class, each kind of site (lair, tower, node),
//! with and without a hero carrying an item. Prints one line for each situation and flags the
//! battles that were refused a command, panicked, stalled or hit the command limit.
//!
//! `cargo run --release -p f4x-learn --example situations -- [--seeds N] [--content DIR]`

use f4x_core::battle::{BattleCommand, BattleEvent};
use f4x_core::game::{Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_learn::build::{build_at, Spot};
use f4x_learn::scenario::generate_raw;

const LIMIT: u32 = 5_000;

#[derive(Default)]
struct Tally {
    fights: u32,
    no_square: u32,
    ended: u32,
    attacker_wins: u32,
    commands: u64,
    longest: u32,
    rounds: u64,
    casts: u32,
    unit_casts: u32,
    refused: Vec<String>,
    stalled: u32,
    panicked: u32,
}

fn fight(game: &mut Game, tally: &mut Tally) {
    let mut steps = 0u32;
    while game.in_battle() && steps < LIMIT {
        let Some(command) = game.next_battle_command() else {
            break;
        };
        if matches!(command, BattleCommand::EndTurn) {
            tally.rounds += 1;
        }
        if matches!(command, BattleCommand::UnitCast { .. }) {
            tally.unit_casts += 1;
        }
        match game.battle_command(command.clone()) {
            Ok(events) => {
                for e in &events {
                    match e {
                        Event::Battle(BattleEvent::SpellCast { .. }) => tally.casts += 1,
                        Event::Battle(BattleEvent::Ended { winner }) => {
                            tally.ended += 1;
                            if *winner == f4x_core::battle::Side::Attacker {
                                tally.attacker_wins += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tally.refused.push(format!("{command:?}: {e:?}"));
                break;
            }
        }
        steps += 1;
    }
    tally.commands += u64::from(steps);
    tally.longest = tally.longest.max(steps);
    if game.in_battle() && tally.refused.is_empty() {
        tally.stalled += 1;
    }
}

fn main() {
    let mut seeds = 12u64;
    let mut dir = String::from("content");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seeds" => seeds = args.next().and_then(|v| v.parse().ok()).expect("--seeds N"),
            "--content" => dir = args.next().expect("--content DIR"),
            "-h" | "--help" => {
                println!("usage: situations [--seeds N] [--content DIR]");
                return;
            }
            other => {
                eprintln!("unknown option {other:?}");
                std::process::exit(2);
            }
        }
    }
    let content = f4x_data::load_content_layers(&[
        std::path::Path::new(&dir),
        std::path::Path::new("content"),
    ])
    .expect("content loads");
    let classes = [
        ("grassland", TerrainClass::Grassland),
        ("forest", TerrainClass::Forest),
        ("hills", TerrainClass::Hills),
        ("mountains", TerrainClass::Mountains),
        ("swamp", TerrainClass::Swamp),
        ("desert", TerrainClass::Desert),
        ("tundra", TerrainClass::Tundra),
    ];
    let mut spots: Vec<(String, Spot)> = classes
        .iter()
        .map(|(n, c)| ((*n).to_string(), Spot::Class(*c)))
        .collect();
    for site in [
        "tower",
        "chaos_node",
        "nature_node",
        "sorcery_node",
        "mysterious_cave",
        "monster_lair",
        "dungeon",
        "ruins",
        "ancient_temple",
        "abandoned_keep",
        "fallen_temple",
    ] {
        spots.push((format!("site {site}"), Spot::Site(site.to_string())));
    }
    let mut bad = 0;
    println!(
        "{:<26} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6}  problems",
        "situation", "hero", "fight", "ended", "atk%", "cmds", "rounds", "casts", "ucasts"
    );
    for (name, spot) in &spots {
        for hero in [false, true] {
            let mut tally = Tally::default();
            for seed in 0..seeds {
                let scenario = generate_raw(&content, seed);
                let Ok(mut game) = build_at(&content, &scenario, spot, seed as usize, hero) else {
                    tally.no_square += 1;
                    continue;
                };
                tally.fights += 1;
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fight(&mut game, &mut tally)
                }));
                if caught.is_err() {
                    tally.panicked += 1;
                }
            }
            let mut problems = Vec::new();
            if tally.panicked > 0 {
                problems.push(format!("{} panics", tally.panicked));
            }
            if tally.stalled > 0 {
                problems.push(format!("{} stalled", tally.stalled));
            }
            if let Some(r) = tally.refused.first() {
                problems.push(format!("{} refused, e.g. {r}", tally.refused.len()));
            }
            if tally.fights == 0 {
                problems.push("no fight built".into());
            }
            bad += problems.len();
            let n = tally.fights.max(1);
            println!(
                "{:<26} {:>5} {:>5} {:>5} {:>5}% {:>6} {:>6} {:>6.1} {:>6.1}  {}",
                name,
                if hero { "yes" } else { "no" },
                tally.fights,
                tally.ended,
                100 * tally.attacker_wins / n,
                tally.commands / u64::from(n),
                tally.rounds / u64::from(n),
                f64::from(tally.casts) / f64::from(n),
                f64::from(tally.unit_casts) / f64::from(n),
                problems.join("; ")
            );
        }
    }
    println!("{bad} problem(s)");
}
