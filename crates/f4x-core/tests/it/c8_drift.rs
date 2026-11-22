//! Class 8: a change moves the game without being seen. Short scripted plays on the fresh saves are
//! digested at their checkpoints (`check::digest`: the saved text cut at the fields of the snapshot, each
//! hashed) and the digests are pinned in `tests/pins/core_drift.txt`. A rule change that moves a game fails
//! here, naming the plays and the fields that moved. The pins are no rule: they record what the game
//! does today. When the move is meant, regenerate them and review the diff of the file:
//!
//! `F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_`
//!
//! The long plays that need `new_game` and the AI are in `f4x-ai/tests/it`.

use crate::support::{busy_round, commands, text, worlds};
use f4x_core::check::digest;
use f4x_core::game::{Command, Game};
use std::collections::BTreeMap;

const PINS: &str = "tests/pins/core_drift.txt";

fn pins_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PINS)
}

/// A play: its name, the rounds at whose end the game is digested, and the way it goes on one round.
type Round = fn(&mut Game);

fn walking(g: &mut Game) {
    busy_round(g, 6);
}

fn ending_turns(g: &mut Game) {
    g.apply(Command::EndTurn);
}

const PLAYS: [(&str, &[usize], Round); 2] = [
    ("walking", &[1, 5, 15], walking),
    ("turns", &[10, 20, 40], ending_turns),
];

/// The line of a game at a checkpoint: `field=hash` for each field of its digest.
fn line(g: &Game) -> String {
    digest(g)
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every checkpoint of every play on every world, as `world play@round` and its line.
fn today() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (world, fresh) in worlds() {
        let world = world.replace(' ', "").replace(',', "_");
        for (play, checkpoints, step) in PLAYS {
            let mut g = crate::support::worlds()
                .into_iter()
                .find(|(n, _)| n.replace(' ', "").replace(',', "_") == world)
                .expect("the world")
                .1;
            let last = *checkpoints.last().expect("a checkpoint");
            for round in 1..=last {
                step(&mut g);
                if checkpoints.contains(&round) {
                    out.insert(format!("{world} {play}@{round}"), line(&g));
                }
            }
        }
        // Every command of the sweep, then three turns: what the commands changed, and what the turns made of it.
        let mut g = fresh;
        for command in commands(&g) {
            g.apply(command);
        }
        out.insert(format!("{world} commands@all"), line(&g));
        for _ in 0..3 {
            g.apply(Command::EndTurn);
        }
        out.insert(format!("{world} commands@+3turns"), line(&g));
    }
    out
}

fn read_pins() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(pins_path()).unwrap_or_default();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn fields(line: &str) -> BTreeMap<&str, &str> {
    line.split(' ').filter_map(|f| f.split_once('=')).collect()
}

#[test]
fn the_scripted_plays_end_where_they_ended() {
    let now = today();
    if std::env::var_os("F4X_REFRESH_PINS").is_some() {
        let mut out = String::from(
            "# Digests of the scripted plays of tests/it/c8_drift.rs: `world play@round: field=hash ...`.\n\
             # Regenerate on purpose: F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_\n",
        );
        for (k, v) in &now {
            out.push_str(&format!("{k}: {v}\n"));
        }
        std::fs::write(pins_path(), out).expect("the pins are written");
        return;
    }
    let pinned = read_pins();
    assert!(
        !pinned.is_empty(),
        "{PINS} is missing or empty: F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_"
    );
    let mut moved = Vec::new();
    for (key, line) in &now {
        match pinned.get(key) {
            None => moved.push(format!("{key}: not pinned")),
            Some(was) if was != line => {
                let (a, b) = (fields(was), fields(line));
                let changed: Vec<&str> = b
                    .iter()
                    .filter(|(k, v)| a.get(*k) != Some(v))
                    .map(|(k, _)| *k)
                    .chain(a.keys().filter(|k| !b.contains_key(*k)).copied())
                    .collect();
                moved.push(format!("{key}: moved in {}", changed.join(", ")));
            }
            Some(_) => {}
        }
    }
    for key in pinned.keys().filter(|k| !now.contains_key(*k)) {
        moved.push(format!("{key}: pinned but no longer played"));
    }
    assert!(
        moved.is_empty(),
        "the game moved ({} of {} checkpoints). If that is meant, regenerate and review the diff of {PINS}:\n\
         F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_\n{}",
        moved.len(),
        now.len(),
        moved.join("\n")
    );
}

#[test]
fn the_digest_is_the_same_for_the_same_game_and_names_what_differs() {
    let (_, g) = worlds().remove(0);
    let copy = f4x_core::game::save::Snapshot::from_text(&text(&g))
        .expect("reads")
        .restore(crate::support::content())
        .expect("restores");
    assert_eq!(digest(&g), digest(&copy));
    let names: Vec<String> = digest(&g).into_iter().map(|(k, _)| k).collect();
    for wanted in ["seed", "draws", "turn", "units", "cities", "players", "map"] {
        assert!(
            names.iter().any(|n| n == wanted),
            "no field {wanted}: {names:?}"
        );
    }
    // One turn moves the clock, the random generator and the units' moves, not the settings or the seed.
    let mut later = copy;
    later.apply(Command::EndTurn);
    let (a, b) = (digest(&g), digest(&later));
    let moved: Vec<&str> = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| x != y)
        .map(|(x, _)| x.0.as_str())
        .collect();
    assert!(
        moved.contains(&"turn") && moved.contains(&"units"),
        "{moved:?}"
    );
    assert!(
        !moved.contains(&"seed") && !moved.contains(&"settings"),
        "{moved:?}"
    );
}
