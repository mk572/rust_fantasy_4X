//! Class 9, the movement helper (`unit/task_walk.rs`): units set out for where their tasks send
//! them, and a check gives up what has no target any more.

use crate::support::{state, worlds};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::rules::UnitId;
use f4x_core::unit::task::{Change, Role};
use f4x_core::unit::task_walk::Check;
use f4x_core::world::serial::Serial;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection};

/// A spearman of `owner` in its first city.
fn create(w: &mut Table, owner: PlayerId) -> (hecs::Entity, Serial) {
    let pos = w.economy().cities_of(owner)[0].1;
    w.apply(Command::CreateUnit {
        owner,
        pos,
        unit: UnitId::new(104 /* high_men_spearmen */),
    });
    let unit = *w.units().all_units().last().expect("the new unit");
    (unit, w.units().serial_of(unit))
}

fn claim(w: &mut Table, player: PlayerId, serial: Serial, role: Role, to: Option<Position>) {
    let change = Change::Claim { serial, role, to };
    assert_eq!(w.apply(Command::Task { player, change }), Vec::new());
}

/// The squares the unit can walk to from where it stands, nearest first, its own left out and
/// those somebody or something holds.
fn walkable(w: &Table, unit: hecs::Entity) -> Vec<Position> {
    let here = w.units().unit_position(unit).expect("a square");
    let mut squares: Vec<Position> = w
        .reachable_squares(&[unit])
        .into_iter()
        .filter(|&p| p != here && w.units().units_at(p).is_empty())
        .filter(|&p| w.economy().city_at(p).is_none() && w.economy().lair_at(p).is_none())
        .collect();
    squares.sort_by_key(|&p| {
        (
            p.x.abs_diff(here.x).max(p.y.abs_diff(here.y)),
            p.plane as u8,
            p.y,
            p.x,
        )
    });
    squares
}

fn walk(w: &mut Table, player: PlayerId, units: Vec<hecs::Entity>) -> Vec<Event> {
    w.apply(Command::WalkTasks {
        player,
        units,
        door: None,
    })
}

fn check(w: &mut Table, player: PlayerId, check: Check) -> Vec<Event> {
    w.apply(Command::CheckTasks {
        player,
        check,
        door: None,
    })
}

/// Whether the unit stands on `to` or has set out from `from`.
fn set_out(w: &Table, unit: hecs::Entity, from: Position) -> bool {
    w.unit_has_order(unit) || w.units().unit_position(unit) != Some(from)
}

#[test]
fn a_unit_sets_out_for_where_its_task_sends_it_and_one_without_a_target_is_refused() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (unit, serial) = create(&mut w, p);
        let from = w.units().unit_position(unit).expect("a square");
        let before = state(&w);
        assert!(
            matches!(
                walk(&mut w, p, vec![unit]).as_slice(),
                [Event::Rejected(Rejection::Move(_))]
            ),
            "{name}: no task"
        );
        assert_eq!(state(&w), before, "{name}");
        let to = walkable(&w, unit)[0];
        for role in [Role::Reinforce, Role::March] {
            let (unit, serial) = create(&mut w, p);
            claim(&mut w, p, serial, role, Some(to));
            walk(&mut w, p, vec![unit]);
            assert!(set_out(&w, unit, from), "{name}: {role:?}");
            w.cheat_remove_unit(unit);
        }
        claim(&mut w, p, serial, Role::Scout, Some(to));
        assert_eq!(w.task_destination(unit), Some(to), "{name}");
    }
}

#[test]
fn an_escort_and_a_unit_resting_with_a_healer_are_sent_to_where_the_other_stands() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (charge, charge_serial) = create(&mut w, p);
        let there = walkable(&w, charge)[0];
        w.cheat_move_unit(charge, there);
        for role in [Role::Escort, Role::Rest] {
            let (unit, serial) = create(&mut w, p);
            let from = w.units().unit_position(unit).expect("a square");
            claim(&mut w, p, serial, role, Some(from));
            if role == Role::Rest {
                // Resting where it is until it is tied to the other.
                assert_eq!(w.task_destination(unit), Some(from), "{name}");
            }
            let change = Change::Tie {
                serial,
                with: charge_serial,
            };
            w.apply(Command::Task { player: p, change });
            assert_eq!(w.task_destination(unit), Some(there), "{name}: {role:?}");
            walk(&mut w, p, vec![unit]);
            assert!(set_out(&w, unit, from), "{name}: {role:?}");
            w.cheat_remove_unit(unit);
        }
    }
}

#[test]
fn the_check_of_the_escorts_sends_one_that_fell_behind_after_its_charge() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (charge, charge_serial) = create(&mut w, p);
        let there = walkable(&w, charge)[0];
        w.cheat_move_unit(charge, there);
        let (escort, serial) = create(&mut w, p);
        let from = w.units().unit_position(escort).expect("a square");
        claim(&mut w, p, serial, Role::Escort, None);
        let change = Change::Tie {
            serial,
            with: charge_serial,
        };
        w.apply(Command::Task { player: p, change });
        check(&mut w, p, Check::Escorts);
        assert!(set_out(&w, escort, from), "{name}");
        assert!(w.player_tasks(p).roles.is(serial, Role::Escort), "{name}");
    }
}

#[test]
fn the_check_of_the_missions_gives_up_a_target_that_is_none_and_sends_on_a_lost_walk() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let home = w.economy().cities_of(p)[0].1;
        // A scout bound for a square already explored, a reinforcement for a square with no own
        // city, and a settler whose player holds the site unfit.
        let (scout, scout_serial) = create(&mut w, p);
        claim(&mut w, p, scout_serial, Role::Scout, Some(home));
        let (helper, helper_serial) = create(&mut w, p);
        let field = walkable(&w, helper)[0];
        claim(&mut w, p, helper_serial, Role::Reinforce, Some(field));
        let (_, settler_serial) = create(&mut w, p);
        claim(&mut w, p, settler_serial, Role::Settle, Some(field));
        // A marcher that lost its walk.
        let (marcher, marcher_serial) = create(&mut w, p);
        let from = w.units().unit_position(marcher).expect("a square");
        // A square next to its objective counts as arrived, so the march goes farther.
        let far = *walkable(&w, marcher)
            .iter()
            .find(|&&q| {
                PlayerView::new(&w, p)
                    .distance(from, q)
                    .is_some_and(|d| d > 1)
            })
            .expect("a square two away");
        claim(&mut w, p, marcher_serial, Role::March, Some(far));
        check(
            &mut w,
            p,
            Check::Missions {
                unfit_sites: vec![settler_serial],
            },
        );
        let tasks = w.player_tasks(p).clone();
        for serial in [scout_serial, helper_serial, settler_serial] {
            assert!(
                tasks.roles.is(serial, Role::AwaitingOrders),
                "{name}: {:?}",
                tasks.roles.get(serial)
            );
        }
        assert!(!w.unit_has_order(scout), "{name}");
        assert!(tasks.roles.is(marcher_serial, Role::March), "{name}");
        assert!(set_out(&w, marcher, from), "{name}");
    }
}
