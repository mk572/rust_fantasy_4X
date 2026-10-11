//! `f4x-tools soak`: let AI wizards play whole games against each other, with no window, and
//! report how each game went. Used to find AI and rules bugs (task 77).

use std::path::{Path, PathBuf};
use std::time::Instant;

use f4x_ai::{play_round, Ai};
use f4x_core::city::PlayerId;
use f4x_core::setup::{new_game, NewGame};
use f4x_core::Game;

pub const USAGE: &str = "usage: f4x-tools soak [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--trace TURNS]";

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut first, mut last) = (1u64, 3u64);
    let (mut turns, mut players, mut every) = (200u32, 2u8, 0u32);
    let mut trace = 0u32;
    let mut dir: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--seeds" => {
                let v = value("--seeds")?;
                let (a, b) = v.split_once("..").unwrap_or((&v, &v));
                first = a.parse().map_err(|_| "bad seed range")?;
                last = b.parse().map_err(|_| "bad seed range")?;
            }
            "--turns" => turns = value("--turns")?.parse().map_err(|_| "bad turns")?,
            "--players" => players = value("--players")?.parse().map_err(|_| "bad players")?,
            "--trace" => trace = value("--trace")?.parse().map_err(|_| "bad turns")?,
            "--every" => every = value("--every")?.parse().map_err(|_| "bad interval")?,
            "--content" => dir = Some(value("--content")?.into()),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    let dir = dir.unwrap_or_else(|| {
        if Path::new("content-local").exists() {
            "content-local".into()
        } else {
            "content".into()
        }
    });
    let content =
        f4x_data::load_content_layers(&[&dir, Path::new("content")]).map_err(|e| e.to_string())?;
    let mut summary = Vec::new();
    for seed in first..=last {
        let mut game = new_game(
            &content,
            &NewGame {
                seed,
                wizards: players,
                all_ai: true,
                ..NewGame::default()
            },
        )?;
        game.set_undo(false);
        let mut ais: Vec<Ai> = (0..players).map(|k| Ai::new(PlayerId(k))).collect();
        let started = Instant::now();
        let mut rejected = 0usize;
        let (mut battles, mut captures) = (0usize, 0usize);
        while game.turn <= turns && game.game_over.is_none() {
            let events = play_round(&mut game, &mut ais);
            if game.turn <= trace && trace > 0 {
                for ai in &ais {
                    println!(
                        "  [t{}] ai {} campaign {:?}",
                        game.turn,
                        ai.player.0,
                        ai.campaign()
                    );
                }
                for (c, city, _) in game.cities() {
                    let prod = game.city_production(c);
                    let item = prod.item.map(|i| f4x_console::describe::item(&game, i));
                    let eco = game.city_economy(c);
                    println!(
                        "  [t{}] {} (p{}): {:?} {}/{} eco {:?}",
                        game.turn,
                        city.name,
                        city.owner.0,
                        item,
                        prod.progress,
                        prod.item.map_or(0, |i| game.item_cost(c, i)),
                        eco.map(|e| (e.figures, e.farmers, e.production, e.food))
                    );
                }
                for e in &events {
                    println!(
                        "  [t{}] {}",
                        game.turn,
                        f4x_console::describe::event(&game, e)
                    );
                }
            }
            battles += events
                .iter()
                .filter(|e| matches!(e, f4x_core::Event::BattleEnded { .. }))
                .count();
            captures += events
                .iter()
                .filter(|e| matches!(e, f4x_core::Event::CityCaptured { .. }))
                .count();
            rejected += events
                .iter()
                .filter(|e| matches!(e, f4x_core::Event::Rejected(_)))
                .count();
            if every > 0 && game.turn % every == 0 {
                println!("seed {seed} turn {}: {}", game.turn, status(&game));
            }
        }
        let outcome = match game.game_over {
            Some(o) => format!(
                "won by player {} ({:?}) on turn {}",
                o.winner.0, o.kind, o.turn
            ),
            None => format!("no winner after {} turns", game.turn - 1),
        };
        println!(
            "seed {seed}: {outcome}; {}; {battles} battles, {captures} cities taken, {rejected} refused orders; {:.1}s",
            status(&game),
            started.elapsed().as_secs_f32()
        );
        summary.push(game.game_over.is_some());
    }
    println!(
        "{} of {} games ended with a winner",
        summary.iter().filter(|&&w| w).count(),
        summary.len()
    );
    Ok(())
}

/// Cities and units of each wizard, e.g. `p0 5c/14u, p1 2c/6u`.
fn status(game: &Game) -> String {
    game.players
        .iter()
        .filter(|p| p.id.0 != u8::MAX && !p.neutral)
        .map(|p| {
            let cities = game
                .cities()
                .iter()
                .filter(|(_, c, _)| c.owner == p.id)
                .count();
            let units = game.player_units(p.id).len();
            let tag = if p.defeated { " (defeated)" } else { "" };
            format!("p{} {cities}c/{units}u/{}g{tag}", p.id.0, p.gold)
        })
        .collect::<Vec<_>>()
        .join(", ")
}
