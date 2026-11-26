//! Standing orders (`GoTo`) are followed at the start of the turn, group by group. A group whose next
//! square is full waits for the groups that can move, and goes in once they have: a column of big
//! stacks walking one way clears from its head, whatever the order of their creation.

use crate::support::{around, WORLDS};
use f4x_core::rules::UnitId;
use f4x_core::world::testutil::fresh;
use f4x_core::world::{Command, Event};
use f4x_def::half_int::HalfInt;

#[test]
fn a_stack_that_finds_its_way_full_goes_in_once_the_stack_on_it_has_gone_on() {
    let (wizards, seed) = WORLDS[0];
    let base = fresh(wizards, seed).expect("a fresh game");
    let wizard = base
        .player_ids()
        .into_iter()
        .find(|&p| base.wizard(p).is_some())
        .expect("a wizard");
    let a = base
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner == wizard)
        .expect("a city")
        .2;
    let moved = |events: &[Event]| events.iter().any(|e| matches!(e, Event::UnitsMoved { .. }));
    // A unit that walks, a square `b` next to the city and a square `c` beyond it, found by trying.
    let step = |w: &mut f4x_core::world::World, unit: UnitId, from, to| {
        w.apply(Command::CreateUnit {
            owner: wizard,
            pos: from,
            unit,
        });
        let u = *w.units().all_units().last().expect("the new unit");
        moved(&w.apply(Command::MoveStack { units: vec![u], to }))
    };
    let mut found = None;
    'search: for def in base.content().rules.units.defs() {
        let unit = UnitId::new(def.num);
        for b in around(&base, a) {
            let mut probe = fresh(wizards, seed).expect("a fresh game");
            if !step(&mut probe, unit, a, b) {
                continue;
            }
            for c in around(&base, b).into_iter().filter(|&c| c != a) {
                let mut probe = fresh(wizards, seed).expect("a fresh game");
                if step(&mut probe, unit, b, c) {
                    found = Some((unit, b, c));
                    break 'search;
                }
            }
        }
    }
    let (unit, b, c) = found.expect("a unit walks from the city over a square to another");

    let mut w = fresh(wizards, seed).expect("a fresh game");
    let limit = w.settings().rules.max_battle_side;
    w.scenario_change_settings(|s| s.rules.max_tile = limit);
    let before = w.units().units_at(b).len();
    let make = |w: &mut f4x_core::world::World, at, n: usize| -> Vec<_> {
        (0..n)
            .map(|_| {
                w.apply(Command::CreateUnit {
                    owner: wizard,
                    pos: at,
                    unit,
                });
                *w.units().all_units().last().expect("the new unit")
            })
            .collect()
    };
    // The rear stack is the older: it is followed first, and finds the square before it full.
    let rear = make(&mut w, a, 2);
    let head = make(&mut w, b, limit.saturating_sub(before));
    assert_eq!(w.units().units_at(b).len(), limit, "the square is full");
    // Orders given with no moves left stand until the turn starts.
    for &u in rear.iter().chain(&head) {
        w.scenario_set_moves_left(u, HalfInt::ZERO);
    }
    for (units, to) in [(rear.clone(), b), (head.clone(), c)] {
        let events = w.apply(Command::GoTo { units, to });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
    }
    let events = w.apply(Command::EndTurn);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::OrderFailed { .. })),
        "an order failed: the rear stack was not given another try"
    );
    for &u in &head {
        assert_eq!(w.units().unit_position(u), Some(c), "the head stack");
    }
    for &u in &rear {
        assert_eq!(w.units().unit_position(u), Some(b), "the rear stack");
    }
}

#[test]
fn a_stack_that_stays_blocked_is_told_why() {
    let (wizards, seed) = WORLDS[0];
    let mut w = fresh(wizards, seed).expect("a fresh game");
    let wizard = w
        .player_ids()
        .into_iter()
        .find(|&p| w.wizard(p).is_some())
        .expect("a wizard");
    let a = w
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner == wizard)
        .expect("a city")
        .2;
    let unit = UnitId::new(w.content().rules.units.defs()[0].num);
    let limit = w.settings().rules.max_battle_side;
    w.scenario_change_settings(|s| s.rules.max_tile = limit);
    // A neighbour that takes the unit, filled by units that stay (no orders, no moves).
    let mut b = None;
    for to in around(&w, a) {
        let mut probe = fresh(wizards, seed).expect("a fresh game");
        probe.apply(Command::CreateUnit {
            owner: wizard,
            pos: a,
            unit,
        });
        let u = *probe.units().all_units().last().expect("a unit");
        if probe
            .apply(Command::MoveStack { units: vec![u], to })
            .iter()
            .any(|e| matches!(e, Event::UnitsMoved { .. }))
        {
            b = Some(to);
            break;
        }
    }
    let Some(b) = b else { return };
    let have = w.units().units_at(b).len();
    for _ in have..limit {
        w.apply(Command::CreateUnit {
            owner: wizard,
            pos: b,
            unit,
        });
    }
    w.apply(Command::CreateUnit {
        owner: wizard,
        pos: a,
        unit,
    });
    let walker = *w.units().all_units().last().expect("a unit");
    w.scenario_set_moves_left(walker, HalfInt::ZERO);
    w.apply(Command::GoTo {
        units: vec![walker],
        to: b,
    });
    let events = w.apply(Command::EndTurn);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OrderFailed { units, .. } if units.contains(&walker))),
        "the blocked stack was not told"
    );
    assert_eq!(w.units().unit_position(walker), Some(a));
}
