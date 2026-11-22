//! Class 1: a command the game refuses changes nothing. Not a coin, not a unit, not a draw of the
//! random generator, not a line of the saved text.
//!
//! The commands are made from what the game itself holds (its units, cities, players, squares, spells,
//! buildings) in every pairing that makes sense and some that do not, so most of them are refused, and
//! each refusal is checked against the text of the game from before.

use crate::support::{text, worlds};
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::{BuildingId, SpellId, UnitId};

/// The commands of a game, in a fixed order that does not depend on its entity handles, so the
/// same list is made again from a game that was restored.
fn commands(g: &Game) -> Vec<Command> {
    let rules = &g.content().rules;
    let mut out = Vec::new();

    let mut players: Vec<PlayerId> = g.player_ids();
    players.push(PlayerId(250));
    let wizard = players
        .iter()
        .copied()
        .find(|&p| !g.is_neutral_player(p) && g.wizard(p).is_some())
        .expect("a wizard");

    let all = g.units().all_units();
    let step = (all.len() / 5).max(1);
    let units: Vec<_> = all.iter().copied().step_by(step).take(6).collect();
    let cities: Vec<_> = g
        .economy()
        .cities()
        .into_iter()
        .map(|c| c.0)
        .take(4)
        .collect();

    let map = g.map().expect("a map");
    let mut squares: Vec<Position> = Vec::new();
    // The squares that are not an ordinary neighbour come first, so that cutting the list keeps them:
    // a corner, a square off the east edge, one off the south edge.
    squares.push(Position {
        x: 0,
        y: 0,
        plane: Plane::Arcanus,
    });
    squares.push(Position {
        x: map.width() + 3,
        y: 1,
        plane: Plane::Arcanus,
    });
    squares.push(Position {
        x: 1,
        y: map.height() + 3,
        plane: Plane::Myrror,
    });
    for &u in &units {
        if let Some(at) = g.units().unit_position(u) {
            squares.push(at);
            squares.extend(crate::support::around(g, at).into_iter().take(3));
            squares.push(Position {
                plane: if at.plane == Plane::Arcanus {
                    Plane::Myrror
                } else {
                    Plane::Arcanus
                },
                ..at
            });
        }
    }
    squares.dedup();
    squares.truncate(10);

    let spells: Vec<SpellId> = rules
        .spells
        .defs()
        .iter()
        .map(|d| SpellId::new(d.num))
        .collect();
    let buildings: Vec<BuildingId> = rules
        .buildings
        .defs()
        .iter()
        .map(|d| BuildingId::new(d.num))
        .collect();
    let unit_ids: Vec<UnitId> = rules
        .units
        .defs()
        .iter()
        .step_by(7)
        .map(|d| UnitId::new(d.num))
        .collect();

    for &p in &players {
        out.push(Command::StartPlayerTurn { player: p });
        out.push(Command::EndPlayerTurn { player: p });
        out.push(Command::Resign { player: p });
        out.push(Command::CancelCast { player: p });
        for rate in [0u8, 3, 255] {
            out.push(Command::SetTaxRate { player: p, rate });
        }
        out.push(Command::SetGrandVizier {
            player: p,
            on: true,
        });
        for (m, r, s) in [(0u8, 0u8, 0u8), (100, 0, 0), (50, 50, 50), (255, 255, 255)] {
            out.push(Command::SetPowerSplit {
                player: p,
                mana: m,
                research: r,
                skill: s,
            });
        }
        for (amount, gold_to_mana) in [(0, true), (5, true), (-5, false), (i32::MAX, true)] {
            out.push(Command::Alchemy {
                player: p,
                amount,
                gold_to_mana,
            });
        }
        out.push(Command::ReleaseUpkeep {
            player: p,
            items: Vec::new(),
        });
        out.push(Command::ReleaseTroops {
            player: p,
            units: units.clone(),
            scrap: Vec::new(),
        });
        out.push(Command::AcceptOffer {
            player: p,
            index: 0,
            name: None,
        });
        out.push(Command::RejectOffer {
            player: p,
            index: 0,
        });
        for &u in &units {
            for &s in spells.iter().step_by(40) {
                out.push(Command::TurnOffUnitSpell {
                    player: p,
                    unit: u,
                    spell: s,
                });
            }
        }
        for &c in &cities {
            out.push(Command::RazeCity { player: p, city: c });
        }
    }
    for &s in &spells {
        out.push(Command::SetResearch {
            player: wizard,
            spell: s,
        });
        out.push(Command::CastSpell {
            player: wizard,
            spell: s,
            target: None,
            extra: 0,
        });
        out.push(Command::TurnOffSpell {
            player: wizard,
            spell: s,
        });
    }
    out.push(Command::ResolveCast {
        player: wizard,
        target: f4x_core::rules::exec::Target::Player(wizard),
    });

    for &u in &units {
        out.push(Command::ClearOrders { units: vec![u] });
        out.push(Command::Purify { units: vec![u] });
        out.push(Command::Meld { units: vec![u] });
        out.push(Command::ShiftPlane { units: vec![u] });
        out.push(Command::DismissUnit { unit: u });
        out.push(Command::SetBridge { unit: u, on: true });
        out.push(Command::SettleOutpost {
            unit: u,
            name: "Nowhere".into(),
        });
        for standby in [
            None,
            Some(f4x_core::unit::Standby::Done),
            Some(f4x_core::unit::Standby::Patrol),
        ] {
            out.push(Command::SetStandby {
                units: vec![u],
                standby,
            });
        }
        for &to in &squares {
            out.push(Command::MoveStack { units: vec![u], to });
            out.push(Command::GoTo { units: vec![u], to });
            out.push(Command::EnterSite { units: vec![u], to });
            out.push(Command::EarthGate { units: vec![u], to });
            out.push(Command::BuildRoad {
                units: vec![u],
                path: vec![to],
            });
        }
    }
    out.push(Command::MoveStack {
        units: Vec::new(),
        to: squares[0],
    });
    out.push(Command::MoveStack {
        units: units.clone(),
        to: squares[0],
    });
    for pair in units.windows(2) {
        out.push(Command::BeginBattle {
            attackers: vec![pair[0]],
            defenders: vec![pair[1]],
        });
        out.push(Command::AutoResolve {
            attackers: vec![pair[0]],
            defenders: vec![pair[1]],
            mana_budget: None,
        });
    }
    out.push(Command::BeginBattle {
        attackers: Vec::new(),
        defenders: units.clone(),
    });

    for &p in &players {
        for &at in &squares {
            out.push(Command::FoundCity {
                owner: p,
                pos: at,
                name: "Newtown".into(),
                race: None,
                population: 1,
            });
            out.push(Command::PlaceFortress {
                player: p,
                pos: at,
                name: "Keep".into(),
                race: None,
                population: 1,
            });
            for &u in unit_ids.iter().take(3) {
                out.push(Command::CreateUnit {
                    owner: p,
                    pos: at,
                    unit: u,
                });
            }
        }
    }
    for &c in &cities {
        out.push(Command::BuyProduction { city: c });
        out.push(Command::RenameCity {
            city: c,
            name: String::new(),
        });
        out.push(Command::SetAutoBuild {
            city: c,
            plan: None,
        });
        for farmers in [0u32, 1, 9999] {
            out.push(Command::SetFarmers { city: c, farmers });
        }
        for &b in buildings.iter().step_by(3) {
            out.push(Command::SellBuilding {
                city: c,
                building: b,
            });
            out.push(Command::SetProduction {
                city: c,
                item: f4x_core::city::production::ProductionItem::Building(b),
            });
        }
        for &u in &unit_ids {
            out.push(Command::SetProduction {
                city: c,
                item: f4x_core::city::production::ProductionItem::Unit(u),
            });
        }
        out.push(Command::SetProduction {
            city: c,
            item: f4x_core::city::production::ProductionItem::TradeGoods,
        });
        out.push(Command::SetProduction {
            city: c,
            item: f4x_core::city::production::ProductionItem::Housing,
        });
        for &s in spells.iter().step_by(40) {
            out.push(Command::TurnOffCitySpell {
                player: wizard,
                city: c,
                spell: s,
            });
        }
    }
    out
}

#[test]
fn a_refused_command_changes_nothing() {
    let mut tried = 0;
    let mut refused = 0;
    for (name, mut g) in worlds().into_iter().step_by(2) {
        // The game goes on changing as commands are accepted (a unit is dismissed, a city founded), so
        // later commands name things that are gone: a stale handle is one more kind of bad input.
        for command in commands(&g) {
            let label = format!("{command:?}");
            let before = text(&g);
            let events =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.apply(command)))
                    .unwrap_or_else(|_| panic!("{name}: {label} panicked"));
            tried += 1;
            if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
                refused += 1;
                assert_eq!(
                    text(&g),
                    before,
                    "{name}: {label} was refused, and changed the game"
                );
            }
        }
    }
    assert!(
        refused > tried / 4,
        "only {refused} of {tried} commands were refused: the sweep is not reaching the refusals"
    );
}

/// The commands the sweep does not make yet, and why: each is a gap to close, not a decision.
const NOT_TRIED: &[(&str, &str)] = &[
    (
        "EndTurn",
        "never refused but at the end of the game (class 5)",
    ),
    ("StartRound", "the master's lifecycle step, never refused"),
    ("EndRound", "the master's lifecycle step, never refused"),
    (
        "Anyway",
        "wraps another command: the sweep needs the confirmation cases",
    ),
    ("Battle", "needs a battle going: its own sweep"),
    ("MoveItem", "needs items in the vault and on heroes"),
    ("CraftItem", "needs a craft and a design"),
    ("ScrapItem", "needs items in the vault"),
    ("SellItem", "needs items in the vault"),
    (
        "Diplomacy",
        "needs meetings and proposals: its own sweep, with the treaties",
    ),
];

/// The variants of `Command`, read from its source.
fn command_names() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/game/command.rs");
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
    let (_, g) = worlds().remove(0);
    let tried: std::collections::HashSet<String> = commands(&g)
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
