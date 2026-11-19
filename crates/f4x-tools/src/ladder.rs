//! `f4x-tools duel ladder`: which unit beats which, as an exchange rate (task 655).
//!
//! For every pair of units (A, B) at one experience level the ladder fights real battles on open
//! ground with the 1.3 rules (`settings`), played by the battle AI with the units' own spells (no
//! wizard casts), and finds how many of A are worth how many of B:
//!
//! - it starts at 4 against 4; the side that wins more of the fights is the winner W, the other L;
//! - L gets a unit; if L now wins (or the fight is even) the ladder ends there with that ratio;
//!   else W loses one; then L gets another, and so on: 4:5, 3:5, 3:6, 2:6, 2:7, 1:7, 1:8, 1:9;
//! - W still winning at 1 against 9 is a wipeout;
//! - 4 against 4 that never ends in 100 turns (the engine's turn limit) is a stalemate.
//!
//! A step is `--seeds` fights (default 6) in which each side moves first half the time, and the
//! side with more wins wins the step. Documented in `docs/duel.md`; tests below.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use f4x_ai::battle::cast::next_command;
use f4x_core::battle::map::BattleMap;
use f4x_core::battle::BattleEvent;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::{Ability, Stat, UnitKind};
use f4x_def::content::Content;

pub const USAGE: &str = "usage: f4x-tools duel ladder [--levels 1-9|N,N,..] [--units N,N,..] [--normal] [--cross LO-HI] [--combine LO-HI] [--ratings FILE] [--seeds N] [--jobs N] [--out DIR] [--report] [--content DIR]
  Fights every unit against every other at each experience level (1 is a recruit or a plain hero,
  9 a Demi-God; a normal unit stops at 6, a fantastic unit has no levels) and writes, per level
  into DIR (default target/ladder): ladder_L.csv (every pair), ladder_L_rank.csv (the fitted strength
  of each unit) and ladder_L.html (the table of who beats whom); with several levels also
  ladder_ranking.csv; --normal fights normal units only (no heroes, creatures or settlers);
  --cross 1-4 fights every normal unit at a level against every one at a higher level
  (cross_A_B.csv, kept if there); --combine 1-4 fits one worth for every unit at every level
  from ladder_L.csv and cross_A_B.csv into ladder_levels.csv; --report builds the other files from the ladder_L.csv of an earlier run
  without fighting. --seeds is the fights of a step (default 6, even), --jobs the threads (default 4).";

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
/// The orders one fight may take before it counts as never ended (a stalemate takes 100 turns).
const MAX_COMMANDS: usize = 40_000;
/// Units at the start of a ladder (4 against 4) and the most one side fights with.
/// The unit that is worth one in the table: High Men spearmen.
const PAWN: u16 = 104; /* high_men_spearmen */
const START: u8 = 4;
const MOST: u8 = 9;

/// The rules the ladder fights by: 1.3, whose death and magic immunity exceptions are on (the
/// original has them; the shipped default has them off, `docs/game-settings.md`).
fn settings(g: &mut Game) {
    g.scenario_change_settings(|s| {
        s.rules.death_immunity_exceptions = true;
        s.rules.magic_immunity_exceptions = true;
    });
}

/// How one fight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    A,
    B,
    /// The turn limit ran out with both sides standing.
    Draw,
}

/// The experience level of a unit of this kind at ladder level `level` (1 to 9), as the level
/// index from 0 on its level path; `None` for a unit that does not level up (fantastic and
/// summoned creatures, units with no path).
pub fn path_index(content: &Content, num: u16, level: usize) -> Option<usize> {
    let def = content
        .rules
        .units
        .iter()
        .map(|(_, d)| d)
        .find(|d| d.num == num)?;
    if !matches!(def.kind, UnitKind::Normal | UnitKind::Hero) {
        return None;
    }
    let path = content.rules.level_paths.get(def.level_path?);
    Some((level.max(1) - 1).min(path.level_count().saturating_sub(1)))
}

/// A small game on open ground for a duel: two islands, a city of each of two players (the
/// attacker P0 in the west, the defender P1 in the east), the 1.3 rules (`settings`). The dice
/// come from `seed`.
pub fn base_game(content: &Arc<Content>, seed: u64) -> Result<Game, String> {
    let ocean = content.terrains.id_numbered(8).ok_or("no ocean")?;
    let grass = content.terrains.id_numbered(4).ok_or("no grassland")?;
    let race = content.rules.races.id_numbered(7).ok_or("no race 7")?;
    let grid = || {
        let mut g = Grid::new(40, 14, ocean);
        for y in 3..=10 {
            for x in (3..=10).chain(28..=35) {
                g.set(x, y, grass);
            }
        }
        g
    };
    let mut game = Game::new(seed);
    game.set_content(Arc::clone(content));
    settings(&mut game);
    game.set_map(Map::new(grid(), grid()));
    game.add_player(P0, "West");
    game.add_player(P1, "East");
    for (owner, x, name) in [(P0, 10, "Westport"), (P1, 28, "Eastport")] {
        game.apply(Command::FoundCity {
            owner,
            pos: Position {
                plane: Plane::Arcanus,
                x,
                y: 6,
            },
            name: name.into(),
            race: Some(race),
            population: 8,
        });
    }
    Ok(game)
}

/// One fight: `na` units of A against `nb` units of B on open ground. `a_attacks` says who is the
/// attacker (the one that moves first). The dice come from `seed`.
pub fn fight(
    content: &Arc<Content>,
    (a, na): (u16, u8),
    (b, nb): (u16, u8),
    levels: (usize, usize),
    a_attacks: bool,
    seed: u64,
) -> Result<Outcome, String> {
    let mut game = base_game(content, seed)?;
    // The attacker is P0, the defender P1.
    let (west, east) = if a_attacks {
        ((a, na, levels.0), (b, nb, levels.1))
    } else {
        ((b, nb, levels.1), (a, na, levels.0))
    };
    let mut sides = [Vec::new(), Vec::new()];
    for (i, (owner, x, (num, count, level))) in
        [(P0, 4u32, west), (P1, 5, east)].into_iter().enumerate()
    {
        let id = content
            .rules
            .units
            .id_numbered(num)
            .ok_or(format!("no unit {num}"))?;
        let index = path_index(content, num, level);
        let xp_of = |i: usize| -> u32 {
            let def = content.rules.units.get(id);
            def.level_path.map_or(0, |p| {
                let levels = &content.rules.level_paths.get(p).levels;
                levels[i.min(levels.len() - 1)].xp
            })
        };
        let mut bonus = 0;
        if let Some(index) = index {
            let def = content.rules.units.get(id);
            let top = def.level_path.map_or(0, |p| {
                content
                    .rules
                    .level_paths
                    .get(p)
                    .levels
                    .len()
                    .saturating_sub(1)
            });
            bonus = index.saturating_sub(top);
        }
        for _ in 0..count {
            let unit = game.spawn_unit(
                owner,
                Position {
                    plane: Plane::Arcanus,
                    x,
                    y: 4,
                },
                id,
            );
            if let Some(index) = index {
                game.scenario_set_experience(unit, xp_of(index));
            }
            sides[i].push(unit);
        }
        game.scenario_level_bonus(owner, bonus);
    }
    let [attackers, defenders] = sides;
    game.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    play_out(&mut game).map(|winner| match winner {
        None => Outcome::Draw,
        Some(w) if w == (if a_attacks { P0 } else { P1 }) => Outcome::A,
        Some(_) => Outcome::B,
    })
}

/// Play the battle that has just begun in `game` to its end on open ground, the battle AI with
/// the units' own spells (no wizard casts). The winner, or `None` when the turn limit ran out
/// with both sides standing.
pub fn play_out(game: &mut Game) -> Result<Option<PlayerId>, String> {
    if game.battle().is_none() {
        return Err("no battle began".into());
    }
    game.scenario_edit_battle(|b| b.map = BattleMap::open());
    let mut exhausted = false;
    for _ in 0..MAX_COMMANDS {
        let Some(command) = next_command(game) else {
            break;
        };
        let events = game.apply(Command::Battle(command.clone()));
        for event in &events {
            match event {
                Event::Rejected(why) => return Err(format!("{command:?} refused: {why:?}")),
                Event::Battle(BattleEvent::Exhausted) => exhausted = true,
                Event::BattleEnded { winner, .. } => {
                    return Ok(if exhausted { None } else { Some(*winner) });
                }
                _ => {}
            }
        }
    }
    Err("never ended".into())
}

/// The fights of one step of the ladder.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub a: u32,
    pub b: u32,
    pub draws: u32,
}

/// Who wins a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    A,
    B,
    /// As many wins each (and not mostly draws).
    Even,
    /// Mostly fights that ran out of turns.
    Stalemate,
}

impl Step {
    pub fn verdict(&self) -> Verdict {
        match self.a.cmp(&self.b) {
            std::cmp::Ordering::Greater => Verdict::A,
            std::cmp::Ordering::Less => Verdict::B,
            std::cmp::Ordering::Equal if self.draws > 2 * self.a => Verdict::Stalemate,
            std::cmp::Ordering::Equal => Verdict::Even,
        }
    }
}

fn mix(parts: &[u64]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in parts {
        h = (h ^ p).wrapping_mul(0x0100_0000_01b3).rotate_left(17);
    }
    h
}

/// `seeds` fights of `na` of A against `nb` of B, each side the attacker half of the time.
fn step(
    content: &Arc<Content>,
    (a, na): (u16, u8),
    (b, nb): (u16, u8),
    levels: (usize, usize),
    seeds: u64,
    errors: &mut Vec<String>,
) -> Step {
    let mut s = Step::default();
    for i in 0..seeds {
        let seed = mix(&[
            levels.0 as u64 * 16 + levels.1 as u64,
            u64::from(a),
            u64::from(b),
            u64::from(na),
            u64::from(nb),
            i,
        ]);
        match fight(content, (a, na), (b, nb), levels, i % 2 == 0, seed) {
            Ok(Outcome::A) => s.a += 1,
            Ok(Outcome::B) => s.b += 1,
            Ok(Outcome::Draw) => s.draws += 1,
            Err(e) => errors.push(format!(
                "{a} x{na} v {b} x{nb} levels {levels:?} seed {seed}: {e}"
            )),
        }
    }
    s
}

/// How a pair's ladder ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The first loser got the upper hand at these numbers.
    Flip,
    /// Even at these numbers.
    Even,
    /// The first winner still wins at 1 against 9.
    Wipeout,
    /// 4 against 4 never ended.
    Stalemate,
}

/// The result of the ladder for the pair (A, B): `na` of A are about `nb` of B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    pub a: u16,
    pub b: u16,
    pub kind: Kind,
    /// Who won at 4 against 4 (`Verdict::A` or `B`; the others for an even or a stalemate pair).
    pub first: Verdict,
    pub na: u8,
    pub nb: u8,
}

impl Pair {
    /// The pair seen from the other side.
    pub fn flipped(&self) -> Pair {
        Pair {
            a: self.b,
            b: self.a,
            first: match self.first {
                Verdict::A => Verdict::B,
                Verdict::B => Verdict::A,
                v => v,
            },
            na: self.nb,
            nb: self.na,
            ..*self
        }
    }

    /// The natural log of how many B one A is worth (positive: A is stronger); `None` for a stalemate.
    pub fn log_ratio(&self) -> Option<f64> {
        (self.kind != Kind::Stalemate).then(|| (f64::from(self.nb) / f64::from(self.na)).ln())
    }

    /// The text of a table cell: `4:5` (four of A are worth five of B), `1:9+` for a wipeout.
    pub fn cell(&self) -> String {
        match self.kind {
            Kind::Stalemate => "draw".into(),
            Kind::Wipeout if self.na < self.nb => format!("{}:{}+", self.na, self.nb),
            Kind::Wipeout => format!("{}+:{}", self.na, self.nb),
            _ => format!("{}:{}", self.na, self.nb),
        }
    }
}

/// The ladder of one pair, A at its level and B at its (`levels`).
pub fn ladder(
    content: &Arc<Content>,
    a: u16,
    b: u16,
    levels: (usize, usize),
    seeds: u64,
    errors: &mut Vec<String>,
) -> Pair {
    let pair = |kind, first, na, nb| Pair {
        a,
        b,
        kind,
        first,
        na,
        nb,
    };
    let first = step(content, (a, START), (b, START), levels, seeds, errors);
    let winner_a = match first.verdict() {
        Verdict::A => true,
        Verdict::B => false,
        Verdict::Stalemate => return pair(Kind::Stalemate, Verdict::Stalemate, START, START),
        Verdict::Even => return pair(Kind::Even, Verdict::Even, START, START),
    };
    let first_verdict = if winner_a { Verdict::A } else { Verdict::B };
    // Counts of the winner and of the loser; the loser gets a unit, then the winner loses one.
    let (mut w, mut l) = (START, START);
    let mut add_to_loser = true;
    loop {
        // The winner never drops below one unit: from 1:7 on only the loser grows (1:8, 1:9).
        if add_to_loser || w == 1 {
            l += 1;
        } else {
            w -= 1;
        }
        add_to_loser = !add_to_loser;
        let (na, nb) = if winner_a { (w, l) } else { (l, w) };
        let s = step(content, (a, na), (b, nb), levels, seeds, errors);
        let winner_still_wins = match s.verdict() {
            Verdict::A => winner_a,
            Verdict::B => !winner_a,
            _ => false,
        };
        if !winner_still_wins {
            let kind = match s.verdict() {
                Verdict::A | Verdict::B => Kind::Flip,
                _ => Kind::Even,
            };
            return pair(kind, first_verdict, na, nb);
        }
        if w == 1 && l == MOST {
            return pair(Kind::Wipeout, first_verdict, na, nb);
        }
    }
}

/// The units the ladder fights: those with an attack, that enter a battle (not ships or settlers).
pub fn unit_set(content: &Arc<Content>, chosen: &[u16], normal_only: bool) -> (Vec<u16>, Vec<u16>) {
    let (mut set, mut left_out) = (Vec::new(), Vec::new());
    for (_, d) in content.rules.units.iter() {
        if !chosen.is_empty() && !chosen.contains(&d.num) {
            continue;
        }
        // Normal units only: no heroes, no creatures, and no settlers (nobody fights with them).
        let settler = d.abilities.contains(&Ability::new(12 /* create_outpost */));
        if normal_only && (d.kind != UnitKind::Normal || settler) {
            continue;
        }
        let attacks = [Stat::Melee, Stat::Ranged]
            .iter()
            .any(|s| d.stats.get(s).is_some_and(|v| v.floor() > 0));
        let fits = attacks && fight(content, (d.num, 1), (d.num, 1), (1, 1), true, 1).is_ok();
        if fits {
            set.push(d.num);
        } else {
            left_out.push(d.num);
        }
    }
    set.sort_unstable();
    left_out.sort_unstable();
    (set, left_out)
}

/// Every pair of the set at one level, on `jobs` threads. Returns the pairs (each once, a < b)
/// and the fights that failed.
pub fn run_level(
    content: &Arc<Content>,
    set: &[u16],
    level: usize,
    seeds: u64,
    jobs: usize,
) -> (Vec<Pair>, Vec<String>) {
    let todo: Vec<(u16, u16)> = set
        .iter()
        .enumerate()
        .flat_map(|(i, &a)| set[i + 1..].iter().map(move |&b| (a, b)))
        .collect();
    run_todo(content, &todo, (level, level), seeds, jobs)
}

/// Every pair of the set with A at level `la` and B at level `lb` (a unit against its own kind
/// too, and both orders: the levels tell the two apart), on `jobs` threads.
pub fn run_cross(
    content: &Arc<Content>,
    set: &[u16],
    (la, lb): (usize, usize),
    seeds: u64,
    jobs: usize,
) -> (Vec<Pair>, Vec<String>) {
    let todo: Vec<(u16, u16)> = set
        .iter()
        .flat_map(|&a| set.iter().map(move |&b| (a, b)))
        .collect();
    run_todo(content, &todo, (la, lb), seeds, jobs)
}

fn run_todo(
    content: &Arc<Content>,
    todo: &[(u16, u16)],
    levels: (usize, usize),
    seeds: u64,
    jobs: usize,
) -> (Vec<Pair>, Vec<String>) {
    let next = AtomicUsize::new(0);
    let done = Mutex::new((Vec::new(), Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&(a, b)) = todo.get(i) else {
                    break;
                };
                let mut errors = Vec::new();
                let p = ladder(content, a, b, levels, seeds, &mut errors);
                let mut d = done.lock().expect("the result lock");
                d.0.push(p);
                d.1.extend(errors);
                if i.is_multiple_of(500) {
                    eprintln!("levels {levels:?}: {i} of {} pairs", todo.len());
                }
            });
        }
    });
    let (mut pairs, errors) = done.into_inner().expect("the result lock");
    pairs.sort_by_key(|p| (p.a, p.b));
    (pairs, errors)
}

/// Where the robust fit stops treating a residual as a fit error and starts treating it as a
/// counter (a log ratio of ln 2: twice as many units as the fit says).
const HUBER: f64 = std::f64::consts::LN_2;

/// A strength for each unit (the natural log of its worth, the mean 0) that fits the ratios of
/// the pairs best: for every pair `s[a] - s[b]` should be `ln(nb / na)`. A wipeout only says "at
/// least that much" and a stalemate has no ratio (`residual`). The fit is robust (Huber, by
/// iteratively reweighted least squares): a pair the fit misses by more than [`HUBER`] pulls less
/// the further off it is, so a unit that counters a few stronger ones does not take their rank.
pub fn strengths(set: &[u16], pairs: &[Pair]) -> BTreeMap<u16, f64> {
    let mut s: BTreeMap<u16, f64> = set.iter().map(|&u| (u, 0.0)).collect();
    let mut links: BTreeMap<u16, Vec<Pair>> = BTreeMap::new();
    for p in pairs.iter().filter(|p| p.log_ratio().is_some()) {
        links.entry(p.a).or_default().push(*p);
        links.entry(p.b).or_default().push(p.flipped());
    }
    for _ in 0..40 {
        for _ in 0..25 {
            let mut next = s.clone();
            for (u, ls) in &links {
                let (mut sum, mut weight) = (0.0, 0.0);
                for p in ls {
                    let Some(r) = residual(&s, p) else { continue };
                    let w = if r.abs() <= HUBER {
                        1.0
                    } else {
                        HUBER / r.abs()
                    };
                    sum += w * (s[&p.b] + (s[&p.a] - s[&p.b]) + r);
                    weight += w;
                }
                if weight > 0.0 {
                    next.insert(*u, sum / weight);
                }
            }
            let mean = next.values().sum::<f64>() / next.len().max(1) as f64;
            for v in next.values_mut() {
                *v -= mean;
            }
            s = next;
        }
    }
    s
}

/// How well the fit predicts pairs it has not seen: the pairs are split by a hash, the strengths
/// fitted on four fifths, and for the rest of the pairs with a winner at 4 against 4 the share
/// where the stronger unit by the fit is the winner, with the mean error of the log ratio of the
/// flips. Returns (pairs tested, share right, mean absolute log error).
pub fn holdout(set: &[u16], pairs: &[Pair]) -> (usize, f64, f64) {
    let held = |p: &Pair| mix(&[u64::from(p.a), u64::from(p.b), 99]).is_multiple_of(5);
    let train: Vec<Pair> = pairs.iter().filter(|p| !held(p)).copied().collect();
    let s = strengths(set, &train);
    let (mut tested, mut right, mut err, mut flips) = (0usize, 0usize, 0.0, 0usize);
    for p in pairs.iter().filter(|p| held(p)) {
        let fitted = s[&p.a] - s[&p.b];
        if matches!(p.first, Verdict::A | Verdict::B) {
            tested += 1;
            right += usize::from((fitted > 0.0) == (p.first == Verdict::A));
        }
        if let (Kind::Flip, Some(d)) = (p.kind, p.log_ratio()) {
            err += (d - fitted).abs();
            flips += 1;
        }
    }
    (
        tested,
        right as f64 / tested.max(1) as f64,
        err / flips.max(1) as f64,
    )
}

fn name(content: &Content, num: u16) -> String {
    content
        .rules
        .units
        .iter()
        .find(|(_, u)| u.num == num)
        .map_or_else(|| "?".to_string(), |(_, u)| u.id.clone())
}

fn kind_name(content: &Content, num: u16) -> &'static str {
    match content
        .rules
        .units
        .iter()
        .find(|(_, u)| u.num == num)
        .map(|(_, u)| u.kind)
    {
        Some(UnitKind::Normal) => "normal",
        Some(UnitKind::Hero) => "hero",
        Some(UnitKind::Summoned) => "summoned",
        _ => "fantastic",
    }
}

fn verdict_name(v: Verdict) -> &'static str {
    match v {
        Verdict::A => "a",
        Verdict::B => "b",
        Verdict::Even => "even",
        Verdict::Stalemate => "stalemate",
    }
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Flip => "flip",
        Kind::Even => "even",
        Kind::Wipeout => "wipeout",
        Kind::Stalemate => "stalemate",
    }
}

/// The CSV of every pair.
pub fn pairs_csv(content: &Content, pairs: &[Pair]) -> String {
    let mut out = String::from("a,b,a_name,b_name,first_winner,result,a_count,b_count,cell\n");
    for p in pairs {
        out += &format!(
            "{},{},{},{},{},{},{},{},{}\n",
            p.a,
            p.b,
            name(content, p.a),
            name(content, p.b),
            verdict_name(p.first),
            kind_label(p.kind),
            p.na,
            p.nb,
            p.cell()
        );
    }
    out
}

/// How far the fit is from what the ladder saw for this pair: observed minus fitted log ratio
/// (positive: A did better than the fit says). A wipeout only says "at least", so it counts only
/// when the fit puts the winner below that bound; a stalemate has no ratio.
pub fn residual(strength: &BTreeMap<u16, f64>, p: &Pair) -> Option<f64> {
    let d = p.log_ratio()?;
    let r = d - (strength[&p.a] - strength[&p.b]);
    Some(match p.kind {
        Kind::Wipeout if d > 0.0 => r.max(0.0),
        Kind::Wipeout => r.min(0.0),
        _ => r,
    })
}

/// The pairs the fit explains worst, the places where who beats whom is not a ranking (a unit
/// that beats a stronger one, rock, paper, scissors) or where a number may be wrong.
pub fn misfits_csv(content: &Content, strength: &BTreeMap<u16, f64>, pairs: &[Pair]) -> String {
    let mut rows: Vec<(f64, &Pair)> = pairs
        .iter()
        .filter_map(|p| residual(strength, p).map(|r| (r, p)))
        .collect();
    rows.sort_by(|x, y| y.0.abs().total_cmp(&x.0.abs()));
    let mut out = String::from("a,b,a_name,b_name,seen,fitted,residual\n");
    for (r, p) in rows.into_iter().take(300) {
        let fitted = (strength[&p.a] - strength[&p.b]).exp();
        out += &format!(
            "{},{},{},{},{},{:.2},{:.2}\n",
            p.a,
            p.b,
            name(content, p.a),
            name(content, p.b),
            p.cell(),
            fitted,
            r
        );
    }
    out
}

/// The worth in spearmen as a bracket, since the fit is no more exact than that: the nearest of
/// a coarse scale (1, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64 and so on).
pub fn bracket(spearmen: f64) -> String {
    let mut steps: Vec<f64> = vec![0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
    let mut x: f64 = 4.0;
    while x < 4096.0 {
        steps.extend([x, x * 1.5]);
        x *= 2.0;
    }
    let near = steps
        .iter()
        .copied()
        .min_by(|a, b| {
            (a.ln() - spearmen.ln())
                .abs()
                .total_cmp(&(b.ln() - spearmen.ln()).abs())
        })
        .unwrap_or(1.0);
    format!("{near}")
}

/// The CSV of the fitted strengths, strongest first; `worth` is the strength as a multiple of the
/// weakest unit's, so that 2.0 means two of the weakest are worth one of this.
pub fn rank_csv(content: &Content, strength: &BTreeMap<u16, f64>, pairs: &[Pair]) -> String {
    let mut rows: Vec<(u16, f64)> = strength.iter().map(|(&u, &s)| (u, s)).collect();
    rows.sort_by(|x, y| y.1.total_cmp(&x.1));
    let floor = rows.last().map_or(0.0, |r| r.1);
    // The pawn: the High Men spearmen, as in chess the pawn is one (the other races' basic units
    // are buffed, so their mean would not be a plain unit).
    let pawn = strength.get(&PAWN).copied().unwrap_or(0.0);
    let mut out = String::from(
        "rank,unit,name,kind,worth,spearmen,bracket,wins,losses,even,draws,misfit,power,power_worth,cost,beats_above_its_rank,loses_below_its_rank\n",
    );
    // The unit strength measure of the game (`battle::power`, the base stats) as a multiple of the
    // weakest unit's, to hold beside `worth` (it knows no levels: read it at level 1).
    let power_of = |u: u16| {
        content
            .rules
            .units
            .iter()
            .find(|(_, d)| d.num == u)
            .map_or((0.0, 0), |(_, d)| {
                (f64::from(f4x_core::battle::power::def_power(d)), d.cost)
            })
    };
    let least = rows
        .iter()
        .map(|r| power_of(r.0).0)
        .filter(|p| *p > 0.0)
        .fold(f64::MAX, f64::min);
    for (i, (u, s)) in rows.iter().enumerate() {
        let (mut w, mut l, mut e, mut d) = (0, 0, 0, 0);
        for p in pairs {
            let p = if p.a == *u {
                *p
            } else if p.b == *u {
                p.flipped()
            } else {
                continue;
            };
            match (p.kind, p.log_ratio()) {
                (Kind::Stalemate, _) => d += 1,
                (_, Some(r)) if r > 1e-9 => w += 1,
                (_, Some(r)) if r < -1e-9 => l += 1,
                _ => e += 1,
            }
        }
        let squares: Vec<f64> = pairs
            .iter()
            .filter(|p| p.a == *u || p.b == *u)
            .filter_map(|p| residual(strength, p))
            .map(|r| r * r)
            .collect();
        let misfit = (squares.iter().sum::<f64>() / squares.len().max(1) as f64).sqrt();
        let (power, cost) = power_of(*u);
        // The opponents the unit does most better and most worse against than its rank says.
        let mut seen: Vec<(f64, u16)> = pairs
            .iter()
            .filter(|p| p.a == *u || p.b == *u)
            .filter_map(|p| {
                let p = if p.a == *u { *p } else { p.flipped() };
                residual(strength, &p).map(|r| (r, p.b))
            })
            .collect();
        seen.sort_by(|x, y| y.0.total_cmp(&x.0));
        let list = |it: Vec<&(f64, u16)>| {
            it.iter()
                .filter(|x| x.0.abs() > HUBER)
                .map(|x| name(content, x.1))
                .collect::<Vec<_>>()
                .join(";")
        };
        let over = list(seen.iter().take(4).collect());
        let under = list(seen.iter().rev().take(4).collect());
        out += &format!(
            "{},{},{},{},{:.2},{:.2},{},{w},{l},{e},{d},{misfit:.2},{power:.1},{:.2},{cost},{over},{under}\n",
            i + 1,
            u,
            name(content, *u),
            kind_name(content, *u),
            (s - floor).exp(),
            (s - pawn).exp(),
            bracket((s - pawn).exp()),
            power / least
        );
    }
    out
}

/// A page with the table of who beats whom: rows and columns strongest first, a cell `4:5` says
/// four of the row's unit are worth five of the column's, green where the row is stronger.
pub fn html(
    content: &Content,
    level: usize,
    strength: &BTreeMap<u16, f64>,
    pairs: &[Pair],
) -> String {
    let mut order: Vec<u16> = strength.keys().copied().collect();
    order.sort_by(|a, b| strength[b].total_cmp(&strength[a]));
    let mut by: BTreeMap<(u16, u16), Pair> = BTreeMap::new();
    for p in pairs {
        by.insert((p.a, p.b), *p);
        by.insert((p.b, p.a), p.flipped());
    }
    let mut out = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Unit ladder, level {level}</title><style>\
:root{{--bg:#fff;--fg:#1a1a1a;--line:#ddd;--win:#2a9d4a;--lose:#d1453b;--grey:#999}}\
@media(prefers-color-scheme:dark){{:root{{--bg:#16181c;--fg:#e8e8e8;--line:#333}}}}\
body{{background:var(--bg);color:var(--fg);font:12px system-ui,sans-serif;margin:16px}}\
table{{border-collapse:collapse}}td,th{{border:1px solid var(--line);padding:1px 4px;text-align:center;white-space:nowrap}}\
th.row{{text-align:left;position:sticky;left:0;background:var(--bg)}}th.col{{writing-mode:vertical-rl;height:9em}}\
</style></head><body><h1>Who beats whom, level {level}</h1><p>Rows and columns strongest first. A cell <b>a:b</b> says <b>a</b> of the row's unit are worth <b>b</b> of the column's; green where the row's unit is stronger, red where weaker, <b>+</b> a wipeout, <b>draw</b> no winner in 100 turns.</p><table><tr><th></th>"
    );
    for &c in &order {
        out += &format!("<th class=\"col\">{}</th>", name(content, c));
    }
    out += "</tr>";
    for &r in &order {
        out += &format!("<tr><th class=\"row\">{}</th>", name(content, r));
        for &c in &order {
            let Some(p) = by.get(&(r, c)) else {
                out += "<td></td>";
                continue;
            };
            let (text, style) = match p.log_ratio() {
                None => ("draw".to_string(), "color:var(--grey)".to_string()),
                Some(x) => {
                    let strength = (x.abs() / 9f64.ln()).clamp(0.0, 1.0);
                    let colour = if x > 1e-9 {
                        "var(--win)"
                    } else if x < -1e-9 {
                        "var(--lose)"
                    } else {
                        "var(--grey)"
                    };
                    (
                        p.cell(),
                        format!(
                            "background:color-mix(in srgb,{colour} {}%,transparent)",
                            (strength * 70.0).round() as u32 + 5
                        ),
                    )
                }
            };
            out += &format!(
                "<td style=\"{style}\" title=\"{} against {}\">{text}</td>",
                name(content, r),
                name(content, c)
            );
        }
        out += "</tr>";
    }
    out + "</table></body></html>\n"
}

/// The pairs of a `ladder_L.csv` written earlier (`--report` builds the other files from it).
pub fn read_pairs(text: &str) -> Result<Vec<Pair>, String> {
    let bad = |line: &str| format!("bad ladder line {line:?}");
    text.lines()
        .skip(1)
        .map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 8 {
                return Err(bad(line));
            }
            let num = |i: usize| f[i].parse::<u16>().map_err(|_| bad(line));
            let verdict = match f[4] {
                "a" => Verdict::A,
                "b" => Verdict::B,
                "even" => Verdict::Even,
                _ => Verdict::Stalemate,
            };
            let kind = match f[5] {
                "flip" => Kind::Flip,
                "even" => Kind::Even,
                "wipeout" => Kind::Wipeout,
                _ => Kind::Stalemate,
            };
            Ok(Pair {
                a: num(0)?,
                b: num(1)?,
                kind,
                first: verdict,
                na: f[6].parse().map_err(|_| bad(line))?,
                nb: f[7].parse().map_err(|_| bad(line))?,
            })
        })
        .collect()
}

/// The node of a unit at a level in the combined fit; a unit that does not level up (a
/// creature) has the one node, level 0, whatever level it fights at.
fn node(num: u16, level: usize) -> u16 {
    num * 16 + level as u16
}

/// Whether the unit gains levels (a normal unit or a hero).
fn levels_up(content: &Content, num: u16) -> bool {
    content
        .rules
        .units
        .iter()
        .find(|(_, u)| u.num == num)
        .is_some_and(|(_, u)| matches!(u.kind, UnitKind::Normal | UnitKind::Hero))
}

/// The unit's node at the level it fights at.
fn node_at(content: &Content, num: u16, level: usize) -> u16 {
    node(num, if levels_up(content, num) { level } else { 0 })
}

/// One strength for every unit at every level of `range`, from the same-level files
/// (`ladder_L.csv`, every kind of unit) and the cross-level ones (`cross_A_B.csv`, normal units)
/// of `dir`, in spearmen: the worth as a multiple of a recruit High Men spearman. A creature has
/// one worth whatever the level (it is fought at each, and the fights all count for its one
/// node). Writes `ladder_levels.csv`, and with `ratings` the rating file (`unit_ratings.ron`).
fn combine_levels(
    content: &Arc<Content>,
    range: &[usize],
    dir: &Path,
    ratings: Option<&Path>,
) -> Result<(), String> {
    let (set, _) = unit_set(content, &[], false);
    // Settlers are nobody's fighters.
    let set: Vec<u16> = set
        .into_iter()
        .filter(|&u| {
            !content.rules.units.iter().any(|(_, d)| {
                d.num == u && d.abilities.contains(&Ability::new(12 /* create_outpost */))
            })
        })
        .collect();
    let read = |file: PathBuf| -> Result<Vec<Pair>, String> {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        read_pairs(&text)
    };
    let mut pairs = Vec::new();
    for (i, &la) in range.iter().enumerate() {
        for p in read(dir.join(format!("ladder_{la}.csv")))? {
            // Two creatures are the same fight at every level: only the first level's count.
            let both_fixed = !levels_up(content, p.a) && !levels_up(content, p.b);
            if set.contains(&p.a) && set.contains(&p.b) && (!both_fixed || la == range[0]) {
                pairs.push(Pair {
                    a: node_at(content, p.a, la),
                    b: node_at(content, p.b, la),
                    ..p
                });
            }
        }
        for &lb in &range[i + 1..] {
            for p in read(dir.join(format!("cross_{la}_{lb}.csv")))? {
                pairs.push(Pair {
                    a: node(p.a, la),
                    b: node(p.b, lb),
                    ..p
                });
            }
        }
    }
    // Only the units the files hold (a run of a few units fills few of them).
    let held: std::collections::BTreeSet<u16> = pairs.iter().flat_map(|p| [p.a, p.b]).collect();
    let set: Vec<u16> = set
        .into_iter()
        .filter(|&u| {
            range
                .iter()
                .any(|&l| held.contains(&node_at(content, u, l)))
        })
        .collect();
    let mut nodes: Vec<u16> = set
        .iter()
        .flat_map(|&u| range.iter().map(move |&l| node_at(content, u, l)))
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    let strength = strengths(&nodes, &pairs);
    let (tested, right, error) = holdout(&nodes, &pairs);
    println!(
        "all levels: {} units at {} levels, {} pairs; the fit predicts the winner of {:.1}% of {tested} pairs it has not seen (mean error of a flip a factor {:.2})",
        set.len(),
        range.len(),
        pairs.len(),
        right * 100.0,
        error.exp()
    );
    let pawn = strength
        .get(&node(PAWN, range[0]))
        .copied()
        .ok_or("the pawn is not in the data")?;
    let worth = |u: u16, l: usize| (strength[&node_at(content, u, l)] - pawn).exp();
    let mut text = String::from("unit,name,kind");
    for l in range {
        text += &format!(",spearmen_{l},bracket_{l}");
    }
    text += "\n";
    let mut rows: Vec<(f64, u16)> = set.iter().map(|&u| (worth(u, range[0]), u)).collect();
    rows.sort_by(|x, y| y.0.total_cmp(&x.0));
    for &(_, u) in &rows {
        text += &format!("{u},{},{}", name(content, u), kind_name(content, u));
        for &l in range {
            let w = worth(u, l);
            text += &format!(",{w:.2},{}", bracket(w));
        }
        text += "\n";
    }
    let file = dir.join("ladder_levels.csv");
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    println!("worth of every unit at every level: {}", file.display());
    if let Some(path) = ratings {
        let mut sorted: Vec<u16> = set.clone();
        sorted.sort_unstable();
        let mut ron = format!(
            "// What a unit is worth in a fight, in High Men spearmen (a recruit High Men spearman is 1): the\n// fit of `f4x-tools duel ladder` over real battles (task 655, `docs/duel.md`). `levels` is the worth at\n// experience level 1, 2, ... as far as it was measured ({} for a unit that levels up, one number for a\n// creature, which does not level); a higher level takes the last number. It is a rough guide, a\n// few percent either way: two units of 100 are worth one of 200, and rock-paper-scissors cases\n// (immunities, special attacks) can overturn it. A hero's numbers are those of the hero without the\n// random abilities, spells and items the game rolls, and `hero_factor` stands in for them.\n// Written by `f4x-tools duel ladder --combine {}-{} --ratings FILE`; no unit of the settler kind.\n(\n    hero_factor: 1.5,\n    units: [\n",
            range.len(),
            range[0],
            range[range.len() - 1]
        );
        for u in sorted {
            let values: Vec<String> = if levels_up(content, u) {
                range.iter().map(|&l| sig3(worth(u, l))).collect()
            } else {
                vec![sig3(worth(u, range[0]))]
            };
            ron += &format!(
                "        (num: {u}, id: \"{}\", levels: [{}]),\n",
                name(content, u),
                values.join(", ")
            );
        }
        ron += "    ],\n)\n";
        std::fs::write(path, ron).map_err(|e| e.to_string())?;
        println!("the ratings: {}", path.display());
    }
    Ok(())
}

/// A number to three significant figures, as RON text.
fn sig3(x: f64) -> String {
    let digits = if x >= 100.0 {
        0
    } else if x >= 10.0 {
        1
    } else {
        2
    };
    format!("{x:.digits$}")
}

fn parse_levels(text: &str) -> Result<Vec<usize>, String> {
    let bad = || format!("bad --levels {text:?}");
    if let Some((from, to)) = text.split_once('-') {
        let (from, to): (usize, usize) = (
            from.trim().parse().map_err(|_| bad())?,
            to.trim().parse().map_err(|_| bad())?,
        );
        return Ok((from..=to).collect());
    }
    text.split(',')
        .map(|n| n.trim().parse().map_err(|_| bad()))
        .collect()
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut report, mut normal, mut levels_given) = (false, false, false);
    let (mut cross, mut combine) = (None::<Vec<usize>>, None::<Vec<usize>>);
    let mut ratings = None::<PathBuf>;
    let (mut levels, mut units, mut seeds, mut jobs) =
        (vec![1usize], Vec::<u16>::new(), 6u64, 4usize);
    let (mut out, mut dir) = (
        PathBuf::from("target/ladder"),
        PathBuf::from(f4x_def::SHIPPED_CONTENT),
    );
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--levels" => {
                levels = parse_levels(&value("--levels")?)?;
                levels_given = true;
            }
            "--units" => {
                for n in value("--units")?.split(',') {
                    units.push(n.trim().parse().map_err(|_| "bad unit number")?);
                }
            }
            "--seeds" => seeds = value("--seeds")?.parse().map_err(|_| "bad --seeds")?,
            "--jobs" => jobs = value("--jobs")?.parse().map_err(|_| "bad --jobs")?,
            "--out" => out = value("--out")?.into(),
            "--report" => report = true,
            "--normal" => normal = true,
            "--cross" => cross = Some(parse_levels(&value("--cross")?)?),
            "--combine" => combine = Some(parse_levels(&value("--combine")?)?),
            "--ratings" => ratings = Some(PathBuf::from(value("--ratings")?)),
            "--content" => dir = value("--content")?.into(),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    if (cross.is_some() || combine.is_some()) && !levels_given {
        levels.clear();
    }
    if seeds == 0 || seeds % 2 != 0 {
        return Err("--seeds must be even, so that each side moves first as often".into());
    }
    if levels.iter().any(|l| !(1..=9).contains(l)) {
        return Err("--levels are 1 to 9".into());
    }
    let content = Arc::new(
        f4x_def::load::load_content_layers(&[&dir, Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())?,
    );
    // `--report`: the pairs are read from the files of an earlier run, nothing is fought.
    let (set, left_out) = if report {
        (Vec::new(), Vec::new())
    } else {
        unit_set(&content, &units, normal)
    };
    println!(
        "{} units, {} pairs per level; left out (no attack or not in a battle): {:?}",
        set.len(),
        set.len() * set.len().saturating_sub(1) / 2,
        left_out
    );
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut ranking: BTreeMap<u16, Vec<f64>> = BTreeMap::new();
    let mut failed = 0;
    for &level in &levels {
        let (set, pairs, errors) = if report {
            let file = out.join(format!("ladder_{level}.csv"));
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let pairs = read_pairs(&text)?;
            let mut set: Vec<u16> = pairs.iter().flat_map(|p| [p.a, p.b]).collect();
            set.sort_unstable();
            set.dedup();
            (set, pairs, Vec::new())
        } else {
            let (pairs, errors) = run_level(&content, &set, level, seeds, jobs);
            (set.clone(), pairs, errors)
        };
        failed += errors.len();
        for e in errors.iter().take(20) {
            eprintln!("ERROR {e}");
        }
        let strength = strengths(&set, &pairs);
        for (&u, &s) in &strength {
            ranking.entry(u).or_default().push(s);
        }
        for (file, text) in [
            (format!("ladder_{level}.csv"), pairs_csv(&content, &pairs)),
            (
                format!("ladder_{level}_misfits.csv"),
                misfits_csv(&content, &strength, &pairs),
            ),
            (
                format!("ladder_{level}_rank.csv"),
                rank_csv(&content, &strength, &pairs),
            ),
            (
                format!("ladder_{level}.html"),
                html(&content, level, &strength, &pairs),
            ),
        ] {
            std::fs::write(out.join(&file), text).map_err(|e| e.to_string())?;
        }
        let (tested, right, error) = holdout(&set, &pairs);
        println!(
            "level {level}: the fit predicts the winner of {:.1}% of {tested} pairs it has not seen (mean error of a flip {:.2} in the log, a factor {:.2})",
            right * 100.0,
            error,
            error.exp()
        );
        let count = |k: Kind| pairs.iter().filter(|p| p.kind == k).count();
        println!(
            "level {level}: {} pairs, {} flips, {} even, {} wipeouts, {} stalemates, written to {}",
            pairs.len(),
            count(Kind::Flip),
            count(Kind::Even),
            count(Kind::Wipeout),
            count(Kind::Stalemate),
            out.display()
        );
    }
    if let Some(range) = &cross {
        // Every normal unit at one level against every normal unit at a higher one; a file of
        // an earlier run is kept, so that a run that stopped goes on where it left off.
        let (set, _) = unit_set(&content, &units, true);
        for (i, &la) in range.iter().enumerate() {
            for &lb in &range[i + 1..] {
                let file = out.join(format!("cross_{la}_{lb}.csv"));
                if file.exists() {
                    println!("levels {la} and {lb}: {} is there", file.display());
                    continue;
                }
                let (pairs, errors) = run_cross(&content, &set, (la, lb), seeds, jobs);
                failed += errors.len();
                for e in errors.iter().take(20) {
                    eprintln!("ERROR {e}");
                }
                std::fs::write(&file, pairs_csv(&content, &pairs)).map_err(|e| e.to_string())?;
                println!(
                    "levels {la} and {lb}: {} pairs, written to {}",
                    pairs.len(),
                    file.display()
                );
            }
        }
    }
    if let Some(range) = &combine {
        combine_levels(&content, range, &out, ratings.as_deref())?;
    }
    if levels.len() > 1 {
        let mut text = String::from("unit,name,kind");
        for l in &levels {
            text += &format!(",worth_{l}");
        }
        text += "\n";
        let floors: Vec<f64> = (0..levels.len())
            .map(|i| ranking.values().map(|v| v[i]).fold(f64::MAX, f64::min))
            .collect();
        for (u, v) in &ranking {
            text += &format!("{u},{},{}", name(&content, *u), kind_name(&content, *u));
            for (i, s) in v.iter().enumerate() {
                text += &format!(",{:.2}", (s - floors[i]).exp());
            }
            text += "\n";
        }
        std::fs::write(out.join("ladder_ranking.csv"), text).map_err(|e| e.to_string())?;
    }
    if failed > 0 {
        return Err(format!("{failed} fights failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Arc<Content> {
        Arc::new(crate::testutil::shipped().clone())
    }

    /// Action: a unit fights an exact copy of itself, each side moving first in turn. Outcome: it is
    /// an even fight, so the ladder stops at 4 against 4 (or a stalemate), never a flip further on.
    #[test]
    fn a_unit_against_its_own_kind_is_even_or_a_standoff() {
        let c = content();
        let (set, _) = unit_set(&c, &[], false);
        let u = set[0];
        let mut errors = Vec::new();
        let p = ladder(&c, u, u, (1, 1), 6, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            matches!(p.kind, Kind::Even | Kind::Stalemate | Kind::Flip),
            "{p:?}"
        );
        assert!(p.na <= MOST && p.nb <= MOST);
    }

    /// Action: the same pair is fought twice. Outcome: the same result (the dice are seeded).
    #[test]
    fn a_ladder_is_repeatable() {
        let c = content();
        let (set, _) = unit_set(&c, &[], false);
        let (a, b) = (set[0], set[set.len() / 2]);
        let (mut e1, mut e2) = (Vec::new(), Vec::new());
        assert_eq!(
            ladder(&c, a, b, (1, 1), 2, &mut e1),
            ladder(&c, a, b, (1, 1), 2, &mut e2)
        );
    }

    /// Action: a pair seen from either side. Outcome: the two views agree on the exchange rate.
    #[test]
    fn a_pair_seen_from_the_other_side_has_the_opposite_ratio() {
        let p = Pair {
            a: 1,
            b: 2,
            kind: Kind::Flip,
            first: Verdict::A,
            na: 4,
            nb: 5,
        };
        let q = p.flipped();
        assert_eq!((q.na, q.nb, q.first), (5, 4, Verdict::B));
        assert!((p.log_ratio().unwrap_or(0.0) + q.log_ratio().unwrap_or(0.0)).abs() < 1e-9);
    }

    /// Action: the fit over a chain of pairs. Outcome: the unit that wins ranks above the one it beats.
    #[test]
    fn the_fit_ranks_the_winner_above_the_loser() {
        let pairs = [
            Pair {
                a: 1,
                b: 2,
                kind: Kind::Flip,
                first: Verdict::A,
                na: 3,
                nb: 5,
            },
            Pair {
                a: 2,
                b: 3,
                kind: Kind::Flip,
                first: Verdict::A,
                na: 4,
                nb: 5,
            },
        ];
        let s = strengths(&[1, 2, 3], &pairs);
        assert!(s[&1] > s[&2] && s[&2] > s[&3]);
    }
}
