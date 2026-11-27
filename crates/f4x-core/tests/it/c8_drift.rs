//! Class 8: a change moves the game without being seen. The commands of the sweep are run on the
//! fresh saves, digested (`check::digest`: the saved text cut at the fields of
//! the snapshot, each hashed) and pinned in `tests/pins/core_drift.txt`. A rule change that moves a game
//! fails here, naming the plays and the fields that moved. The pins are no rule: they record what the game
//! does today. The plays with turns are the game's (`f4x-master/tests/it/drift.rs`). When a move is
//! meant, regenerate the pins and review the diff of the file:
//!
//! `F4X_REFRESH_PINS=1 cargo test -p f4x-core --features testutil --test it c8_`
//!
//! The long plays that need `new_game` and the AI are in `f4x-ai/tests/it`.

use crate::support::{commands, text, worlds};
use f4x_core::check::digest;
use f4x_core::world::testutil::pin;
use f4x_core::world::testutil::Table;
use std::collections::BTreeMap;

const PINS: &str = "tests/pins/core_drift.txt";

/// The line of a world at a checkpoint: its generator, then `field=hash` for each field of its
/// digest.
fn line(w: &Table) -> String {
    let mut parts = vec![format!("seed={}", w.seed()), format!("draws={}", w.draws())];
    parts.extend(digest(w).into_iter().map(|(k, v)| format!("{k}={v}")));
    parts.join(" ")
}

/// Every command of the sweep on every world, as `world play@checkpoint` and its line.
fn today() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (world, mut w) in worlds() {
        let world = world.replace(' ', "").replace(',', "_");
        for command in commands(&w) {
            w.apply(command);
        }
        out.insert(format!("{world} commands@all"), line(&w));
    }
    out
}

#[test]
fn the_scripted_plays_end_where_they_ended() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PINS);
    let header =
        "# Digests of the scripted plays of tests/it/c8_drift.rs: `world play@checkpoint: field=hash ...`.\n\
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
    // A unit more moves the units, not the settings.
    let mut later = Table::new(copy, w.rng.clone());
    let unit = *later.units().all_units().first().expect("a unit");
    let kind = later.units().unit_kind_id(unit).expect("its kind");
    let owner = later.units().unit_owner(unit).expect("its owner");
    let at = later.units().unit_position(unit).expect("its square");
    later.spawn_unit(owner, at, kind);
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
