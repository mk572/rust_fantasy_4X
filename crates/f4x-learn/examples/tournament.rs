//! A tournament of trained battle networks, neural against neural (`docs/PLANNED.md` item 25).
//!
//! `tournament rank OUT_DIR --entries FILE [--pairs 15] [--top K] [options]`
//!     every entry plays every other in a short match; the standings say who is best, and
//!     `--top K` writes the best K as `parents.txt` (an entries file for `make_variants`).
//! `tournament run OUT_DIR --entries FILE [--swiss-rounds 6] [--swiss-pairs 15] [--ko 8]
//!     [--ko-pairs 101] [options]`
//!     a Swiss stage (rated by a Bradley-Terry fit of the pair results), then the top `--ko`
//!     entries play single-elimination knockout seeded by that rating.
//!
//! `tournament vs OUT_DIR --entries FILE --against NAME [--pairs 15]`
//!     every other entry plays the entry NAME in a short match; the profiles show what each
//!     did against it (use it to measure what a nudge or noise does to a parent).
//!
//! Options: `--bank FILE` (default models-local/bank-final.ron), `--seed N`, `--step-limit N`
//! (commands after which a battle is stopped as a draw, default 3000), `--vs-scripted N` (battles
//! each knockout entry plays against the scripted AI at the end, for information only; 0 skips).
//!
//! `--entries FILE`: one entry per line, `name<TAB>checkpoint<TAB>parent<TAB>note` (what
//! `make_variants` writes; the last two columns are optional).
//!
//! A pair is one scenario played twice with the sides swapped; the rules of a pair and a match
//! are in `f4x_learn::duel`. Finished matches are kept in OUT_DIR/matches, so a stopped run
//! continues where it was when started again with the same options. Run it through
//! `scripts/cool_run.sh` (or `scripts/tournament.sh`).

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::prelude::*;
use f4x_learn::bank::Bank;
use f4x_learn::duel::{play_pairs, tally, Decider, MatchResult, PairRecord};
use f4x_learn::net::{load_checkpoint, PolicyNet};
use f4x_learn::obs::ObsSpec;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

struct Entry {
    name: String,
    path: String,
    parent: String,
    note: String,
    net: PolicyNet,
}

/// A finished match, as kept on disk.
#[derive(Serialize, Deserialize)]
struct MatchRec {
    label: String,
    a: String,
    b: String,
    pairs: Vec<PairRecord>,
    result: MatchResult,
}

struct Ctx {
    spec: ObsSpec,
    content: f4x_core::content::Content,
    bank: Bank,
    device: Device,
    out: PathBuf,
    step_limit: u32,
    seed: u64,
    entries: Vec<Entry>,
    /// Every match played or loaded, in order.
    done: Vec<MatchRec>,
    log: std::fs::File,
}

impl Ctx {
    fn say(&mut self, line: impl AsRef<str>) {
        println!("{}", line.as_ref());
        let _ = writeln!(self.log, "{}", line.as_ref());
    }

    /// Play (or load) the match of entries `a` and `b`: `pairs` pairs on the scenarios of
    /// `slot`. `a` is entry 0 of every pair.
    fn play(
        &mut self,
        label: &str,
        a: usize,
        b: usize,
        pairs: usize,
        slot: u64,
    ) -> Result<usize, String> {
        let (na, nb) = (self.entries[a].name.clone(), self.entries[b].name.clone());
        let file = self.out.join("matches").join(format!("{label}.ron"));
        let kept = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| ron::from_str::<MatchRec>(&t).ok())
            .filter(|m| m.a == na && m.b == nb && m.pairs.len() == pairs);
        let (rec, took) = match kept {
            Some(m) => (m, None),
            None => {
                let t = Instant::now();
                let mut rng = Pcg64Mcg::seed_from_u64(self.seed ^ slot.wrapping_mul(0x9e37_79b9));
                let recs = play_pairs(
                    [&self.entries[a].net, &self.entries[b].net],
                    &self.spec,
                    &self.content,
                    &self.bank,
                    &self.device,
                    50_000_000 + slot * 100_000,
                    pairs,
                    self.step_limit,
                    &mut rng,
                )?;
                let result = tally(&recs, &mut rng);
                let m = MatchRec {
                    label: label.to_string(),
                    a: na.clone(),
                    b: nb.clone(),
                    pairs: recs,
                    result,
                };
                std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::write(
                    &file,
                    ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::default())
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                std::fs::write(file.with_extension("txt"), describe(&m))
                    .map_err(|e| e.to_string())?;
                (m, Some(t.elapsed().as_secs_f32()))
            }
        };
        let r = rec.result;
        let line = format!(
            "{label}: {na} {:.1} - {:.1} {nb} -> {}{}{}",
            r.points[0],
            r.points[1],
            if r.winner == 0 { &na } else { &nb },
            if r.coin_flip { " (coin flip)" } else { "" },
            match took {
                Some(s) => format!("  [{s:.0}s]"),
                None => "  [kept from an earlier run]".into(),
            }
        );
        self.say(line);
        self.done.push(rec);
        Ok(self.done.len() - 1)
    }

    fn index(&self, name: &str) -> usize {
        self.entries.iter().position(|e| e.name == name).unwrap()
    }
}

/// A match in words, pair by pair.
fn describe(m: &MatchRec) -> String {
    let mut s = format!(
        "{}: {} (entry 0) against {} (entry 1)\npoints {:.1} - {:.1}, winner {}{}\n\n",
        m.label,
        m.a,
        m.b,
        m.result.points[0],
        m.result.points[1],
        if m.result.winner == 0 { &m.a } else { &m.b },
        if m.result.coin_flip {
            " (coin flip)"
        } else {
            ""
        }
    );
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    for p in &m.pairs {
        *by.entry(format!("{:?}", p.decided_by)).or_default() += 1;
    }
    s.push_str(&format!("pairs decided by: {by:?}\n\n"));
    for p in &m.pairs {
        let g = |i: usize| {
            let g = &p.games[i];
            format!(
                "{} (kills {:?}, left {:?}, hp {:?}, mana {:?}, {} steps)",
                match g.winner {
                    Some(0) => "entry 0 won",
                    Some(_) => "entry 1 won",
                    None => "no winner",
                },
                g.kills,
                g.survivors,
                g.hit_points,
                g.mana,
                g.steps
            )
        };
        s.push_str(&format!(
            "seed {} odds {}: {:?} {:?}\n  entry 0 attacking: {}\n  entry 1 attacking: {}\n",
            p.seed,
            ["even", "overwhelming", "overwhelmed"][p.odds as usize],
            p.decided_by,
            p.points,
            g(0),
            g(1)
        ));
    }
    s
}

// ---- standings and rating ---------------------------------------------------------------

#[derive(Default, Clone)]
struct Line {
    matches: f32,
    match_wins: f32,
    pair_points: f32,
    pairs: f32,
}

fn lines(n: usize, ctx: &Ctx) -> Vec<Line> {
    let mut v = vec![Line::default(); n];
    for m in &ctx.done {
        let (a, b) = (ctx.index(&m.a), ctx.index(&m.b));
        for (me, e) in [(a, 0), (b, 1)] {
            v[me].matches += 1.0;
            v[me].match_wins += f32::from(m.result.winner == e);
            v[me].pair_points += m.result.points[e];
            v[me].pairs += m.pairs.len() as f32;
        }
    }
    v
}

/// A Bradley-Terry strength for each entry from the pair points of every match, as an Elo-like
/// number (1500 at the average). One imaginary even draw against a phantom of average strength
/// keeps an unbeaten or winless entry finite.
fn ratings(n: usize, ctx: &Ctx) -> Vec<f64> {
    let mut w = vec![vec![0.0f64; n]; n];
    for m in &ctx.done {
        let (a, b) = (ctx.index(&m.a), ctx.index(&m.b));
        w[a][b] += f64::from(m.result.points[0]);
        w[b][a] += f64::from(m.result.points[1]);
    }
    let mut p = vec![1.0f64; n];
    for _ in 0..1000 {
        let old = p.clone();
        for i in 0..n {
            let mut won = 0.5;
            let mut den = 1.0 / (old[i] + 1.0);
            for j in 0..n {
                if i != j {
                    won += w[i][j];
                    den += (w[i][j] + w[j][i]) / (old[i] + old[j]);
                }
            }
            p[i] = won / den;
        }
        let g = (p.iter().map(|x| x.ln()).sum::<f64>() / n as f64).exp();
        for x in &mut p {
            *x /= g;
        }
    }
    p.iter().map(|x| 1500.0 + 400.0 * x.log10()).collect()
}

/// Entry numbers best first: by rating.
fn order_by_rating(n: usize, ctx: &Ctx) -> (Vec<usize>, Vec<f64>) {
    let r = ratings(n, ctx);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| r[b].total_cmp(&r[a]));
    (idx, r)
}

fn standings_text(ctx: &Ctx, title: &str) -> String {
    let n = ctx.entries.len();
    let (order, r) = order_by_rating(n, ctx);
    let l = lines(n, ctx);
    let mut s =
        format!("{title}\n\n rank  rating  matches won  pair points  entry (parent; note)\n");
    for (k, &i) in order.iter().enumerate() {
        let e = &ctx.entries[i];
        s.push_str(&format!(
            "{:>5}  {:>6.0}  {:>4}/{:<7}  {:>5.1}/{:<5}  {} ({}; {})\n",
            k + 1,
            r[i],
            l[i].match_wins,
            l[i].matches,
            l[i].pair_points,
            l[i].pairs,
            e.name,
            e.parent,
            e.note
        ));
    }
    s
}

/// What each entry did over all its battles: the style profile.
fn profiles_text(ctx: &Ctx) -> String {
    let n = ctx.entries.len();
    #[derive(Default, Clone)]
    struct P {
        games: f32,
        wins: f32,
        casts: f32,
        flees: f32,
        mana: f32,
        lost: f32,
        kills: f32,
        steps: f32,
    }
    let mut v = vec![P::default(); n];
    for m in &ctx.done {
        let idx = [ctx.index(&m.a), ctx.index(&m.b)];
        for p in &m.pairs {
            for g in &p.games {
                for e in 0..2 {
                    let q = &mut v[idx[e]];
                    q.games += 1.0;
                    q.wins += f32::from(g.winner == Some(e));
                    q.casts += g.casts[e] as f32;
                    q.flees += g.flees[e] as f32;
                    q.mana += g.mana[e] as f32;
                    q.lost += g.lost[e] as f32;
                    q.kills += g.kills[e] as f32;
                    q.steps += g.steps as f32;
                }
            }
        }
    }
    let mut s = String::from(
        "Style profile: what each entry did, per battle, over all its battles in the tournament\n\n\
         battles  win%  casts  flees  mana  lost  kills  commands  entry (recipe)\n",
    );
    for (i, e) in ctx.entries.iter().enumerate() {
        let q = &v[i];
        let g = q.games.max(1.0);
        s.push_str(&format!(
            "{:>7}  {:>3.0}%  {:>5.2}  {:>5.2}  {:>4.0}  {:>4.1}  {:>5.1}  {:>8.0}  {} ({})\n",
            q.games,
            100.0 * q.wins / g,
            q.casts / g,
            q.flees / g,
            q.mana / g,
            q.lost / g,
            q.kills / g,
            q.steps / g,
            e.name,
            e.note
        ));
    }
    s
}

fn deciders_text(ctx: &Ctx) -> String {
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut coin = 0;
    for m in &ctx.done {
        coin += usize::from(m.result.coin_flip);
        for p in &m.pairs {
            *by.entry(format!("{:?}", p.decided_by)).or_default() += 1;
        }
    }
    let order = [
        Decider::Wins,
        Decider::Kills,
        Decider::Survivors,
        Decider::HitPoints,
        Decider::Mana,
        Decider::Draw,
    ];
    let total: usize = by.values().sum();
    let mut s =
        format!("Pairs decided by ({total} pairs; matches settled by coin flip: {coin}):\n");
    for d in order {
        s.push_str(&format!(
            "  {:?}: {}\n",
            d,
            by.get(&format!("{d:?}")).copied().unwrap_or(0)
        ));
    }
    s
}

// ---- stages -----------------------------------------------------------------------------

fn swiss(ctx: &mut Ctx, rounds: usize, pairs: usize) -> Result<(), String> {
    let n = ctx.entries.len();
    let mut points = vec![0.0f32; n];
    let mut pair_pts = vec![0.0f32; n];
    let mut played: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut had_bye = vec![false; n];
    let mut rng = Pcg64Mcg::seed_from_u64(ctx.seed);
    for round in 1..=rounds {
        let mut order: Vec<usize> = (0..n).collect();
        order.shuffle(&mut rng);
        if round > 1 {
            order.sort_by(|&a, &b| {
                points[b]
                    .total_cmp(&points[a])
                    .then(pair_pts[b].total_cmp(&pair_pts[a]))
            });
        }
        // An odd number: the lowest entry without a bye sits out and scores a match win.
        if n % 2 == 1 {
            let k = order.iter().rposition(|&i| !had_bye[i]).unwrap_or(n - 1);
            let bye = order.remove(k);
            had_bye[bye] = true;
            points[bye] += 1.0;
            let name = ctx.entries[bye].name.clone();
            ctx.say(format!("swiss round {round}: {name} has a bye"));
        }
        let mut free = order;
        let mut tables = Vec::new();
        while !free.is_empty() {
            let a = free.remove(0);
            let k = free
                .iter()
                .position(|b| !played[a].contains(b))
                .unwrap_or(0);
            tables.push((a, free.remove(k)));
        }
        for (t, (a, b)) in tables.into_iter().enumerate() {
            let label = format!(
                "swiss-r{round}-{}-vs-{}",
                ctx.entries[a].name, ctx.entries[b].name
            );
            let m = ctx.play(&label, a, b, pairs, (round * 1000 + t) as u64)?;
            let r = ctx.done[m].result;
            points[if r.winner == 0 { a } else { b }] += 1.0;
            pair_pts[a] += r.points[0];
            pair_pts[b] += r.points[1];
            played[a].insert(b);
            played[b].insert(a);
        }
    }
    Ok(())
}

/// Seeds (from 1) in bracket order, so that neighbours meet in round one and the top seeds meet
/// as late as possible.
fn bracket_order(k: usize) -> Vec<usize> {
    let mut order = vec![1usize];
    while order.len() < k {
        let total = order.len() * 2 + 1;
        order = order.iter().flat_map(|&s| [s, total - s]).collect();
    }
    order
}

fn knockout(
    ctx: &mut Ctx,
    k: usize,
    pairs: usize,
    bracket: &mut String,
) -> Result<Vec<usize>, String> {
    let n = ctx.entries.len();
    let (order, rating) = order_by_rating(n, ctx);
    let seeds: Vec<usize> = order.into_iter().take(k).collect();
    bracket.push_str("Knockout, seeded by rating after the Swiss stage:\n");
    for (s, &i) in seeds.iter().enumerate() {
        bracket.push_str(&format!(
            "  seed {}: {} (rating {:.0})\n",
            s + 1,
            ctx.entries[i].name,
            rating[i]
        ));
    }
    let mut alive: Vec<usize> = bracket_order(k).into_iter().map(|s| seeds[s - 1]).collect();
    let mut out_in: Vec<(usize, usize)> = Vec::new(); // (entry, round it went out in)
    let mut round = 1;
    while alive.len() > 1 {
        bracket.push_str(&format!("\nRound {round} ({} entries):\n", alive.len()));
        let mut next = Vec::new();
        for (t, pair) in alive.chunks(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let label = format!(
                "ko-r{round}-{}-vs-{}",
                ctx.entries[a].name, ctx.entries[b].name
            );
            let m = ctx.play(&label, a, b, pairs, (900 + round * 10 + t) as u64)?;
            let r = ctx.done[m].result;
            let (win, lose) = if r.winner == 0 { (a, b) } else { (b, a) };
            bracket.push_str(&format!(
                "  {} {:.1} - {:.1} {}  ->  {}{}\n",
                ctx.entries[a].name,
                r.points[0],
                r.points[1],
                ctx.entries[b].name,
                ctx.entries[win].name,
                if r.coin_flip { " (coin flip)" } else { "" }
            ));
            out_in.push((lose, round));
            next.push(win);
        }
        alive = next;
        round += 1;
    }
    let champion = alive[0];
    let mut placing = vec![champion];
    out_in.reverse();
    placing.extend(out_in.into_iter().map(|(e, _)| e));
    bracket.push_str(&format!("\nChampion: {}\n", ctx.entries[champion].name));
    Ok(placing)
}

fn load_entries(file: &Path, spec: &ObsSpec, device: &Device) -> Result<Vec<Entry>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            return Err(format!(
                "{}: '{line}' is not name<TAB>checkpoint",
                file.display()
            ));
        }
        if out.iter().any(|e| e.name == cols[0]) {
            return Err(format!(
                "{}: the name {} is used twice",
                file.display(),
                cols[0]
            ));
        }
        out.push(Entry {
            name: cols[0].to_string(),
            path: cols[1].to_string(),
            parent: cols.get(2).unwrap_or(&"-").to_string(),
            note: cols.get(3).unwrap_or(&"").to_string(),
            net: load_checkpoint(spec, Path::new(cols[1]), device)?,
        });
    }
    Ok(out)
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or("usage: tournament rank|run OUT_DIR --entries FILE [options]")?;
    if !["rank", "run", "vs"].contains(&mode.as_str()) {
        return Err(format!("unknown mode {mode} (rank, run or vs)"));
    }
    let out: PathBuf = args
        .next()
        .ok_or("usage: tournament rank|run OUT_DIR ...")?
        .into();
    let mut entries_file: Option<PathBuf> = None;
    let mut against: Option<String> = None;
    let mut bank_path = "models-local/bank-final.ron".to_string();
    let (mut pairs, mut top) = (15usize, 0usize);
    let (mut rounds, mut swiss_pairs, mut ko, mut ko_pairs) = (6usize, 15usize, 8usize, 101usize);
    let (mut seed, mut step_limit, mut vs_scripted) = (1u64, 3000u32, 300usize);
    while let Some(a) = args.next() {
        let mut v = |n: &str| args.next().ok_or(format!("{n} needs a value"));
        let num = |s: String| s.parse::<u64>().map_err(|_| format!("bad number '{s}'"));
        match a.as_str() {
            "--entries" => entries_file = Some(v("--entries")?.into()),
            "--against" => against = Some(v("--against")?),
            "--bank" => bank_path = v("--bank")?,
            "--pairs" => pairs = num(v("--pairs")?)? as usize,
            "--top" => top = num(v("--top")?)? as usize,
            "--swiss-rounds" => rounds = num(v("--swiss-rounds")?)? as usize,
            "--swiss-pairs" => swiss_pairs = num(v("--swiss-pairs")?)? as usize,
            "--ko" => ko = num(v("--ko")?)? as usize,
            "--ko-pairs" => ko_pairs = num(v("--ko-pairs")?)? as usize,
            "--seed" => seed = num(v("--seed")?)?,
            "--step-limit" => step_limit = num(v("--step-limit")?)? as u32,
            "--vs-scripted" => vs_scripted = num(v("--vs-scripted")?)? as usize,
            other => return Err(format!("unknown option {other}")),
        }
    }
    let entries_file = entries_file.ok_or("--entries FILE is required")?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let content =
        f4x_data::load_content_layers(&[Path::new("content")]).map_err(|e| e.to_string())?;
    let spec = ObsSpec::new(&content);
    let device = Device::flex();
    let entries = load_entries(&entries_file, &spec, &device)?;
    if entries.len() < 2 {
        return Err("a tournament needs at least two entries".into());
    }
    let text = std::fs::read_to_string(&bank_path)
        .map_err(|e| format!("{bank_path}: {e} (make one with the make_bank example)"))?;
    let bank = Bank::from_text(&text).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join("log.txt"))
        .map_err(|e| e.to_string())?;
    let mut ctx = Ctx {
        spec,
        content,
        bank,
        device,
        out: out.clone(),
        step_limit,
        seed,
        entries,
        done: Vec::new(),
        log,
    };
    let n = ctx.entries.len();
    let started = Instant::now();
    ctx.say(format!(
        "tournament {mode}: {n} entries from {}, bank {bank_path} ({} scenarios), step limit {step_limit}, seed {seed}",
        entries_file.display(),
        ctx.bank.scenarios.len()
    ));

    if mode == "rank" || mode == "vs" {
        let mut slot = 0;
        let anchor = match (&mode[..], &against) {
            ("vs", Some(name)) => Some(
                ctx.entries
                    .iter()
                    .position(|e| &e.name == name)
                    .ok_or_else(|| format!("no entry named {name}"))?,
            ),
            ("vs", None) => return Err("vs needs --against NAME".into()),
            _ => None,
        };
        for a in 0..n {
            for b in a + 1..n {
                if anchor.is_some_and(|k| a != k && b != k) {
                    continue;
                }
                let label = format!("rr-{}-vs-{}", ctx.entries[a].name, ctx.entries[b].name);
                ctx.play(&label, a, b, pairs, slot)?;
                slot += 1;
            }
        }
        let s = standings_text(
            &ctx,
            if mode == "vs" {
                "Against one entry"
            } else {
                "Round robin"
            },
        );
        std::fs::write(out.join("standings.txt"), &s).map_err(|e| e.to_string())?;
        std::fs::write(out.join("profiles.txt"), profiles_text(&ctx)).map_err(|e| e.to_string())?;
        ctx.say(s);
        if top > 0 {
            let (order, _) = order_by_rating(n, &ctx);
            let mut m = String::new();
            for &i in order.iter().take(top) {
                let e = &ctx.entries[i];
                m.push_str(&format!(
                    "{}\t{}\t{}\t{}\n",
                    e.name, e.path, e.parent, e.note
                ));
            }
            std::fs::write(out.join("parents.txt"), m).map_err(|e| e.to_string())?;
            ctx.say(format!(
                "the best {top} are in {}",
                out.join("parents.txt").display()
            ));
        }
        ctx.say(format!("done in {:.0}s", started.elapsed().as_secs_f32()));
        return Ok(());
    }

    // ---- run: Swiss, then the knockout -----------------------------------------------------
    let k = {
        let mut k = 1;
        while k * 2 <= ko.min(n) {
            k *= 2;
        }
        k
    };
    if k < 2 {
        return Err("the knockout needs at least two entries".into());
    }
    swiss(&mut ctx, rounds, swiss_pairs)?;
    let s = standings_text(&ctx, "After the Swiss stage");
    std::fs::write(out.join("standings-swiss.txt"), &s).map_err(|e| e.to_string())?;
    ctx.say(s);
    let mut bracket = String::new();
    let placing = knockout(&mut ctx, k, ko_pairs, &mut bracket)?;
    std::fs::write(out.join("bracket.txt"), &bracket).map_err(|e| e.to_string())?;
    ctx.say(&bracket);
    let s = standings_text(&ctx, "Final: rating over every match");
    std::fs::write(out.join("standings.txt"), &s).map_err(|e| e.to_string())?;
    std::fs::write(out.join("profiles.txt"), profiles_text(&ctx)).map_err(|e| e.to_string())?;
    std::fs::write(out.join("deciders.txt"), deciders_text(&ctx)).map_err(|e| e.to_string())?;
    ctx.say(deciders_text(&ctx));

    // The champion's weights.
    let champ_name = ctx.entries[placing[0]].name.clone();
    let champ = &ctx.entries[placing[0]];
    let from = PathBuf::from(&champ.path);
    std::fs::copy(&from, out.join("winner.bpk")).map_err(|e| format!("{}: {e}", from.display()))?;
    let shape = from.with_extension("shape.ron");
    if shape.exists() {
        let _ = std::fs::copy(&shape, out.join("winner.shape.ron"));
    }

    // For information only: how the knockout entries do against the scripted AI.
    let mut vs = String::new();
    if vs_scripted > 0 {
        let mut rng = Pcg64Mcg::seed_from_u64(seed);
        vs.push_str(&format!(
            "Against the scripted AI ({vs_scripted} battles as attacker, advice hidden; for information, not used to choose):\n"
        ));
        for &i in &placing {
            let ev = f4x_learn::train::evaluate(
                &ctx.entries[i].net,
                &ctx.spec,
                &ctx.content,
                &ctx.bank,
                &ctx.device,
                9_000_000,
                vs_scripted,
                false,
                &mut rng,
            )?;
            vs.push_str(&format!(
                "  {}: {}/{} won, reward {:.3}\n",
                ctx.entries[i].name, ev.wins, ev.games, ev.reward
            ));
        }
        std::fs::write(out.join("vs_scripted.txt"), &vs).map_err(|e| e.to_string())?;
        ctx.say(&vs);
    }
    let summary = format!(
        "tournament {n} entries, {} matches, {:.0}s\nchampion: {} -> {}\n\n{bracket}\n{vs}",
        ctx.done.len(),
        started.elapsed().as_secs_f32(),
        champ_name,
        out.join("winner.bpk").display()
    );
    std::fs::write(out.join("summary.txt"), summary).map_err(|e| e.to_string())?;
    ctx.say(format!(
        "champion: {} ({:.0}s)",
        ctx.entries[placing[0]].name,
        started.elapsed().as_secs_f32()
    ));
    Ok(())
}
