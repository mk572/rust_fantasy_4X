//! `f4x-tools soak`: let AI wizards play whole games against each other, with no window, and
//! report how each game went. Used to find AI and rules bugs (planned 24, `docs/PLANNED.md`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use f4x_ai::{play_round, Ai};
use f4x_core::game::setup::{new_game, NewGame};
use f4x_core::game::Game;
use f4x_core::player::PlayerId;

pub const USAGE: &str = "usage: f4x-tools soak [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--trace TURNS] [--detail] [--grant-mastery TURN] [--battle-ai CHECKPOINT] [--refused-log FILE]";

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut first, mut last) = (1u64, 3u64);
    let (mut turns, mut players, mut every) = (200u32, 2u8, 0u32);
    let mut trace = 0u32;
    let mut detail = false;
    let mut grant_mastery: Option<u32> = None;
    let mut battle_ai: Option<PathBuf> = None;
    let mut refused_log: Option<PathBuf> = None;
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
            "--detail" => detail = true,
            "--grant-mastery" => {
                grant_mastery = Some(value("--grant-mastery")?.parse().map_err(|_| "bad turn")?)
            }
            "--trace" => trace = value("--trace")?.parse().map_err(|_| "bad turns")?,
            "--every" => every = value("--every")?.parse().map_err(|_| "bad interval")?,
            "--content" => dir = Some(value("--content")?.into()),
            "--battle-ai" => battle_ai = Some(value("--battle-ai")?.into()),
            "--refused-log" => refused_log = Some(value("--refused-log")?.into()),
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
    // One line per refused command, tab separated, for finding out why the AI sends them.
    let mut refused_out = match &refused_log {
        Some(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = std::io::BufWriter::new(
                std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?,
            );
            writeln!(f, "seed\tturn\tplayer\tsource\treason\tcommand")
                .map_err(|e| e.to_string())?;
            Some(f)
        }
        None => None,
    };
    let mut summary = Vec::new();
    for seed in first..=last {
        let mut game = new_game(
            &content,
            &NewGame {
                seed,
                all_ai: true,
                ..NewGame::with_wizards(players)
            },
        )?;
        game.set_undo(false);
        if let Some(path) = &battle_ai {
            #[cfg(feature = "learned")]
            for k in 0..players {
                let brain = f4x_learn::brain::LearnedBrain::load(&content, path)?;
                game.set_battle_brain(PlayerId(k), Some(Box::new(brain)));
            }
            #[cfg(not(feature = "learned"))]
            return Err(format!(
                "--battle-ai {} needs f4x-tools built with `--features learned`",
                path.display()
            ));
        }
        let mut ais: Vec<Ai> = (0..players).map(|k| Ai::new(PlayerId(k))).collect();
        for ai in &mut ais {
            ai.record_refusals(refused_out.is_some());
        }
        let started = Instant::now();
        let mut rejected = 0usize;
        let (mut battles, mut captures) = (0usize, 0usize);
        while game.turn <= turns && game.game_over.is_none() {
            if grant_mastery == Some(game.turn) {
                grant_mastery_to_all(&mut game);
            }
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
                for (c, city, _) in game.economy().cities() {
                    let prod = game.economy().city_production(c);
                    let item = prod.item.map(|i| f4x_console::describe::item(&game, i));
                    let eco = game.economy().city_economy(c);
                    println!(
                        "  [t{}] {} (p{}): {:?} {}/{} eco {:?}",
                        game.turn,
                        city.name,
                        city.owner.0,
                        item,
                        prod.progress,
                        prod.item.map_or(0, |i| game.economy().item_cost(c, i)),
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
            if let Some(out) = refused_out.as_mut() {
                for ai in &ais {
                    for r in ai.take_refusals() {
                        writeln!(
                            out,
                            "{seed}\t{}\t{}\t{}\t{}\t{}",
                            r.turn, r.player.0, r.source, r.reason, r.command
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
            }
            battles += events
                .iter()
                .filter(|e| matches!(e, f4x_core::game::Event::BattleEnded { .. }))
                .count();
            captures += events
                .iter()
                .filter(|e| matches!(e, f4x_core::game::Event::CityCaptured { .. }))
                .count();
            rejected += events
                .iter()
                .filter(|e| matches!(e, f4x_core::game::Event::Rejected(_)))
                .count();
            if every > 0 && game.turn % every == 0 {
                println!("seed {seed} turn {}: {}", game.turn, status(&game));
                if detail {
                    for line in detail_lines(&game, &ais) {
                        println!("    {line}");
                    }
                }
            }
        }
        if detail {
            for (c, city, pos) in game.economy().cities() {
                if game.is_neutral_player(city.owner) {
                    continue;
                }
                let seen_by: Vec<String> = game
                    .players()
                    .iter()
                    .filter(|p| p.id != city.owner && game.is_explored(p.id, pos))
                    .map(|p| format!("p{}", p.id.0))
                    .collect();
                println!(
                    "    city {} of p{} at {:?} {},{} size {}: explored by [{}]",
                    city.name,
                    city.owner.0,
                    pos.plane,
                    pos.x,
                    pos.y,
                    game.economy().city_economy(c).map_or(0, |e| e.figures),
                    seen_by.join(" ")
                );
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
    game.players()
        .iter()
        .filter(|p| p.id.0 != u8::MAX && !p.neutral)
        .map(|p| {
            let cities = game
                .economy()
                .cities()
                .iter()
                .filter(|(_, c, _)| c.owner == p.id)
                .count();
            let units = game.units().player_units(p.id).len();
            let tag = if p.defeated { " (defeated)" } else { "" };
            format!("p{} {cities}c/{units}u/{}g{tag}", p.id.0, p.gold)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line per wizard about magic and exploring: mana, spells known, research, the spell being
/// cast and how much of the map is explored.
fn detail_lines(game: &Game, ais: &[Ai]) -> Vec<String> {
    let mut out = Vec::new();
    for p in game
        .players()
        .iter()
        .filter(|p| p.id.0 != u8::MAX && !p.neutral)
    {
        let Some(m) = game.magic(p.id) else {
            continue;
        };
        let name = |s: f4x_core::rules::SpellId| game.content().rules.spells.get(s).id.clone();
        let explored = game.explored_squares(p.id);
        let income = game.income(p.id);
        let mut known: Vec<String> = m.known.iter().map(|&s| name(s)).collect();
        known.sort();
        out.push(format!("  known: {}", known.join(" ")));
        out.push(format!(
            "p{}: skill {}/{}, gold {} (cities {}, buildings -{}, units -{}, food {}), food {}, mana {} (+{} upkeep {}), spells {}, researching {} ({} left), casting {}, split {}/{}/{}, explored {}{}, scouts {:?}, campaign {:?}, landing {:?}",
            p.id.0,
            m.skill_left,
            m.casting_skill(),
            game.treasury(p.id).net_gold(),
            game.treasury(p.id).city_gold,
            game.treasury(p.id).building_upkeep,
            game.treasury(p.id).unit_upkeep,
            game.treasury(p.id).food_gold,
            game.treasury(p.id).food_surplus,
            p.mana,
            income.split.mana,
            income.mana_upkeep,
            m.known.len(),
            m.researching.map_or("-".into(), name),
            m.research_remaining,
            m.casting
                .as_ref()
                .map_or("-".into(), |c| format!("{} ({} left)", name(c.spell), c.remaining)),
            m.mana_pct,
            m.research_pct,
            m.skill_pct,
            explored,
            if ais.iter().any(|a| a.player == p.id && a.frontier_closed()) {
                " (frontier closed)"
            } else {
                ""
            },
            ais.iter()
                .find(|a| a.player == p.id)
                .map(|a| a.scout_targets().into_iter().map(|(_, p)| (p.x, p.y)).collect::<Vec<_>>()),
            ais.iter().find(|a| a.player == p.id).and_then(|a| a.campaign()).map(|c| (c.x, c.y)),
            ais.iter()
                .find(|a| a.player == p.id)
                .and_then(|a| a.landing())
                .map(|(t, port, f)| ((t.x, t.y), (port.x, port.y), f))
        ));
    }
    out
}

/// Give every wizard the Spell of Mastery with the mana and skill to cast it, so a game shows
/// whether the AI casts it and wins without waiting for the 60,000 research points.
fn grant_mastery_to_all(game: &mut Game) {
    let Some(spell) = game.content().rules.spells.id_of("spell_of_mastery") else {
        return;
    };
    let ids: Vec<PlayerId> = game
        .players()
        .iter()
        .filter(|p| p.id.0 != u8::MAX && !p.neutral)
        .map(|p| p.id)
        .collect();
    for id in ids {
        if let Some(p) = game.player_mut(id) {
            p.magic.known.insert(spell);
            p.magic.researchable.remove(&spell);
            p.magic.skill_points += 2000;
            p.mana += 6000;
        }
    }
}
