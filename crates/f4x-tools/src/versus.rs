//! `f4x-tools soak-versus`: the AI under work against the frozen reference (`scripts/fork_ai.py`).
//! For each seed it plays four games, all with the reference's neutral cities and monsters:
//! reference only, the new AI in the first half of the seats, the new AI in the second half
//! (so the start positions weigh the same for both), and new only. It prints the counts of
//! `metrics.rs` per wizard for each side, and where the games stop being the same game.
//! `f4x-tools soak-diff A B` compares two saved snapshots.

use std::path::{Path, PathBuf};

use f4x_core::world::save::ContentCheck;
use f4x_master::SaveFile;

use crate::cache;
use crate::contestant::Version;
use crate::metrics::Standing;
use crate::report::{self, Played};
use crate::soak::{self, Options, Roster};

pub const USAGE: &str = "usage: f4x-tools soak-versus [--seeds FIRST..LAST] [--turns N] [--players 2..5] [--content DIR] [--every N] [--detail] [--snapshots-every N (default 50, 0: none)] [--out DIR (default soak-out)] [--refused-log FILE | --no-refused-log] [--jobs N (default: one for each core)] [--no-cache] [--mixed-only (play only the two mixed games of a seed, no reference-only or new-only game)] [--from SAVE (one game, on from that save)]\n       f4x-tools soak-diff SNAPSHOT_A SNAPSHOT_B [--content DIR]";

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

pub fn run(args: &[String]) -> Result<(), String> {
    let mixed_only = args.iter().any(|a| a == "--mixed-only");
    let args: Vec<String> = args
        .iter()
        .filter(|a| *a != "--mixed-only")
        .cloned()
        .collect();
    let mut opts = Options::parse(&args, USAGE)?;
    if opts.players < 2 {
        return Err("soak-versus needs at least 2 players".into());
    }
    opts.snapshots_every.get_or_insert(50);
    opts.out.get_or_insert_with(|| "soak-out".into());
    let content = opts.load_content()?;
    let refused = soak::open_refused_log(&soak::refused_log_path(&opts))?;
    let mut variants = variants(opts.players);
    if mixed_only {
        variants.retain(|v| v.name == "new-first" || v.name == "new-last");
    }
    // Every game of every seed, played `jobs` at a time; the reference-only game is taken from
    // the cache when this build has played it before.
    let out = opts.out.clone().unwrap_or_default();
    let work: Vec<(u64, usize)> = (opts.first..=opts.last)
        .flat_map(|seed| (0..variants.len()).map(move |k| (seed, k)))
        .collect();
    let all = soak::in_parallel(opts.jobs(), &work, |&(seed, k)| {
        let v = &variants[k];
        let cacheable = v.name == "ref-all" && !opts.no_cache;
        if cacheable {
            if let Some(kept) = cache::load(&out, &opts, seed) {
                report::print_log(&kept);
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
    let played: Vec<Played> = work
        .iter()
        .zip(&all)
        .map(|(&(seed, k), result)| Played {
            seed,
            label: variants[k].name,
            versions: &variants[k].versions,
            result,
        })
        .collect();
    report::print(
        &opts,
        &content,
        "Versus: new AI against the reference",
        &played,
    );
    Ok(())
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
        let world = SaveFile::from_text(text)
            .and_then(|s| s.restore(content.clone(), ContentCheck::Strict))
            .map_err(|e| format!("{}: {e}", path.display()))?
            .game;
        println!("{} (turn {}):", path.display(), world.round());
        println!("  {}", soak::status(&world));
        for p in world
            .player_ids()
            .into_iter()
            .filter(|&p| world.is_wizard(p))
        {
            let line: Vec<String> = Standing::of(&world, p, world.round())
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
