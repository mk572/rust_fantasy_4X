//! Class 8 for the plays that are turns (`docs/f4x-core-test-design.md`): short scripted plays on the
//! fresh saves are digested at their checkpoints and the digests are pinned in
//! `tests/pins/drift.txt`. A change that moves a game fails here, naming the plays and the fields that
//! moved. The pins are no rule: they record what the game does today. When the move is meant,
//! regenerate them and review the diff of the file:
//!
//! `F4X_REFRESH_PINS=1 cargo test -p f4x-master --test it drift`
//!
//! What a run of commands alone does to a world is pinned in core (`c8_drift.rs`).

use std::collections::BTreeMap;

use f4x_core::check::digest;
use f4x_core::world::testutil::pin;
use f4x_core::world::testutil::sweep::commands;
use f4x_master::Game;

use crate::support::{busy_round, games};

const PINS: &str = "tests/pins/drift.txt";

/// A play: its name, the rounds at whose end the game is digested, and the way it goes on one round.
type Round = fn(&mut Game);

fn walking(w: &mut Game) {
    busy_round(w, 6);
}

fn ending_turns(w: &mut Game) {
    w.play_round();
}

const PLAYS: [(&str, &[usize], Round); 2] = [
    ("walking", &[1, 5, 15], walking),
    ("turns", &[10, 20, 40], ending_turns),
];

/// The line of a game at a checkpoint: its round, its end and its generator, then `field=hash` for
/// each field of the digest of its world.
fn line(game: &Game) -> String {
    let over = game.game_over().map_or_else(
        || "none".to_string(),
        |o| format!("{}:{:?}:{}", o.winner.0, o.kind, o.turn),
    );
    let mut parts = vec![
        format!("round={}", game.round()),
        format!("over={over}"),
        format!("seed={}", game.seed()),
        format!("draws={}", game.draws()),
    ];
    parts.extend(digest(game).into_iter().map(|(k, v)| format!("{k}={v}")));
    parts.join(" ")
}

/// Every checkpoint of every play on every world, as `world play@round` and its line.
fn today() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (play, checkpoints, step) in PLAYS {
        for (world, mut w) in games() {
            let world = world.replace(' ', "").replace(',', "_");
            let last = *checkpoints.last().expect("a checkpoint");
            for round in 1..=last {
                step(&mut w);
                if checkpoints.contains(&round) {
                    out.insert(format!("{world} {play}@{round}"), line(&w));
                }
            }
        }
    }
    // Every command of the sweep, then three turns: what the turns made of what the commands changed.
    for (world, mut w) in games() {
        let world = world.replace(' ', "").replace(',', "_");
        for command in commands(&w) {
            w.command(command);
        }
        for _ in 0..3 {
            w.play_round();
        }
        out.insert(format!("{world} commands@+3turns"), line(&w));
    }
    out
}

#[test]
fn the_scripted_plays_end_where_they_ended() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PINS);
    let header = "# Digests of the scripted plays of tests/it/drift.rs: `world play@round: field=hash ...`.\n\
                  # Regenerate on purpose: F4X_REFRESH_PINS=1 cargo test -p f4x-master --test it drift\n";
    let refresh = "F4X_REFRESH_PINS=1 cargo test -p f4x-master --test it drift";
    pin::check(&file, header, refresh, &today());
}
