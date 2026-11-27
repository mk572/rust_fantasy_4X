//! Class 6, every setting: the game is made again with each rule setting at each value it accepts
//! (read from the saved settings, tried against what the game accepts), and played. Whatever the
//! value, the world holds together turn after turn, a refused command changes nothing, and the game
//! saves and loads as itself. The values are not named here: a bool is flipped, a number is tried
//! over a spread and kept where the game accepts it, a choice takes the variants of its type.

use crate::support::{busy_round, content, state, text, worlds};
use f4x_core::check::world_problems;
use f4x_core::world::save::Snapshot;
use f4x_core::world::testutil::sweep::commands;
use f4x_core::world::testutil::Table;
use f4x_core::world::Event;
use f4x_def::testdir::{deep, sample};
use f4x_master::Game;

/// The `rules: (...)` line of a save, as its pairs `(name, value)`.
fn rule_pairs(full: &str) -> (String, Vec<(String, String)>) {
    let line = full
        .lines()
        .find(|l| l.trim().starts_with("rules: ("))
        .expect("the rule settings");
    let inner = line
        .trim()
        .strip_prefix("rules: (")
        .and_then(|l| l.strip_suffix("),"))
        .expect("a line of pairs");
    let (mut depth, mut start) = (0, 0);
    let mut parts = Vec::new();
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    let pairs = parts
        .into_iter()
        .map(|p| {
            let (k, v) = p.trim().split_once(": ").expect("name: value");
            (k.to_string(), v.to_string())
        })
        .collect();
    (line.to_string(), pairs)
}

/// The variants of the enums of the settings, in the spelling of the save (`snake_case`).
fn choices() -> Vec<Vec<String>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../f4x-def/src/settings.rs");
    let source = std::fs::read_to_string(path).expect("the settings source");
    let mut out = Vec::new();
    for name in ["MeleeVsFliers", "SpellAccess"] {
        let body = source
            .split(&format!("pub enum {name} {{"))
            .nth(1)
            .unwrap_or_else(|| panic!("{name} is not in the settings"));
        let variants: Vec<String> = body
            .lines()
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
            .map(|w| {
                let mut s = String::new();
                for (i, c) in w.chars().enumerate() {
                    if c.is_uppercase() && i > 0 {
                        s.push('_');
                    }
                    s.push(c.to_ascii_lowercase());
                }
                s
            })
            .collect();
        assert!(variants.len() >= 2, "{name}: {variants:?}");
        out.push(variants);
    }
    out
}

/// The other values a setting could take, from the one it has.
fn others(value: &str, choices: &[Vec<String>]) -> Vec<String> {
    match value {
        "true" => vec!["false".into()],
        "false" => vec!["true".into()],
        v if v.starts_with("(human: ") => {
            // A setting per side: both on, both off, each alone.
            [
                "(human: true, ai: true)",
                "(human: false, ai: false)",
                "(human: true, ai: false)",
                "(human: false, ai: true)",
            ]
            .iter()
            .filter(|c| **c != v)
            .map(|c| c.to_string())
            .collect()
        }
        v if v.parse::<f64>().is_ok() => {
            // A spread in the normal run, every number to a hundred in the deep one.
            let tried: Vec<u32> = if deep() {
                (0..=100).chain([255]).collect()
            } else {
                vec![0, 1, 3, 6, 9, 20, 255]
            };
            tried
                .iter()
                .map(|n| n.to_string())
                .filter(|n| n != v)
                .collect()
        }
        v => choices
            .iter()
            .find(|c| c.iter().any(|x| x == v))
            .map(|c| c.iter().filter(|x| *x != v).cloned().collect())
            .unwrap_or_default(),
    }
}

fn refused(events: &[Event]) -> bool {
    events.iter().any(|e| matches!(e, Event::Rejected(_)))
}

/// Play the game under its settings: rounds with the world checked after each, the commands of the
/// sweep with each refusal changing nothing, a save and load that returns the same game.
fn play(w: Table, at: &str) {
    let mut w = Game::from(w);
    // A game made under the setting has no city above the cap; this one was made under another.
    let cap = w.economy().max_city_size();
    for (city, _, _) in w.economy().cities() {
        let size = w.city_population(city).figures;
        if size > cap {
            w.scenario_set_citizens(city, cap);
        }
    }
    for round in 0..sample(4, 2) {
        busy_round(&mut w, 6);
        let bad = world_problems(&w);
        assert!(bad.is_empty(), "{at}, after round {round}: {bad:#?}");
    }
    for command in commands(&w)
        .into_iter()
        .step_by(if deep() { 2 } else { 25 })
    {
        let label: String = format!("{command:?}").chars().take(100).collect();
        let before = state(&w);
        let events = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.command(command)))
            .unwrap_or_else(|_| panic!("{at}: {label} panicked"));
        if refused(&events) {
            let after = state(&w);
            if after != before {
                for (x, y) in before
                    .lines()
                    .zip(after.lines())
                    .filter(|(x, y)| x != y)
                    .take(8)
                {
                    println!("DIFF\n- {x}\n+ {y}");
                }
                println!("EVENTS {events:?}");
            }
            assert_eq!(
                after, before,
                "{at}: {label} was refused and changed the game"
            );
        }
    }
    let bad = world_problems(&w);
    assert!(bad.is_empty(), "{at}, after the commands: {bad:#?}");
    let back = Snapshot::from_text(&text(&w))
        .expect("reads")
        .restore(content())
        .unwrap_or_else(|e| panic!("{at}: the game does not load again: {e}"));
    assert_eq!(text(&back), text(&w), "{at}: a loaded game differs");
}

#[test]
fn the_game_holds_under_every_value_of_every_rule_setting() {
    let (_, w) = worlds().remove(0);
    let full = text(&w);
    let (line, pairs) = rule_pairs(&full);
    assert!(pairs.len() > 30, "only {} settings were read", pairs.len());
    let choices = choices();
    let mut without_value = Vec::new();
    let mut played = 0;
    for (name, value) in &pairs {
        let mut ok = Vec::new();
        for other in others(value, &choices) {
            let edited_line =
                line.replacen(&format!("{name}: {value}"), &format!("{name}: {other}"), 1);
            let edited = full.replacen(&line, &edited_line, 1);
            let loaded = std::panic::catch_unwind(|| {
                Snapshot::from_text(&edited).and_then(|s| s.restore(content()))
            })
            .unwrap_or_else(|_| panic!("loading panicked with {name}: {other}"));
            if let Ok(world) = loaded {
                ok.push((other, world));
            }
        }
        let accepted = ok.len();
        // The ends of what the game accepts, in the normal run; every value in the deep run.
        let last = ok.len().saturating_sub(1);
        for (i, (other, world)) in ok.into_iter().enumerate() {
            if deep() || i == 0 || i == last {
                played += 1;
                play(
                    Table::new(world, w.rng.clone()),
                    &format!(
                        "{name}: {other} (F4X_DEEP=1 tries every value, to find where it breaks)"
                    ),
                );
            }
        }
        if accepted == 0 {
            without_value.push(name.clone());
        }
    }
    assert!(played >= pairs.len(), "only {played} settings were played");
    assert!(
        without_value.is_empty(),
        "the game accepts no other value of {without_value:?}"
    );
}
