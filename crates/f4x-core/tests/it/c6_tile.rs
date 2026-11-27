//! Three limits on units meet on a square: the battle side (a setting, met when the battle starts), the
//! square (a setting, met when units walk in: twice as high on a tower) and the sanity limit (a hard
//! ceiling on everything that puts units there). A unit that is hired, summoned or built is never
//! refused for a full square; only the sanity limit stops it.

use crate::support::worlds;
use f4x_core::map::Position;
use f4x_core::unit::TILE_SANITY_LIMIT;
use f4x_core::world::{Command, Event, LossCause};

fn wizard_and_city(w: &f4x_core::world::World) -> (f4x_core::player::PlayerId, Position) {
    let wizard = w
        .player_ids()
        .into_iter()
        .find(|&p| w.wizard(p).is_some())
        .expect("a wizard");
    let at = w
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
    let (_, mut w) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&w);
    let unit = f4x_core::rules::UnitId::new(w.content().rules.units.defs()[0].num);
    let walking = w.tile_limit(at);
    let before = w.units().units_at(at).len();
    for _ in 0..(walking + 5) {
        let events = w.apply(Command::CreateUnit {
            owner: wizard,
            pos: at,
            unit,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
    }
    assert_eq!(w.units().units_at(at).len(), before + walking + 5);
    assert!(f4x_core::check::world_problems(&w).is_empty());
}

/// Past the sanity limit the cheapest units go, with a message each, and the square is back at the
/// limit.
#[test]
fn the_sanity_limit_disbands_the_cheapest_unit() {
    let (_, mut w) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&w);
    let mut kinds: Vec<_> = w
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
    let room = TILE_SANITY_LIMIT.saturating_sub(w.units().units_at(at).len());
    // Fill the square with dear units, then add cheap ones past the limit one at a time.
    for _ in 0..room {
        w.apply(Command::CreateUnit {
            owner: wizard,
            pos: at,
            unit: dear,
        });
    }
    assert_eq!(w.units().units_at(at).len(), TILE_SANITY_LIMIT);
    let events = w.apply(Command::CreateUnit {
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
    assert_eq!(w.units().units_at(at).len(), TILE_SANITY_LIMIT);
    assert!(f4x_core::check::world_problems(&w).is_empty());
}

/// A tower takes twice the walking limit of any other square; no other square does.
#[test]
fn a_tower_takes_twice_the_walking_limit() {
    let (_, w) = worlds().remove(0);
    let map = w.map().expect("a map");
    let (width, h) = (map.width(), map.height());
    let mut seen = std::collections::BTreeMap::<usize, usize>::new();
    for plane in [f4x_core::map::Plane::Arcanus, f4x_core::map::Plane::Myrror] {
        for y in 0..h {
            for x in 0..width {
                *seen
                    .entry(w.tile_limit(Position { x, y, plane }))
                    .or_default() += 1;
            }
        }
    }
    let limits: Vec<usize> = seen.keys().copied().collect();
    assert_eq!(
        limits,
        vec![w.max_tile(), 2 * w.max_tile()],
        "the limits that are in use: {seen:?}"
    );
}

/// A save with a crowded square (written by hand) is put right by the first command.
#[test]
fn a_crowded_save_is_put_right_by_the_next_command() {
    let (_, mut w) = worlds().remove(0);
    let (wizard, at) = wizard_and_city(&w);
    let unit = f4x_core::rules::UnitId::new(w.content().rules.units.defs()[0].num);
    // Over the limit without a command (a scenario step), then saved and loaded.
    for _ in 0..TILE_SANITY_LIMIT + 3 {
        w.spawn_unit(wizard, at, unit);
    }
    let text = crate::support::text(&w);
    let world = f4x_core::world::save::Snapshot::from_text(&text)
        .expect("reads")
        .restore(crate::support::content())
        .expect("loads");
    let mut loaded = f4x_core::world::testutil::Table::new(world, w.rng.clone());
    assert!(!loaded.crowded_anywhere().is_empty());
    loaded.apply(Command::SetTaxRate {
        player: wizard,
        rate: 0,
    });
    assert!(loaded.crowded_anywhere().is_empty());
}

/// A prisoner of a cleared lair is freed where the lair stands, in the winner's service, even when the
/// army that freed it is a full stack: nothing is offered, nothing is refused for room.
#[test]
fn a_freed_prisoner_stands_on_the_square_of_the_lair() {
    use crate::support::around;
    use f4x_core::world::testutil::battle_policy;
    let mut freed = 0;
    for (wizards, seed) in crate::support::WORLDS {
        let name = format!("{wizards} wizards, seed {seed}");
        let base = f4x_core::world::testutil::fresh(wizards, seed).expect("a fresh game");
        let (wizard, _) = wizard_and_city(&base);
        let strong = {
            let mut defs: Vec<_> = base
                .content()
                .rules
                .units
                .defs()
                .iter()
                .filter(|d| d.kind != f4x_core::rules::UnitKind::Hero)
                .collect();
            defs.sort_by_key(|d| std::cmp::Reverse(d.cost));
            f4x_core::rules::UnitId::new(defs[0].num)
        };
        for lair in base.economy().lairs().into_iter().filter(|l| l.prisoner) {
            let mut w = f4x_core::world::testutil::fresh(wizards, seed).expect("a fresh game");
            let Some(from) = around(&w, lair.pos).into_iter().next() else {
                continue;
            };
            let side = w.max_battle_side();
            for _ in 0..side + 1 {
                w.apply(Command::CreateUnit {
                    owner: wizard,
                    pos: from,
                    unit: strong,
                });
            }
            let stack = w.units().units_at(from);
            let heroes = w.units().hero_units(wizard).len();
            let mut events = w.apply(Command::EnterSite {
                units: stack,
                to: lair.pos,
            });
            let mut steps = 0;
            while w.battle().is_some() && steps < 5000 {
                let Some(c) = battle_policy(&w) else { break };
                events.extend(w.apply(Command::Battle(c)));
                steps += 1;
            }
            let loot = events.iter().find_map(|e| match e {
                Event::SiteCleared { loot, .. } => Some(loot.clone()),
                _ => None,
            });
            if loot.is_some_and(|l| l.prisoner) {
                freed += 1;
                let here = w.units().units_at(lair.pos);
                assert_eq!(
                    w.units().hero_units(wizard).len(),
                    heroes + 1,
                    "{name}: the hero serves the winner"
                );
                assert!(
                    here.iter()
                        .any(|&u| w.units().hero_units(wizard).contains(&u)),
                    "{name}: the hero stands on the lair's square"
                );
            }
        }
    }
    assert!(freed > 0, "no prisoner was freed in any world");
}
