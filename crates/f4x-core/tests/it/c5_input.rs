//! Class 5: `apply` takes whatever a client, a script or an AI sends. Bad input is refused with a
//! reason, never a panic, and a refusal changes nothing (class 1 holds for all of it).

use crate::support::{commands, content, text, worlds};
use f4x_core::map::{Plane, Position};
use f4x_core::world::save::Snapshot;
use f4x_core::world::{Command, Event, Rejection, World};

/// Apply `command`; a panic names the command.
fn apply(w: &mut World, command: Command, what: &str) -> Vec<Event> {
    let label = format!("{command:?}");
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.apply(command)))
        .unwrap_or_else(|_| panic!("{what}: {label} panicked"))
}

fn refused(events: &[Event]) -> bool {
    events.iter().any(|e| matches!(e, Event::Rejected(_)))
}

#[test]
fn nothing_can_be_done_once_the_game_is_over() {
    use f4x_core::world::endgame::{EndKind, GameOver};
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let list = commands(&w);
        let winner = w
            .player_ids()
            .into_iter()
            .find(|&p| w.wizard(p).is_some())
            .expect("a wizard");
        w.game_over = Some(GameOver {
            winner,
            kind: EndKind::Conquest,
            turn: w.turn,
        });
        let before = text(&w);
        for command in list {
            let label = format!("{command:?}");
            let events = apply(&mut w, command, &name);
            assert_eq!(
                events.as_slice(),
                [Event::Rejected(Rejection::GameOver)],
                "{name}: {label} after the game ended"
            );
        }
        assert_eq!(text(&w), before, "{name}");
        assert_eq!(
            apply(&mut w, Command::EndTurn, &name),
            [Event::Rejected(Rejection::GameOver)]
        );
    }
}

#[test]
fn commands_naming_units_that_are_gone_are_refused() {
    // One world in the normal run, every other one in the deep run.
    let every = if f4x_def::testdir::deep() { 2 } else { 4 };
    for (name, mut w) in worlds().into_iter().step_by(every) {
        let list = commands(&w);
        for u in w.units().all_units() {
            w.scenario_remove_unit(u);
        }
        assert!(w.units().all_units().is_empty(), "{name}: units are left");
        let mut seen = 0;
        for command in list {
            let label = format!("{command:?}");
            let before = text(&w);
            let events = apply(&mut w, command, &name);
            if refused(&events) {
                seen += 1;
                assert_eq!(
                    text(&w),
                    before,
                    "{name}: {label} was refused, and changed the game"
                );
            }
        }
        assert!(seen > 100, "{name}: only {seen} refusals");
    }
}

#[test]
fn numbers_at_the_ends_of_their_range_are_refused_or_handled() {
    use f4x_core::rules::{SpellId, UnitId};
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let wizard = w
            .player_ids()
            .into_iter()
            .find(|&p| w.wizard(p).is_some())
            .expect("a wizard");
        let city = w
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.1.owner == wizard)
            .expect("a city")
            .0;
        let at = w
            .economy()
            .cities()
            .into_iter()
            .find(|c| c.1.owner == wizard)
            .expect("a city")
            .2;
        let spell = SpellId::new(w.content().rules.spells.defs()[0].num);
        let unit = UnitId::new(w.content().rules.units.defs()[0].num);
        let extremes = vec![
            Command::SetFarmers {
                city,
                farmers: u32::MAX,
            },
            Command::SetTaxRate {
                player: wizard,
                rate: u8::MAX,
            },
            Command::SetPowerSplit {
                player: wizard,
                mana: u8::MAX,
                research: u8::MAX,
                skill: u8::MAX,
            },
            Command::Alchemy {
                player: wizard,
                amount: i32::MIN,
                gold_to_mana: true,
            },
            Command::Alchemy {
                player: wizard,
                amount: i32::MAX,
                gold_to_mana: false,
            },
            Command::CastSpell {
                player: wizard,
                spell,
                target: None,
                extra: i32::MIN,
            },
            Command::CastSpell {
                player: wizard,
                spell,
                target: None,
                extra: i32::MAX,
            },
            Command::AcceptOffer {
                player: wizard,
                index: usize::MAX,
                name: None,
                at: None,
            },
            Command::RejectOffer {
                player: wizard,
                index: usize::MAX,
            },
            Command::FoundCity {
                owner: wizard,
                pos: Position {
                    x: u32::MAX,
                    y: u32::MAX,
                    plane: Plane::Myrror,
                },
                name: String::new(),
                race: None,
                population: u32::MAX,
            },
            Command::FoundCity {
                owner: wizard,
                pos: at,
                name: "x".repeat(100_000),
                race: None,
                population: u32::MAX,
            },
            Command::PlaceFortress {
                player: wizard,
                pos: at,
                name: String::new(),
                race: None,
                population: u32::MAX,
            },
            Command::RenameCity {
                city,
                name: "y".repeat(100_000),
            },
            Command::CreateUnit {
                owner: wizard,
                pos: Position {
                    x: u32::MAX,
                    y: 0,
                    plane: Plane::Arcanus,
                },
                unit,
            },
            Command::MoveStack {
                units: Vec::new(),
                to: Position {
                    x: u32::MAX,
                    y: u32::MAX,
                    plane: Plane::Arcanus,
                },
            },
        ];
        for command in extremes {
            let label = format!("{command:?}");
            let before = text(&w);
            let events = apply(&mut w, command, &name);
            let label: String = label.chars().take(120).collect();
            if refused(&events) {
                assert_eq!(
                    text(&w),
                    before,
                    "{name}: {label} was refused, and changed the game"
                );
            }
            // Accepted or not, what is left is a world that holds.
            let bad = f4x_core::check::world_problems(&w);
            assert!(bad.is_empty(), "{name}: after {label}: {bad:#?}");
        }
    }
}

#[test]
fn a_game_with_no_map_and_no_players_takes_every_command_without_a_panic() {
    let (_, full) = worlds().remove(0);
    let list = commands(&full);
    let mut empty = World::new(1);
    empty.set_content(content());
    for command in list {
        apply(&mut empty, command, "an empty game");
    }
    for _ in 0..3 {
        apply(&mut empty, Command::EndTurn, "an empty game");
    }
    // What was left is still a game that saves and loads.
    let again = Snapshot::from_text(&text(&empty))
        .expect("reads")
        .restore(content());
    assert!(again.is_ok(), "{:?}", again.err());
}
