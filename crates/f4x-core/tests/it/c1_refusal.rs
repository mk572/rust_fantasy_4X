//! Class 1: a command the game refuses changes nothing. Not a coin, not a unit, not a draw of the
//! random generator, not a line of the saved text.
//!
//! The commands are made from what the game itself holds (its units, cities, players, squares, spells,
//! buildings) in every pairing that makes sense and some that do not, so most of them are refused, and
//! each refusal is checked against the text of the game from before.

use crate::support::{commands, state, worlds};
use f4x_core::world::Event;

/// Try every command of the sweep on each world and check each refusal. Returns how many commands
/// were tried and how many were refused.
fn sweep(every: usize) -> (usize, usize) {
    let mut tried = 0;
    let mut refused = 0;
    // The item commands: (refused, accepted).
    let mut item = (0usize, 0usize);
    for (name, mut w) in worlds().into_iter().step_by(every) {
        // Something in the vaults, so that the item commands are refused for what they ask and not
        // for an empty place.
        let items: Vec<_> = w
            .content()
            .rules
            .items
            .defs()
            .iter()
            .step_by(37)
            .take(3)
            .map(f4x_core::item::Item::from_def)
            .collect();
        for p in w.wizards() {
            for item in &items {
                w.give_item(p, item.clone());
            }
        }
        // The game goes on changing as commands are accepted (a unit is dismissed, a city founded), so
        // later commands name things that are gone: a stale handle is one more kind of bad input.
        for command in commands(&w) {
            let label = format!("{command:?}");
            let before = state(&w);
            let events =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.apply(command)))
                    .unwrap_or_else(|_| panic!("{name}: {label} panicked"));
            tried += 1;
            let rejected = events.iter().any(|e| matches!(e, Event::Rejected(_)));
            if ["MoveItem", "ScrapItem", "SellItem", "CraftItem"]
                .iter()
                .any(|n| label.starts_with(n))
            {
                if rejected {
                    item.0 += 1;
                } else {
                    item.1 += 1;
                }
            }
            if rejected {
                refused += 1;
                assert_eq!(
                    state(&w),
                    before,
                    "{name}: {label} was refused, and changed the game"
                );
            }
        }
    }
    assert!(
        item.0 > 0 && item.1 > 0,
        "the item commands were refused {} times and accepted {} times: the sweep needs both",
        item.0,
        item.1
    );
    (tried, refused)
}

#[test]
fn a_refused_command_changes_nothing() {
    // One of the four worlds in the normal run, all four in the deep one.
    let every = if f4x_def::testdir::deep() { 1 } else { 4 };
    let (tried, refused) = sweep(every);
    assert!(
        refused > tried / 4,
        "only {refused} of {tried} commands were refused: the sweep is not reaching the refusals"
    );
}

/// The commands the sweep does not make yet, and why: each is a gap to close, not a decision.
const NOT_TRIED: &[(&str, &str)] = &[
    (
        "Anyway",
        "wraps another command: the sweep needs the confirmation cases",
    ),
    ("Battle", "needs a battle going: its own sweep"),
];

/// The variants of `Command`, read from its source.
fn command_names() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/world/command.rs");
    let source = std::fs::read_to_string(path).expect("the command source");
    let body = source.split("pub enum Command {").nth(1).expect("the enum");
    body.lines()
        .take_while(|l| !l.starts_with('}'))
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .filter_map(|l| {
            let word: String = l
                .trim()
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            word.chars()
                .next()
                .filter(|c| c.is_uppercase())
                .map(|_| word)
        })
        .collect()
}

#[test]
fn every_command_is_in_the_sweep_or_named_as_missing() {
    let (_, w) = worlds().remove(0);
    let tried: std::collections::HashSet<String> = commands(&w)
        .iter()
        .map(|c| {
            format!("{c:?}")
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect()
        })
        .collect();
    let named: std::collections::HashSet<&str> = NOT_TRIED.iter().map(|n| n.0).collect();
    let names = command_names();
    assert!(
        names.len() > 40,
        "the command source was not read: {names:?}"
    );
    for name in &names {
        assert!(
            tried.contains(name) != named.contains(name.as_str()),
            "{name}: it is {} in the sweep and {} on the list of missing ones",
            if tried.contains(name) { "" } else { "not" },
            if named.contains(name.as_str()) {
                ""
            } else {
                "not"
            }
        );
    }
    let gone: Vec<_> = named
        .iter()
        .filter(|n| !names.iter().any(|x| x == *n))
        .collect();
    assert!(
        gone.is_empty(),
        "{gone:?} are on the list of missing commands but are no commands"
    );
}

#[test]
fn a_refused_move_leaves_the_standby_of_the_unit() {
    use crate::support::around;
    use f4x_core::unit::Standby;
    let mut refused = 0;
    for (name, mut w) in worlds() {
        for u in w.units().all_units().into_iter().take(40) {
            let Some(at) = w.units().unit_position(u) else {
                continue;
            };
            for to in around(&w, at) {
                w.apply(f4x_core::world::Command::SetStandby {
                    units: vec![u],
                    standby: Some(Standby::Done),
                });
                let before = state(&w);
                let events = w.apply(f4x_core::world::Command::MoveStack { units: vec![u], to });
                if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
                    refused += 1;
                    assert_eq!(state(&w), before, "{name}: a refused move to {to:?}");
                }
            }
        }
    }
    assert!(refused > 20, "only {refused} moves were refused");
}
