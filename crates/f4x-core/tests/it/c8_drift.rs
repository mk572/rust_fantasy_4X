//! Class 8: a change moves the game without being seen. The commands of the sweep, and the turns after
//! them, are played on the fresh saves, digested (`check::digest`: the saved text cut at the fields of
//! the snapshot, each hashed) and pinned in `tests/pins/core_drift.txt`. A rule change that moves a game
//! fails here, naming the plays and the fields that moved. The pins are no rule: they record what the game
//! does today. The plays that are turns are the game's (`f4x-master/tests/it/drift.rs`). When a move is
//! meant, regenerate the pins and review the diff of the file:
//!
//! `F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_`
//!
//! The long plays that need `new_game` and the AI are in `f4x-ai/tests/it`.

use crate::support::{commands, text, worlds};
use f4x_core::check::digest;
use f4x_core::world::testutil::pin;
use f4x_master::Game;
use std::collections::BTreeMap;

const PINS: &str = "tests/pins/core_drift.txt";

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

/// Every checkpoint on every world, as `world play@checkpoint` and its line.
fn today() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (world, fresh) in worlds() {
        let world = world.replace(' ', "").replace(',', "_");
        // Every command of the sweep, then three turns: what the commands changed, and what the turns made of it.
        let mut w = Game::from(fresh);
        for command in commands(&w) {
            w.command(command);
        }
        out.insert(format!("{world} commands@all"), line(&w));
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
    let header =
        "# Digests of the scripted plays of tests/it/c8_drift.rs: `world play@round: field=hash ...`.\n\
         # Regenerate on purpose: F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_\n";
    let refresh = "F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_";
    pin::check(&file, header, refresh, &today());
}

#[test]
fn the_digest_is_the_same_for_the_same_game_and_names_what_differs() {
    let (_, w) = worlds().remove(0);
    let copy = f4x_core::world::save::Snapshot::from_text(&text(&w))
        .expect("reads")
        .restore(crate::support::content())
        .expect("restores");
    assert_eq!(digest(&w), digest(&copy));
    let names: Vec<String> = digest(&w).into_iter().map(|(k, _)| k).collect();
    for wanted in ["units", "cities", "players", "map"] {
        assert!(
            names.iter().any(|n| n == wanted),
            "no field {wanted}: {names:?}"
        );
    }
    // One round moves the random generator and the units, not the settings or the seed.
    let mut later = Game::new(copy, w.rng.clone());
    later.play_round();
    assert!(later.draws() > w.draws());
    assert_eq!(later.seed(), w.seed());
    let (a, b) = (digest(&w), digest(&later));
    let moved: Vec<&str> = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| x != y)
        .map(|(x, _)| x.0.as_str())
        .collect();
    assert!(moved.contains(&"units"), "{moved:?}");
    assert!(!moved.contains(&"settings"), "{moved:?}");
}
