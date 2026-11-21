//! The summary every kind of soak prints after its games (`soak` and `soak-versus` call the same
//! routine, so their logs have the same lines): the games that ended with a winner, the counts
//! of `metrics.rs` per wizard and game for each kind of seat, the standing of the wizards at
//! the end, the casts of every game and of each kind of seat (always printed), and, when a seed was played in more than one
//! variant, who won the mixed games and where the variants stop being the same game.
//!
//! A plain soak is the one-variant case: all its seats are the new AI, so it has one column.
//! The per-game lines are `soak::game`'s; `print_log` prints those of a game taken from the cache.
//!
//! Tests: below, on made-up results (the columns, the wins and the first difference).

use std::path::Path;

use f4x_core::player::PlayerId;
use f4x_def::content::Content;

use crate::contestant::Version;
use crate::metrics::{
    add_casts, casts_line, known_never_cast, never_cast, never_cast_in, Metrics, Standing,
};
use crate::soak::{GameResult, Options};
use f4x_ai::System;

/// One game of a seed: its variant, who plays each seat, and how it went.
pub struct Played<'a> {
    pub seed: u64,
    /// The variant's name ("ref-all", "new-first"); empty for a plain soak.
    pub label: &'a str,
    /// The version of each wizard, by player number.
    pub versions: &'a [Version],
    pub result: &'a GameResult,
}

impl Played<'_> {
    /// The diplomacy system the wizard played (task 761); A in a game that recorded none.
    fn system_of(&self, player: PlayerId) -> System {
        self.result
            .systems
            .get(usize::from(player.0))
            .copied()
            .unwrap_or_default()
    }

    fn version_of(&self, player: PlayerId) -> Version {
        self.versions
            .get(usize::from(player.0))
            .copied()
            .unwrap_or(Version::New)
    }

    /// Whether every seat is played by the same version.
    fn pure(&self) -> Option<Version> {
        let first = *self.versions.first()?;
        self.versions.iter().all(|&v| v == first).then_some(first)
    }
}

fn version_name(version: Version) -> &'static str {
    match version {
        Version::New => "new",
        Version::Reference => "ref",
    }
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

/// The columns of the tables: the games all of one version play (named for the variant, "all"
/// for a plain soak) and, for the games that mix versions, the seats of each version. Reference
/// columns come first, so a comparison reads the same way every time.
fn columns(played: &[Played]) -> Vec<(String, Column)> {
    type Found = Vec<(u8, String, Column)>;
    fn column(found: &mut Found, rank: u8, name: String) -> usize {
        match found.iter().position(|(r, n, _)| *r == rank && *n == name) {
            Some(i) => i,
            None => {
                found.push((rank, name, Column::default()));
                found.len() - 1
            }
        }
    }
    let mut found = Found::new();
    for game in played {
        match game.pure() {
            Some(version) => {
                let rank = if version == Version::Reference { 0 } else { 3 };
                let name = if game.label.is_empty() {
                    "all"
                } else {
                    game.label
                };
                let i = column(&mut found, rank, name.to_string());
                found[i].2.add(game.result, |_| true);
            }
            None => {
                for version in [Version::Reference, Version::New] {
                    let rank = if version == Version::Reference { 1 } else { 2 };
                    let i = column(
                        &mut found,
                        rank,
                        format!("mixed: {}", version_name(version)),
                    );
                    found[i]
                        .2
                        .add(game.result, |p| game.version_of(p) == version);
                }
            }
        }
    }
    found.sort_by_key(|(rank, _, _)| *rank);
    found.into_iter().map(|(_, n, c)| (n, c)).collect()
}

/// The first snapshot turn at which the two games' fingerprints differ, if any.
fn first_difference(a: &GameResult, b: &GameResult) -> Option<u32> {
    a.fingerprints
        .iter()
        .zip(&b.fingerprints)
        .find(|((_, x), (_, y))| x != y)
        .map(|((turn, _), _)| *turn)
}

/// Print the lines a game printed when it was played: for a game taken from the cache.
pub fn print_log(result: &GameResult) {
    for line in &result.log {
        println!("{line}");
    }
}

/// The summary of a soak: `played` holds every game, the games of a seed together and in the
/// order of their variants (the first is the one the others are compared with).
pub fn print(opts: &Options, content: &Content, title: &str, played: &[Played]) {
    let mut seeds: Vec<u64> = played.iter().map(|g| g.seed).collect();
    seeds.dedup();
    println!();
    println!(
        "== {title}: {} seeds, {} players, {} games ==",
        seeds.len(),
        opts.players,
        played.len()
    );
    let ended = played.iter().filter(|g| g.result.over.is_some()).count();
    println!("{ended} of {} games ended with a winner", played.len());
    print_wins(played);
    print_systems(played);
    print_tables(&seeds, played);
    print_casts(content, played);
    print_divergence(opts, &seeds, played);
}

/// Games that ended with a winner by variant, and who won the games that mix versions.
fn print_wins(played: &[Played]) {
    let mut labels: Vec<&str> = Vec::new();
    for game in played.iter().filter(|g| g.pure().is_some()) {
        if !labels.contains(&game.label) {
            labels.push(game.label);
        }
    }
    if labels.len() > 1 {
        let by_label: Vec<String> = labels
            .iter()
            .map(|&label| {
                let games: Vec<_> = played
                    .iter()
                    .filter(|g| g.pure().is_some() && g.label == label)
                    .collect();
                let won = games.iter().filter(|g| g.result.over.is_some()).count();
                format!("{label} {won}/{}", games.len())
            })
            .collect();
        println!("games ended with a winner: {}", by_label.join(", "));
    }
    let mixed: Vec<&Played> = played.iter().filter(|g| g.pure().is_none()).collect();
    if mixed.is_empty() {
        return;
    }
    let won_by = |version| {
        mixed
            .iter()
            .filter(|g| {
                g.result
                    .over
                    .is_some_and(|o| g.version_of(o.winner) == version)
            })
            .count()
    };
    let undecided = mixed.iter().filter(|g| g.result.over.is_none()).count();
    println!(
        "mixed games (new in the first half of the seats, then in the second): new won {}, reference won {}, no winner {undecided}",
        won_by(Version::New),
        won_by(Version::Reference),
    );
}

/// The two diplomacy systems side by side, when a game had a wizard of system B (task 761): who won,
/// who was still alive at the end, and every count of `metrics.rs` per wizard and game for the seats
/// of each system.
fn print_systems(played: &[Played]) {
    if !played.iter().any(|g| g.result.systems.contains(&System::B)) {
        return;
    }
    let systems = [System::A, System::B];
    let mut columns: Vec<(String, Column)> = systems
        .iter()
        .map(|s| (s.name().to_string(), Column::default()))
        .collect();
    let (mut won, mut alive) = ([0usize; 2], [0usize; 2]);
    let mut undecided = 0;
    for game in played {
        for (i, system) in systems.iter().enumerate() {
            columns[i]
                .1
                .add(game.result, |p| game.system_of(p) == *system);
            alive[i] += game
                .result
                .standing
                .iter()
                .filter(|(p, st)| st.cities > 0 && game.system_of(*p) == *system)
                .count();
        }
        match game.result.over {
            Some(o) => {
                let i = systems.iter().position(|s| *s == game.system_of(o.winner));
                won[i.unwrap_or(0)] += 1;
            }
            None => undecided += 1,
        }
    }
    println!();
    println!("== By diplomacy system ==");
    println!(
        "wins: A {}, B {}, no winner {undecided} ({} games)",
        won[0],
        won[1],
        played.len()
    );
    println!(
        "wizards alive at the end (of {} and {}): A {}, B {}",
        columns[0].1.wizards, columns[1].1.wizards, alive[0], alive[1]
    );
    print_rows(&columns);
}

/// The counts per wizard and game for each kind of seat, then the standing at the end.
fn print_tables(seeds: &[u64], played: &[Played]) {
    let columns = columns(played);
    println!();
    println!("Per wizard and game (average over {} seeds):", seeds.len());
    print_rows(&columns);
}

/// The rows of the tables: each count per wizard and game, the first landing, the standing at the end.
fn print_rows(columns: &[(String, Column)]) {
    print!("{:<20}", "");
    for (name, _) in columns {
        print!("{name:>12}");
    }
    println!();
    for (i, (label, _)) in Metrics::default().rows().iter().enumerate() {
        print!("{label:<20}");
        for (_, col) in columns {
            print!("{:>12}", col.per_wizard(i64::from(col.metrics.rows()[i].1)));
        }
        println!();
    }
    print!("{:<20}", "first landing turn");
    for (_, col) in columns {
        print!(
            "{:>12}",
            col.metrics
                .first_landing()
                .map_or("-".into(), |t| format!("{t:.1}"))
        );
    }
    println!();
    println!("-- at the end of the game --");
    for (i, (label, _)) in Standing::default().rows().iter().enumerate() {
        print!("{label:<20}");
        for (_, col) in columns {
            print!("{:>12}", col.per_wizard(col.standing[i]));
        }
        println!();
    }
}

/// How often each spell was cast over all the games, and which map and battle spells never were:
/// for the whole soak, and, when it has several variants (`soak-versus`), for each variant, so the
/// new AI's casts can be read against the reference's.
fn print_casts(content: &Content, played: &[Played]) {
    let mut labels: Vec<&str> = Vec::new();
    for g in played {
        if !g.label.is_empty() && !labels.contains(&g.label) {
            labels.push(g.label);
        }
    }
    let everyone: Vec<&Played> = played.iter().collect();
    print_casts_of(content, &everyone, "all games");
    for label in labels {
        let group: Vec<&Played> = played.iter().filter(|g| g.label == label).collect();
        print_casts_of(content, &group, label);
    }
}

fn print_casts_of(content: &Content, played: &[&Played], who: &str) {
    let all = add_casts(played.iter().map(|g| g.result.tally.casts()));
    println!("{who}, casts: {}", casts_line(&all));
    println!(
        "{who}, map spells never cast: {}",
        never_cast(&content.rules, &all).join(", ")
    );
    let fought = add_casts(played.iter().map(|g| g.result.tally.battle_casts()));
    println!("{who}, battle casts: {}", casts_line(&fought));
    let known = add_casts(played.iter().map(|g| g.result.tally.known()));
    println!(
        "{who}, map spells known and never cast (wizards that knew them): {}",
        known_never_cast(
            &content.rules,
            &known,
            &all,
            f4x_def::rules::CastContext::Overland
        )
    );
    println!("{who}, spells known (wizards): {}", casts_line(&known));
    let mana: std::collections::BTreeMap<String, u32> = played
        .iter()
        .flat_map(|g| g.result.tally.battle_mana().iter())
        .fold(Default::default(), |mut m, (k, v)| {
            *m.entry(k.clone()).or_default() += (*v).max(0) as u32;
            m
        });
    println!("{who}, battle mana by spell: {}", casts_line(&mana));
    println!(
        "{who}, battle spells never cast: {}",
        never_cast_in(&content.rules, &fought, f4x_def::rules::CastContext::Battle).join(", ")
    );
}

/// Where each variant of a seed stops being the same game as the seed's first variant. Only
/// when a seed was played in more than one variant and snapshots were kept.
fn print_divergence(opts: &Options, seeds: &[u64], played: &[Played]) {
    if opts.snapshots_every == Some(0) || opts.snapshots_every.is_none() {
        return;
    }
    let mut rows = Vec::new();
    for &seed in seeds {
        let games: Vec<&Played> = played.iter().filter(|g| g.seed == seed).collect();
        for game in games.iter().skip(1) {
            rows.push((
                seed,
                game.label,
                first_difference(games[0].result, game.result),
                game.result.over.map(|o| o.turn),
            ));
        }
    }
    if rows.is_empty() {
        return;
    }
    println!();
    println!(
        "First snapshot that differs from the first variant (snapshots in {}/seedN-VARIANT/tNNNN.f4x):",
        opts.out
            .as_deref()
            .unwrap_or(Path::new("soak-out"))
            .display()
    );
    for (seed, name, turn, ended) in rows {
        let ended = ended.map_or(String::new(), |t| format!(", game ended turn {t}"));
        match turn {
            Some(t) => println!("  seed {seed} {name}: turn {t}{ended}"),
            None => println!("  seed {seed} {name}: none{ended}"),
        }
    }
    println!("Compare two of them: f4x-tools soak-diff A B");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Tally;

    /// The result of a game that was only set up (no turn played), with these fingerprints.
    fn result(fingerprints: Vec<(u32, u64)>) -> GameResult {
        let game = f4x_newgame::setup::new_game(
            crate::testutil::shipped(),
            &f4x_newgame::setup::NewGame {
                seed: 3,
                all_ai: true,
                ..f4x_newgame::setup::NewGame::with_wizards(2)
            },
        )
        .expect("a new game");
        GameResult {
            over: None,
            tally: Tally::new(&game),
            standing: Vec::new(),
            fingerprints,
            systems: Vec::new(),
            log: Vec::new(),
        }
    }

    fn played<'a>(
        seed: u64,
        label: &'a str,
        versions: &'a [Version],
        result: &'a GameResult,
    ) -> Played<'a> {
        Played {
            seed,
            label,
            versions,
            result,
        }
    }

    #[test]
    fn a_plain_soak_has_one_column_and_a_versus_has_four_in_a_fixed_order() {
        use Version::{New as N, Reference as R};
        let r = result(Vec::new());
        let soak = [played(1, "", &[N, N], &r)];
        let names: Vec<String> = columns(&soak).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["all"]);
        let (all_ref, first, last, all_new) = ([R, R], [N, R], [R, N], [N, N]);
        let versus = [
            played(1, "new-all", &all_new, &r),
            played(1, "new-first", &first, &r),
            played(1, "new-last", &last, &r),
            played(1, "ref-all", &all_ref, &r),
        ];
        let names: Vec<String> = columns(&versus).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["ref-all", "mixed: ref", "mixed: new", "new-all"]);
    }

    #[test]
    fn the_first_difference_is_the_first_snapshot_turn_that_is_not_the_same() {
        let a = result(vec![(50, 1), (100, 2), (150, 3)]);
        let b = result(vec![(50, 1), (100, 9), (150, 8)]);
        assert_eq!(first_difference(&a, &b), Some(100));
        assert_eq!(first_difference(&a, &a), None);
    }
}
