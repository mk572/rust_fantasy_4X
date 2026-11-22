//! Three limits on units meet on a square: the battle side (a setting, met when the battle starts), the
//! square (a setting, met when units walk in: twice as high on a tower) and the sanity limit (a hard
//! ceiling on everything that puts units there). A unit that is hired, summoned or built is never
//! refused for a full square; only the sanity limit stops it.

use crate::support::worlds;
use f4x_core::game::{Command, Event, LossCause};
use f4x_core::map::Position;
use f4x_core::unit::TILE_SANITY_LIMIT;

fn wizard_and_city(g: &f4x_core::game::Game) -> (f4x_core::player::PlayerId, Position) {
    let wizard = g
        .player_ids()
        .into_iter()
        .find(|&p| g.wizard(p).is_some())
        .expect("a wizard");
    let at = g
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner == wizard)
        .expect("a city")
        .2;
    (wizard, at)
}

/// Units that appear on a square are let in past the walking limit, up to the sanity limit.
#[test]
fn units_put_on_a_square_are_not_refused_for_the_walking_limit() {
    let (_, mut g) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&g);
    let unit = f4x_core::rules::UnitId::new(g.content().rules.units.defs()[0].num);
    let walking = g.tile_limit(at);
    let before = g.units().units_at(at).len();
    for _ in 0..(walking + 5) {
        let events = g.apply(Command::CreateUnit {
            owner: wizard,
            pos: at,
            unit,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
    }
    assert_eq!(g.units().units_at(at).len(), before + walking + 5);
    assert!(f4x_core::check::world_problems(&g).is_empty());
}

/// Past the sanity limit the cheapest units go, with a message each, and the square is back at the
/// limit.
#[test]
fn the_sanity_limit_disbands_the_cheapest_unit() {
    let (_, mut g) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&g);
    let mut kinds: Vec<_> = g
        .content()
        .rules
        .units
        .defs()
        .iter()
        .filter(|d| d.cost > 0)
        .collect();
    kinds.sort_by_key(|d| d.cost);
    let (cheap, dear) = (kinds[0], kinds[kinds.len() - 1]);
    assert!(
        cheap.cost < dear.cost,
        "the content has units of two prices"
    );
    let (cheap, dear) = (
        f4x_core::rules::UnitId::new(cheap.num),
        f4x_core::rules::UnitId::new(dear.num),
    );
    let room = TILE_SANITY_LIMIT.saturating_sub(g.units().units_at(at).len());
    // Fill the square with dear units, then add cheap ones past the limit one at a time.
    for _ in 0..room {
        g.apply(Command::CreateUnit {
            owner: wizard,
            pos: at,
            unit: dear,
        });
    }
    assert_eq!(g.units().units_at(at).len(), TILE_SANITY_LIMIT);
    let events = g.apply(Command::CreateUnit {
        owner: wizard,
        pos: at,
        unit: cheap,
    });
    let lost: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::UnitLost {
                player,
                unit,
                cause: LossCause::Crowded,
            } => Some((*player, *unit)),
            _ => None,
        })
        .collect();
    assert_eq!(lost, vec![(wizard, cheap)], "{events:?}");
    assert_eq!(g.units().units_at(at).len(), TILE_SANITY_LIMIT);
    assert!(f4x_core::check::world_problems(&g).is_empty());
}

/// A tower takes twice the walking limit of any other square; no other square does.
#[test]
fn a_tower_takes_twice_the_walking_limit() {
    let (_, g) = worlds().remove(0);
    let map = g.map().expect("a map");
    let (w, h) = (map.width(), map.height());
    let mut seen = std::collections::BTreeMap::<usize, usize>::new();
    for plane in [f4x_core::map::Plane::Arcanus, f4x_core::map::Plane::Myrror] {
        for y in 0..h {
            for x in 0..w {
                *seen
                    .entry(g.tile_limit(Position { x, y, plane }))
                    .or_default() += 1;
            }
        }
    }
    let limits: Vec<usize> = seen.keys().copied().collect();
    assert_eq!(
        limits,
        vec![g.max_tile(), 2 * g.max_tile()],
        "the limits that are in use: {seen:?}"
    );
}

/// The sanity limit looks only at squares where a unit was just put, so every way of putting one there
/// must be watched. Whatever the commands of the sweep do, no square is left over the limit (the game
/// itself asserts the same after each command in a debug build).
#[test]
fn no_command_leaves_a_square_over_the_sanity_limit() {
    use crate::support::{busy_round, commands};
    for (name, mut g) in worlds().into_iter().step_by(2) {
        // Crowd a square close to the limit, so that what the commands put there tips it over.
        let (wizard, at) = wizard_and_city(&g);
        let unit = f4x_core::rules::UnitId::new(g.content().rules.units.defs()[0].num);
        while g.units().units_at(at).len() < TILE_SANITY_LIMIT {
            g.apply(Command::CreateUnit {
                owner: wizard,
                pos: at,
                unit,
            });
        }
        for command in commands(&g) {
            let label = format!("{command:?}");
            g.apply(command);
            assert!(
                g.crowded_anywhere().is_empty(),
                "{name}: after {label}: {:?}",
                g.crowded_anywhere()
            );
        }
        for _ in 0..5 {
            busy_round(&mut g, 200);
            assert!(g.crowded_anywhere().is_empty(), "{name}");
        }
    }
}

/// A save with a crowded square (written by hand) is put right by the first command.
#[test]
fn a_crowded_save_is_put_right_by_the_next_command() {
    let (_, mut g) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&g);
    let unit = f4x_core::rules::UnitId::new(g.content().rules.units.defs()[0].num);
    // Over the limit without a command (a scenario step), then saved and loaded.
    for _ in 0..TILE_SANITY_LIMIT + 3 {
        g.spawn_unit(wizard, at, unit);
    }
    let text = crate::support::text(&g);
    let mut loaded = f4x_core::game::save::Snapshot::from_text(&text)
        .expect("reads")
        .restore(crate::support::content())
        .expect("loads");
    assert!(!loaded.crowded_anywhere().is_empty());
    loaded.apply(Command::SetTaxRate {
        player: wizard,
        rate: 0,
    });
    assert!(loaded.crowded_anywhere().is_empty());
}
