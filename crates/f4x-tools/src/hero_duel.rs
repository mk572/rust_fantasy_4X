//! `f4x-tools duel heroes`: every hero against every other, one against one, with the abilities
//! the game rolls for a hero (task 655).
//!
//! A hero is one individual, so there are no multiples: the pair is fought one against one on open
//! ground with the 1.3 rules, the battle AI playing both (and the heroes casting the spells they
//! know, from their own mana). For each of `--seeds` seeds (default 10) both heroes are rolled
//! with the game's own roll (`hero::kind::roll_one`: the template's abilities, then the random
//! picks) from that seed, and the pair fights a best of three with the same two heroes and
//! fresh dice. The hero that starts (the attacker moves first) alternates by seed, so each
//! starts in half of them. A seed goes to the hero that wins two fights (nobody if the turn limit
//! ends the fights); the pair's score is the share of seeds the first hero wins, a drawn seed
//! half. Documented in `docs/duel.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use f4x_core::game::rng::GameRng;
use f4x_core::game::{Command, Game};
use f4x_core::hero::kind::roll_one;
use f4x_core::hero::Hero;
use f4x_core::map::{Plane, Position};
use f4x_def::content::Content;
use f4x_def::names::NameBundle;

use crate::ladder::{base_game, play_out};

pub const USAGE: &str =
    "usage: f4x-tools duel heroes [--level 1-9] [--seeds N] [--jobs N] [--out DIR] [--content DIR]
  Every hero against every other, one against one, at an experience level (default 5, Champion),
  with the abilities and spells the game rolls for a hero. Each of --seeds seeds (default 10, even)
  rolls both heroes anew and fights a best of three; each hero starts in half of the seeds.
  Writes heroes_L.csv (every pair) and heroes_L_rank.csv (the heroes by their mean score) into DIR
  (default target/ladder).";

const P0: f4x_core::player::PlayerId = f4x_core::player::PlayerId(0);
const P1: f4x_core::player::PlayerId = f4x_core::player::PlayerId(1);

fn mix(parts: &[u64]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in parts {
        h = (h ^ p).wrapping_mul(0x0100_0000_01b3).rotate_left(17);
    }
    h
}

/// One fight of two rolled heroes at `level`; `a_attacks` says who moves first. `Some(true)` when
/// A wins, `Some(false)` when B does, `None` when the turn limit ends it.
pub fn hero_fight(
    content: &Arc<Content>,
    (a, b): (&Hero, &Hero),
    level: usize,
    a_attacks: bool,
    seed: u64,
) -> Result<Option<bool>, String> {
    let mut game: Game = base_game(content, seed)?;
    let (west, east) = if a_attacks { (a, b) } else { (b, a) };
    let mut sides = Vec::new();
    for (owner, x, hero) in [(P0, 4u32, west), (P1, 5, east)] {
        let entity = game.scenario_make_hero(hero.clone());
        let at = Position {
            plane: Plane::Arcanus,
            x,
            y: 4,
        };
        let unit = game
            .spawn_hero(owner, at, hero.kind)
            .map_err(|e| format!("{e:?}"))?;
        debug_assert_eq!(unit, entity);
        let template = content
            .rules
            .hero_templates
            .get(hero.kind)
            .ok_or("no such hero")?;
        let path = content.rules.level_paths.get(
            content
                .rules
                .units
                .get(template.unit)
                .level_path
                .ok_or("no level path")?,
        );
        let xp = path.levels[(level.max(1) - 1).min(path.levels.len() - 1)].xp;
        game.scenario_set_experience(unit, xp);
        sides.push(vec![unit]);
    }
    let defenders = sides.pop().ok_or("no defender")?;
    let attackers = sides.pop().ok_or("no attacker")?;
    game.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    Ok(play_out(&mut game)?.map(|w| (w == P0) == a_attacks))
}

/// The seeds of one pair.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duel {
    pub a: usize,
    pub b: usize,
    pub a_seeds: u32,
    pub b_seeds: u32,
    pub drawn: u32,
}

impl Duel {
    /// The share of the seeds the first hero wins, a drawn seed half.
    pub fn score(&self) -> f64 {
        let total = self.a_seeds + self.b_seeds + self.drawn;
        (f64::from(self.a_seeds) + f64::from(self.drawn) / 2.0) / f64::from(total.max(1))
    }
}

/// The duel of two hero types: `seeds` seeds, a best of three each.
pub fn duel(
    content: &Arc<Content>,
    (ka, kb): (usize, usize),
    level: usize,
    seeds: u64,
    errors: &mut Vec<String>,
) -> Duel {
    let mut d = Duel {
        a: ka,
        b: kb,
        ..Duel::default()
    };
    let names = NameBundle::default();
    let rules = &content.rules;
    for s in 0..seeds {
        let mut rng = GameRng::new(mix(&[level as u64, ka as u64, kb as u64, s, 1]));
        let (Some(ta), Some(tb)) = (rules.hero_templates.get(ka), rules.hero_templates.get(kb))
        else {
            continue;
        };
        let ha = roll_one(&mut rng, rules, &names, ka, ta);
        let hb = roll_one(&mut rng, rules, &names, kb, tb);
        let (mut wa, mut wb) = (0, 0);
        for f in 0..3u64 {
            if wa == 2 || wb == 2 {
                break;
            }
            let seed = mix(&[level as u64, ka as u64, kb as u64, s, 2 + f]);
            match hero_fight(content, (&ha, &hb), level, s % 2 == 0, seed) {
                Ok(Some(true)) => wa += 1,
                Ok(Some(false)) => wb += 1,
                Ok(None) => {}
                Err(e) => errors.push(format!("heroes {ka} v {kb} seed {s} fight {f}: {e}")),
            }
        }
        match wa.cmp(&wb) {
            std::cmp::Ordering::Greater if wa >= 2 => d.a_seeds += 1,
            std::cmp::Ordering::Less if wb >= 2 => d.b_seeds += 1,
            _ => d.drawn += 1,
        }
    }
    d
}

fn hero_name(content: &Content, kind: usize) -> String {
    content.rules.hero_templates.get(kind).map_or_else(
        || "?".into(),
        |t| content.rules.units.get(t.unit).id.clone(),
    )
}

pub fn pairs_csv(content: &Content, duels: &[Duel]) -> String {
    let mut out = String::from("a,b,a_name,b_name,a_seeds,b_seeds,drawn,score_a\n");
    for d in duels {
        out += &format!(
            "{},{},{},{},{},{},{},{:.2}\n",
            d.a,
            d.b,
            hero_name(content, d.a),
            hero_name(content, d.b),
            d.a_seeds,
            d.b_seeds,
            d.drawn,
            d.score()
        );
    }
    out
}

/// The heroes by their mean score against all the others (best first): `score` is the mean share
/// of seeds won, `beats`, `loses` and `even` count the opponents with a score above, below and
/// at one half.
pub fn rank_csv(content: &Content, duels: &[Duel]) -> String {
    let mut by: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for d in duels {
        by.entry(d.a).or_default().push(d.score());
        by.entry(d.b).or_default().push(1.0 - d.score());
    }
    let mut rows: Vec<(usize, f64, usize, usize, usize)> = by
        .iter()
        .map(|(&k, v)| {
            let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
            let beats = v.iter().filter(|s| **s > 0.5 + 1e-9).count();
            let loses = v.iter().filter(|s| **s < 0.5 - 1e-9).count();
            (k, mean, beats, loses, v.len() - beats - loses)
        })
        .collect();
    rows.sort_by(|x, y| y.1.total_cmp(&x.1));
    let mut out = String::from("rank,kind,name,score,beats,loses,even\n");
    for (i, (k, mean, beats, loses, even)) in rows.iter().enumerate() {
        out += &format!(
            "{},{},{},{:.1},{beats},{loses},{even}\n",
            i + 1,
            k,
            hero_name(content, *k),
            mean * 100.0
        );
    }
    out
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut level, mut seeds, mut jobs) = (5usize, 10u64, 4usize);
    let (mut out, mut dir) = (
        PathBuf::from("target/ladder"),
        PathBuf::from(f4x_def::SHIPPED_CONTENT),
    );
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--level" => level = value("--level")?.parse().map_err(|_| "bad --level")?,
            "--seeds" => seeds = value("--seeds")?.parse().map_err(|_| "bad --seeds")?,
            "--jobs" => jobs = value("--jobs")?.parse().map_err(|_| "bad --jobs")?,
            "--out" => out = value("--out")?.into(),
            "--content" => dir = value("--content")?.into(),
            other => return Err(format!("unknown option {other:?}\n{USAGE}")),
        }
    }
    if seeds == 0 || seeds % 2 != 0 || !(1..=9).contains(&level) {
        return Err("--seeds must be even and --level 1 to 9".into());
    }
    let content = Arc::new(
        f4x_def::load::load_content_layers(&[&dir, Path::new(f4x_def::SHIPPED_CONTENT)])
            .map_err(|e| e.to_string())?,
    );
    let kinds = content.rules.hero_templates.len();
    let todo: Vec<(usize, usize)> = (0..kinds)
        .flat_map(|a| (a + 1..kinds).map(move |b| (a, b)))
        .collect();
    println!(
        "{kinds} heroes, {} pairs, level {level}, {seeds} seeds each",
        todo.len()
    );
    let next = AtomicUsize::new(0);
    let done = Mutex::new((Vec::new(), Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(&pair) = todo.get(i) else { break };
                let mut errors = Vec::new();
                let d = duel(&content, pair, level, seeds, &mut errors);
                let mut g = done.lock().expect("the result lock");
                g.0.push(d);
                g.1.extend(errors);
                if i.is_multiple_of(100) {
                    eprintln!("heroes: {i} of {} pairs", todo.len());
                }
            });
        }
    });
    let (mut duels, errors): (Vec<Duel>, Vec<String>) = done.into_inner().expect("the lock");
    duels.sort_by_key(|d| (d.a, d.b));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(
        out.join(format!("heroes_{level}.csv")),
        pairs_csv(&content, &duels),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        out.join(format!("heroes_{level}_rank.csv")),
        rank_csv(&content, &duels),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "heroes_{level}.csv and heroes_{level}_rank.csv written to {}",
        out.display()
    );
    for e in errors.iter().take(20) {
        eprintln!("ERROR {e}");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} fights failed", errors.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Action: the same two hero types are duelled twice. Outcome: the same result, the dice
    /// and the rolls being seeded.
    #[test]
    fn a_hero_duel_is_repeatable() {
        let c = Arc::new(crate::testutil::shipped().clone());
        let (mut e1, mut e2) = (Vec::new(), Vec::new());
        let one = duel(&c, (0, 1), 5, 2, &mut e1);
        let two = duel(&c, (0, 1), 5, 2, &mut e2);
        assert!(e1.is_empty(), "{e1:?}");
        assert_eq!(one, two);
        assert_eq!(one.a_seeds + one.b_seeds + one.drawn, 2);
    }

    /// Action: one hero type fights another with the roles swapped. Outcome: the two views of
    /// the score add to one.
    #[test]
    fn the_score_of_a_pair_is_the_share_of_seeds() {
        let d = Duel {
            a: 0,
            b: 1,
            a_seeds: 6,
            b_seeds: 3,
            drawn: 1,
        };
        assert!((d.score() - 0.65).abs() < 1e-9);
    }
}
