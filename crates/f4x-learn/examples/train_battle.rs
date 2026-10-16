//! Train the battle AI (task 170). Run through `scripts/cool_run.sh`:
//!
//! `RAYON_NUM_THREADS=4 scripts/cool_run.sh cargo run --release -p f4x-learn --features train --example train_battle -- OUT_DIR [--iters N] [--bc N] [--battles N] [--no-advice] [--seed N] [--until UNIX_SECONDS] [--resume CHECKPOINT] [--start-iter N]`
//!
//! It first clones the scripted AI on `--bc` scripted battles (a short warm start, skipped with
//! 0), then trains with PPO on self-play and play against the scripted AI. It writes a
//! checkpoint and a log line for each iteration under OUT_DIR.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::prelude::*;
use f4x_learn::obs::ObsSpec;
use f4x_learn::train::{
    baseline, evaluate, rollout, scripted_samples, Learner, Opponent, PpoConfig, RolloutConfig,
};
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

/// Seconds kept for the evaluations and the checkpoint after the last iteration of a run with a
/// deadline.
const EVAL_MARGIN: u64 = 360;

/// Battles in an evaluation (the held-out scenarios, with different dice each time round).
const EVAL_N: usize = 300;

/// What the wizard did in an evaluation: decisions, flees, casts and mana, and the most cast
/// spells (at most `top` of them).
fn wizard_line(ev: &f4x_learn::train::Evaluation, top: usize) -> String {
    let games = ev.games.max(1) as f32;
    let mut spells: Vec<(&String, &usize)> = ev.by_spell.iter().collect();
    spells.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let shown: Vec<String> = spells
        .iter()
        .take(top)
        .map(|(name, n)| format!("{name} {n}"))
        .collect();
    format!(
        "wizard asked {}, fled {}, cast {} ({:.2} a game), mana spent {} ({:.1} a game); spells: {}",
        ev.wizard_asks,
        ev.flees,
        ev.casts,
        ev.casts as f32 / games,
        ev.mana_spent,
        ev.mana_spent as f32 / games,
        if shown.is_empty() { "none".into() } else { shown.join(", ") }
    )
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out: PathBuf = args
        .next()
        .ok_or("usage: train_battle OUT_DIR [options]")?
        .into();
    let (mut iters, mut bc, mut battles, mut seed, mut advice) =
        (50usize, 200usize, 64usize, 1u64, 0.5f64);
    let mut briefs = true;
    // Stop at this time (Unix seconds): the last iteration is the one that would end after it.
    let mut until: Option<u64> = None;
    // Continue from a checkpoint's weights (the optimiser starts afresh); `--start-iter N` numbers
    // the iterations from N + 1, so the logs of the two runs line up.
    let mut resume: Option<PathBuf> = None;
    let mut start_iter = 0usize;
    while let Some(a) = args.next() {
        let mut v = |n: &str| args.next().ok_or(format!("{n} needs a value"));
        match a.as_str() {
            "--iters" => iters = v("--iters")?.parse().map_err(|_| "bad number")?,
            "--bc" => bc = v("--bc")?.parse().map_err(|_| "bad number")?,
            "--battles" => battles = v("--battles")?.parse().map_err(|_| "bad number")?,
            "--seed" => seed = v("--seed")?.parse().map_err(|_| "bad number")?,
            "--resume" => resume = Some(v("--resume")?.into()),
            "--start-iter" => start_iter = v("--start-iter")?.parse().map_err(|_| "bad number")?,
            "--until" => until = Some(v("--until")?.parse().map_err(|_| "bad number")?),
            "--no-advice" => advice = 0.0,
            "--no-briefs" => briefs = false,
            other => return Err(format!("unknown option {other}")),
        }
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let content =
        f4x_data::load_content_layers(&[Path::new("content")]).map_err(|e| e.to_string())?;
    let spec = ObsSpec::new(&content);
    let bank_path = std::env::var("BANK").unwrap_or_else(|_| "models-local/bank.ron".into());
    let text = std::fs::read_to_string(&bank_path)
        .map_err(|e| format!("{bank_path}: {e} (make one with the make_bank example)"))?;
    let bank = f4x_learn::bank::Bank::from_text(&text).map_err(|e| e.to_string())?;
    // The last scenarios of each class are held out for evaluation.
    let (train_bank, eval_bank) = {
        let mut train = f4x_learn::bank::Bank::default();
        let mut eval = f4x_learn::bank::Bank::default();
        for o in [
            f4x_learn::scenario::Odds::Even,
            f4x_learn::scenario::Odds::Overwhelming,
            f4x_learn::scenario::Odds::Overwhelmed,
        ] {
            let of: Vec<_> = bank
                .scenarios
                .iter()
                .filter(|s| s.odds == o)
                .cloned()
                .collect();
            let cut = of.len().saturating_sub(of.len() / 8 + 1);
            train.scenarios.extend(of[..cut].iter().cloned());
            eval.scenarios.extend(of[cut..].iter().cloned());
        }
        (train, eval)
    };
    let device = Device::flex();
    let mut learner = Learner::new(&spec, &device);
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let mut log = std::fs::File::create(out.join("log.txt")).map_err(|e| e.to_string())?;
    let mut say = |line: String| {
        println!("{line}");
        let _ = writeln!(log, "{line}");
    };
    let started = Instant::now();
    if let Some(path) = &resume {
        learner.net = f4x_learn::net::load_checkpoint(&spec, path, &learner.device)?;
        bc = 0;
        say(format!(
            "resumed from {} (weights only: the optimiser starts afresh, the warm start is skipped); \
             iterations are numbered from {}",
            path.display(),
            start_iter + 1
        ));
    }

    if bc > 0 {
        let data = scripted_samples(&spec, &content, &train_bank, 1_000_000, bc)?;
        let refs: Vec<_> = data.iter().collect();
        say(format!(
            "clone: {} scripted orders from {bc} battles",
            refs.len()
        ));
        for epoch in 0..6 {
            let (loss, hit) = learner.clone_scripted(&spec, &refs, 1e-3, 128);
            say(format!(
                "clone epoch {epoch}: loss {loss:.3}, orders matched {:.1}%",
                100.0 * hit
            ));
        }
    }
    let eval_seed = 9_000_000;
    for (name, random) in [("scripted AI", false), ("random orders", true)] {
        let b = baseline(
            &spec, &content, &eval_bank, eval_seed, EVAL_N, random, &mut rng,
        )?;
        say(format!(
            "baseline {name} as attacker vs scripted: {}/{} won, reward {:.3}, by odds {:?}",
            b.wins, b.games, b.reward, b.by_odds
        ));
    }
    let ev = evaluate(
        &learner.plain(),
        &spec,
        &content,
        &eval_bank,
        &learner.device,
        eval_seed,
        EVAL_N,
        false,
        &mut rng,
    )?;
    say(format!(
        "start: vs scripted {}/{} won, reward {:.3} ({:.0}s)",
        ev.wins,
        ev.games,
        ev.reward,
        started.elapsed().as_secs_f32()
    ));

    let ppo = PpoConfig::default();
    let mut next_seed = 2_000_000u64 + seed.saturating_sub(1) * 10_000_000;
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    };
    let mut iter_took = 0u64;
    // Casting, by spell, of every evaluation, for reading afterwards.
    let mut casts_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join("casts.txt"))
        .map_err(|e| e.to_string())?;
    for k in 1..=iters {
        let it = start_iter + k;
        let iter_began = now();
        let net = learner.plain();
        let mut samples = Vec::new();
        let mut st_all = Vec::new();
        let (mut asks, mut flees, mut casts) = (0, 0, 0);
        for opponent in [Opponent::SelfPlay, Opponent::Scripted] {
            let cfg = RolloutConfig {
                opponent,
                advice_chance: advice,
                random_briefs: briefs,
                ..RolloutConfig::default()
            };
            let (s, st) = rollout(
                &net,
                &spec,
                &content,
                &train_bank,
                &learner.device,
                &cfg,
                next_seed,
                battles,
                &mut rng,
            )?;
            next_seed += 10 * battles as u64;
            samples.extend(s);
            asks += st.wizard_asks;
            flees += st.flees;
            casts += st.casts;
            st_all.push(st);
        }
        let mana: i64 = st_all.iter().map(|s| s.mana_spent).sum();
        let (pl, vl, en) = learner.ppo(&spec, &samples, &ppo, &mut rng);
        say(format!(
            "iter {it}: {} decisions; self-play reward {:.3}, vs scripted {}/{} won (reward {:.3}); policy {pl:.3} value {vl:.3} entropy {en:.2}; wizard asked {asks}, fled {flees}, cast {casts}, mana spent {mana}; {:.0}s",
            samples.len(),
            st_all[0].reward,
            st_all[1].wins,
            st_all[1].games,
            st_all[1].reward,
            started.elapsed().as_secs_f32()
        ));
        // The last iteration: the count is reached, or another one would end after the deadline.
        iter_took = iter_took.max(now() - iter_began);
        let last = k == iters || until.is_some_and(|t| now() + iter_took + EVAL_MARGIN >= t);
        if it.is_multiple_of(5) || last {
            for shown in [false, true] {
                let ev = evaluate(
                    &learner.plain(),
                    &spec,
                    &content,
                    &eval_bank,
                    &learner.device,
                    eval_seed,
                    EVAL_N,
                    shown,
                    &mut rng,
                )?;
                let advice_word = if shown { "shown" } else { "hidden" };
                say(format!(
                    "eval {it} (advice {advice_word}): vs scripted {}/{} won, reward {:.3}; by odds (even, overwhelming, overwhelmed): {:?}; {}",
                    ev.wins,
                    ev.games,
                    ev.reward,
                    ev.by_odds,
                    wizard_line(&ev, 8)
                ));
                let _ = writeln!(
                    casts_file,
                    "eval {it} (advice {advice_word}): {}",
                    wizard_line(&ev, usize::MAX)
                );
            }
            f4x_learn::net::save_checkpoint(
                learner.net.clone(),
                &spec,
                &out.join(format!("net_{it:04}")),
            )?;
        }
        if last {
            say(format!("stopped after iteration {it}"));
            break;
        }
    }
    Ok(())
}
