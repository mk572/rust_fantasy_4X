//! `f4x-tools soak-versus`: the AI under work against the frozen reference (`scripts/fork_ai.py`).
//! For each seed it plays four games, all with the reference's neutral cities and monsters:
//! reference only, the new AI in the first half of the seats, the new AI in the second half
//! (so the start positions weigh the same for both), and new only. It prints the counts of
//! `metrics.rs` per wizard for each side, and where the games stop being the same game.
//! `f4x-tools soak-diff A B` compares two saved snapshots.

use std::path::{Path, PathBuf};

use f4x_core::game::save::Snapshot;
use f4x_core::player::PlayerId;

use crate::cache;
use crate::contestant::Version;
use crate::metrics::{Metrics, Standing};
use crate::soak::{self, GameResult, Options, Roster};

pub const USAGE: &str = "usage: f4x-tools soak-versus [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--detail] [--snapshots-every N (default 50, 0: none)] [--out DIR (default soak-out)] [--refused-log FILE] [--jobs N (default: one for each core)] [--no-cache]\n       f4x-tools soak-diff SNAPSHOT_A SNAPSHOT_B [--content DIR]";

/// One of the four games of a seed.
struct Variant {
    name: &'static str,
    versions: Vec<Version>,
}

fn variants(players: u8) -> Vec<Variant> {
    let players = usize::from(players);
    let half = (players / 2).max(1);
    let seats = |first: Version, rest: Version| -> Vec<Version> {
        (0..players)
            .map(|k| if k < half { first } else { rest })
            .collect()
    };
    vec![
        Variant {
            name: "ref-all",
            versions: seats(Version::Reference, Version::Reference),
        },
        Variant {
            name: "new-first",
            versions: seats(Version::New, Version::Reference),
        },
        Variant {
            name: "new-last",
            versions: seats(Version::Reference, Version::New),
        },
        Variant {
            name: "new-all",
            versions: seats(Version::New, Version::New),
        },
    ]
}

/// The wizards of one kind of seat over all games: how many wizard-games, their counts and
/// their standing at the end.
#[derive(Default)]
struct Column {
    wizards: usize,
    metrics: Metrics,
    standing: [i64; 9],
}

impl Column {
    fn add(&mut self, game: &GameResult, seats: impl Fn(PlayerId) -> bool) {
        for (player, standing) in &game.standing {
            if !seats(*player) {
                continue;
            }
            self.wizards += 1;
            self.metrics += game.tally.of(*player);
            for (slot, (_, value)) in self.standing.iter_mut().zip(standing.rows()) {
                *slot += value;
            }
        }
    }

    fn per_wizard(&self, total: i64) -> String {
        if self.wizards == 0 {
            return "-".into();
        }
        format!("{:.1}", total as f64 / self.wizards as f64)
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut opts = Options::parse(args, USAGE)?;
    if opts.players < 2 {
        return Err("soak-versus needs at least 2 players".into());
    }
    opts.snapshots_every.get_or_insert(50);
    opts.out.get_or_insert_with(|| "soak-out".into());
    let content = opts.load_content()?;
    let refused = soak::open_refused_log(&opts.refused_log)?;
    let variants = variants(opts.players);
    let [mut ref_all, mut new_all, mut mixed_ref, mut mixed_new] = <[Column; 4]>::default();
    // Wins of the mixed games by version, and games nobody won.
    let (mut new_wins, mut ref_wins, mut undecided) = (0, 0, 0);
    let (mut ref_all_ended, mut new_all_ended, mut seeds) = (0, 0, 0);
    let mut divergence = Vec::new();
    // Every game of every seed, played `jobs` at a time; the reference-only game is taken from
    // the cache when this build has played it before.
    let out = opts.out.clone().unwrap_or_default();
    let work: Vec<(u64, usize)> = (opts.first..=opts.last)
        .flat_map(|seed| (0..variants.len()).map(move |k| (seed, k)))
        .collect();
    let all = soak::in_parallel(opts.jobs(), &work, |&(seed, k)| {
        let v = &variants[k];
        let cacheable = v.name == "ref-all" && !opts.no_cache && opts.battle_ai.is_none();
        if cacheable {
            if let Some(kept) = cache::load(&out, &opts, seed) {
                println!("seed {seed} [{}]: kept from an earlier run", v.name);
                return Ok(kept);
            }
        }
        let roster = Roster {
            versions: v.versions.clone(),
            neutrals: Version::Reference,
        };
        let result = soak::game(&content, seed, &opts, &roster, v.name, &refused)?;
        if cacheable {
            cache::store(&out, &opts, seed, &result)?;
        }
        Ok(result)
    })?;
    for (n, seed) in (opts.first..=opts.last).enumerate() {
        seeds += 1;
        let played = &all[n * variants.len()..(n + 1) * variants.len()];
        for (v, result) in variants.iter().zip(played) {
            let version_of = |p: PlayerId| v.versions[usize::from(p.0)];
            match v.name {
                "ref-all" => {
                    ref_all.add(result, |_| true);
                    ref_all_ended += usize::from(result.over.is_some());
                }
                "new-all" => {
                    new_all.add(result, |_| true);
                    new_all_ended += usize::from(result.over.is_some());
                }
                _ => {
                    mixed_new.add(result, |p| version_of(p) == Version::New);
                    mixed_ref.add(result, |p| version_of(p) == Version::Reference);
                    match result.over.map(|o| version_of(o.winner)) {
                        Some(Version::New) => new_wins += 1,
                        Some(Version::Reference) => ref_wins += 1,
                        None => undecided += 1,
                    }
                }
            }
        }
        for (v, result) in variants.iter().zip(played).skip(1) {
            divergence.push((
                seed,
                v.name,
                first_difference(&played[0], result),
                result.over.map(|o| o.turn),
            ));
        }
    }
    println!();
    println!(
        "== Versus: new AI against the reference, {seeds} seeds, {} players ==",
        opts.players
    );
    println!(
        "mixed games (new in the first half of the seats, then in the second): new won {new_wins}, reference won {ref_wins}, no winner {undecided}"
    );
    println!(
        "games ended with a winner: ref-all {ref_all_ended}/{seeds}, new-all {new_all_ended}/{seeds}"
    );
    println!();
    println!("Per wizard and game (average over {seeds} seeds):");
    let columns = [
        ("ref-all", &ref_all),
        ("mixed: ref", &mixed_ref),
        ("mixed: new", &mixed_new),
        ("new-all", &new_all),
    ];
    print!("{:<20}", "");
    for (name, _) in &columns {
        print!("{name:>12}");
    }
    println!();
    for (i, (label, _)) in Metrics::default().rows().iter().enumerate() {
        print!("{label:<20}");
        for (_, col) in &columns {
            print!("{:>12}", col.per_wizard(i64::from(col.metrics.rows()[i].1)));
        }
        println!();
    }
    println!("-- at the end of the game --");
    for (i, (label, _)) in Standing::default().rows().iter().enumerate() {
        print!("{label:<20}");
        for (_, col) in &columns {
            print!("{:>12}", col.per_wizard(col.standing[i]));
        }
        println!();
    }
    if opts.snapshots_every != Some(0) {
        println!();
        println!(
            "First snapshot that differs from ref-all (snapshots in {}/seedN-VARIANT/tNNNN.f4x):",
            opts.out
                .as_deref()
                .unwrap_or(Path::new("soak-out"))
                .display()
        );
        for (seed, name, turn, ended) in divergence {
            let ended = ended.map_or(String::new(), |t| format!(", game ended turn {t}"));
            match turn {
                Some(t) => println!("  seed {seed} {name}: turn {t}{ended}"),
                None => println!("  seed {seed} {name}: none{ended}"),
            }
        }
        println!("Compare two of them: f4x-tools soak-diff A B");
    }
    Ok(())
}

/// The first snapshot turn at which the two games' fingerprints differ, if any.
fn first_difference(a: &GameResult, b: &GameResult) -> Option<u32> {
    a.fingerprints
        .iter()
        .zip(&b.fingerprints)
        .find(|((_, x), (_, y))| x != y)
        .map(|((turn, _), _)| *turn)
}

/// `soak-diff`: show how two saved games differ.
pub fn diff(args: &[String]) -> Result<(), String> {
    let mut files = Vec::new();
    let mut dir: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--content" => dir = Some(it.next().ok_or("--content needs a value")?.into()),
            _ if a.starts_with("--") => return Err(format!("unknown option {a:?}\n{USAGE}")),
            _ => files.push(PathBuf::from(a)),
        }
    }
    let [a, b] = files.as_slice() else {
        return Err(USAGE.into());
    };
    let mut opts = Options::parse(&[], USAGE)?;
    if let Some(dir) = dir {
        opts.dir = dir;
    }
    let content = opts.load_content()?;
    let read =
        |path: &Path| std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let (text_a, text_b) = (read(a)?, read(b)?);
    for (path, text) in [(a, &text_a), (b, &text_b)] {
        let game = Snapshot::from_text(text)
            .and_then(|s| s.restore(content.clone()))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("{} (turn {}):", path.display(), game.turn);
        println!("  {}", soak::status(&game));
        for p in game
            .player_ids()
            .into_iter()
            .filter(|&p| !game.is_neutral_player(p))
        {
            let line: Vec<String> = Standing::of(&game, p)
                .rows()
                .iter()
                .map(|(name, v)| format!("{name} {v}"))
                .collect();
            println!("  p{}: {}", p.0, line.join(", "));
        }
    }
    let body = |t: &str| {
        t.split_once('\n')
            .map_or(String::new(), |(_, b)| b.to_string())
    };
    let (body_a, body_b) = (body(&text_a), body(&text_b));
    let differing: Vec<_> = body_a
        .lines()
        .zip(body_b.lines())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .collect();
    if differing.is_empty() && body_a.lines().count() == body_b.lines().count() {
        println!("The saved games are identical.");
        return Ok(());
    }
    println!(
        "{} lines differ (of {} and {}); the first ones:",
        differing.len(),
        body_a.lines().count(),
        body_b.lines().count()
    );
    for (n, (x, y)) in differing.iter().take(20) {
        println!("  line {}:\n    - {}\n    + {}", n + 2, x.trim(), y.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seats(players: u8) -> Vec<Vec<Version>> {
        variants(players).into_iter().map(|v| v.versions).collect()
    }

    #[test]
    fn the_new_ai_takes_each_half_of_the_seats_in_turn() {
        use Version::{New as N, Reference as R};
        assert_eq!(
            seats(4),
            vec![
                vec![R, R, R, R],
                vec![N, N, R, R],
                vec![R, R, N, N],
                vec![N, N, N, N]
            ]
        );
    }

    #[test]
    fn every_seat_is_played_by_each_version_once_across_the_mixed_games() {
        for players in 2..=5 {
            let all = seats(players);
            for seat in 0..usize::from(players) {
                let new_in_mixed = [&all[1], &all[2]]
                    .iter()
                    .filter(|v| v[seat] == Version::New)
                    .count();
                assert_eq!(new_in_mixed, 1, "{players} players, seat {seat}");
            }
        }
    }
}
