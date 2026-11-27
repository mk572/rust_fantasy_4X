//! Class 9, the unit tasks: what a player changes in its army ledger is kept and saved with the
//! game, dated with the round the game gives, and says nothing of a unit that is not in the army.

use crate::support::{content, state, worlds};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::UnitId;
use f4x_core::unit::task::{Assembly, Change, Crossing, MusterKind, Role, Tasks};
use f4x_core::world::save::Snapshot;
use f4x_core::world::serial::Serial;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event};

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

/// A scout, a muster of one and a gathering, for the units given.
fn changes_for(scout: Serial, gathering: Serial, at: Position) -> Vec<Change> {
    vec![
        Change::Claim {
            serial: scout,
            role: Role::Scout,
            to: Some(at),
        },
        Change::BeginMuster {
            kind: MusterKind::Raid,
            rally: at,
            objective: Some(at),
            serials: vec![gathering],
        },
        Change::SetAssembly(Assembly {
            target: at,
            beside: Some(at),
            rally: None,
        }),
    ]
}

fn change(w: &mut Table, player: PlayerId, change: Change) -> Vec<Event> {
    w.apply(Command::Task { player, change })
}

fn tasks(w: &Table, player: PlayerId) -> Tasks {
    w.player_tasks(player).clone()
}

#[test]
fn each_change_is_kept_for_the_player_and_saved_with_the_game(
) -> Result<(), Box<dyn std::error::Error>> {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        assert_eq!(tasks(&w, p), Tasks::default(), "{name}");
        let (scout, scout_serial) = create(&mut w, p);
        let (_, gathering) = create(&mut w, p);
        let at = w.units().unit_position(scout).expect("a square");
        let moves = w.units().unit_moves_left(scout);
        w.round = 3;
        for c in changes_for(scout_serial, gathering, at) {
            assert_eq!(change(&mut w, p, c), Vec::new(), "{name}");
        }
        let mut wanted = Tasks::default();
        wanted
            .roles
            .claim(scout_serial, Role::Scout, 3, Some(at))
            .expect("a free unit");
        wanted
            .roles
            .begin_muster(MusterKind::Raid, at, Some(at), &[gathering], 3)
            .expect("a free unit");
        wanted.assemblies.push(Assembly {
            target: at,
            beside: Some(at),
            rally: None,
        });
        assert_eq!(tasks(&w, p), wanted, "{name}");
        assert_eq!(tasks(&w, other), Tasks::default(), "{name}");
        // A task orders no unit about.
        assert_eq!(w.units().unit_position(scout), Some(at), "{name}");
        assert_eq!(w.units().unit_moves_left(scout), moves, "{name}");
        assert!(!w.unit_has_order(scout), "{name}");
        let back = Snapshot::from_text(&w.snapshot().to_text()?)?.restore(content())?;
        assert_eq!(*back.player_tasks(p), wanted, "{name}");
        assert_eq!(*back.player_tasks(other), Tasks::default(), "{name}");
    }
    Ok(())
}

/// The turns an entry records are those of the round the command is given.
#[test]
fn a_task_is_dated_with_the_round_of_the_command() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (unit, serial) = create(&mut w, p);
        let at = w.units().unit_position(unit).expect("a square");
        let held = |w: &Table| {
            let tasks = w.player_tasks(p);
            let a = tasks.roles.get(serial).expect("an entry");
            (a.since, a.planned)
        };
        w.round = 4;
        let claim = Change::Claim {
            serial,
            role: Role::Scout,
            to: None,
        };
        change(&mut w, p, claim);
        assert_eq!(held(&w), (4, 4), "{name}");
        w.round = 9;
        let target = Change::SetTarget {
            serial,
            role: Role::Scout,
            to: Some(at),
        };
        change(&mut w, p, target);
        assert_eq!(held(&w), (4, 9), "{name}");
        w.round = 12;
        change(&mut w, p, Change::RestartPlan { serial });
        assert_eq!(held(&w), (4, 12), "{name}");
        w.round = 15;
        change(&mut w, p, Change::ReleaseAny { serial });
        assert_eq!(held(&w), (15, 15), "{name}");
    }
}

/// Base roles are written for many units at once, where no routine has a task for the unit, and a
/// role a unit holds already keeps its date.
#[test]
fn base_roles_are_written_for_the_units_without_a_task() {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        let (unit, scout) = create(&mut w, p);
        let (_, idle) = create(&mut w, p);
        let (_, foreign) = create(&mut w, other);
        let at = w.units().unit_position(unit).expect("a square");
        change(
            &mut w,
            p,
            Change::Claim {
                serial: scout,
                role: Role::Scout,
                to: None,
            },
        );
        let bases = [scout, idle, foreign]
            .map(|s| (s, Role::Garrison, Some(at)))
            .to_vec();
        for round in [5, 6] {
            w.round = round;
            let command = Change::SetBases {
                bases: bases.clone(),
            };
            assert_eq!(change(&mut w, p, command), Vec::new(), "{name}");
            let tasks = w.player_tasks(p);
            let held = |s| tasks.roles.get(s).map(|a| (a.role, a.to, a.since));
            assert_eq!(held(idle), Some((Role::Garrison, Some(at), 5)), "{name}");
            assert_eq!(
                tasks.roles.role_of(scout),
                Some(Role::Scout),
                "{name}: it has a task"
            );
            assert_eq!(held(foreign), None, "{name}: another's unit");
        }
    }
}

/// The ship that carries a unit is recorded, with the square it boards at when it still has to.
#[test]
fn the_ship_that_carries_a_unit_is_recorded() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (unit, serial) = create(&mut w, p);
        let (_, ship) = create(&mut w, p);
        let at = w.units().unit_position(unit).expect("a square");
        for role in [Role::SeaParty, Role::Scout] {
            change(&mut w, p, Change::ReleaseAny { serial });
            change(
                &mut w,
                p,
                Change::Claim {
                    serial,
                    role,
                    to: None,
                },
            );
        }
        change(&mut w, p, Change::Tie { serial, with: ship });
        let tied = w.player_tasks(p).roles.tied_with(ship);
        assert_eq!(tied, vec![serial], "{name}");
        assert_eq!(w.player_tasks(p).roles.boarding_of(serial), None, "{name}");
        change(&mut w, p, Change::Unboard { serial });
        change(
            &mut w,
            p,
            Change::SetCrossing {
                serial,
                crossing: Some(Crossing::Sea),
            },
        );
        change(
            &mut w,
            p,
            Change::BoardAt {
                serial,
                ship,
                tile: at,
            },
        );
        let tasks = w.player_tasks(p);
        assert_eq!(tasks.roles.with_of(serial), Some(ship), "{name}");
        assert_eq!(tasks.roles.boarding_of(serial), Some(at), "{name}");
    }
}

#[test]
fn a_replayed_command_makes_the_same_change() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (unit, serial) = create(&mut w, p);
        let (_, gathering) = create(&mut w, p);
        let at = w.units().unit_position(unit).expect("a square");
        let mut changes = changes_for(serial, gathering, at);
        let mut all = Tasks::default();
        all.roles
            .claim(serial, Role::Scout, 3, Some(at))
            .expect("a free unit");
        changes.push(Change::Import(Box::new(all)));
        for change in changes {
            let command = Command::Task { player: p, change };
            let text = w.command_to_text(&command).expect("a command as text");
            assert_eq!(w.command_from_text(&text), Some(command), "{name}");
        }
    }
}

#[test]
fn a_unit_that_is_not_in_the_army_has_no_task() {
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        let (leaver, leaver_serial) = create(&mut w, p);
        let (stayer, stayer_serial) = create(&mut w, p);
        let (_, foreign) = create(&mut w, other);
        let at = w.units().unit_position(stayer).expect("a square");
        for c in changes_for(stayer_serial, leaver_serial, at) {
            change(&mut w, p, c);
        }
        let claim = Change::Claim {
            serial: foreign,
            role: Role::Meld,
            to: Some(at),
        };
        change(&mut w, p, claim);
        let held = tasks(&w, p);
        assert!(held.roles.get(foreign).is_none(), "{name}: another's unit");
        assert!(held.roles.get(leaver_serial).is_some(), "{name}");
        assert_eq!(held.roles.musters().len(), 1, "{name}");
        // The same for tasks taken over whole.
        let mut whole = held.clone();
        whole
            .roles
            .claim(foreign, Role::Meld, 3, Some(at))
            .expect("a free unit");
        change(&mut w, p, Change::Import(Box::new(whole)));
        assert_eq!(tasks(&w, p), held, "{name}");

        // The unit leaves: its task goes, and the muster it was the last of.
        w.apply(Command::DismissUnit { unit: leaver });
        let held = tasks(&w, p);
        assert!(held.roles.get(leaver_serial).is_none(), "{name}");
        assert!(held.roles.musters().is_empty(), "{name}");
        assert!(held.roles.get(stayer_serial).is_some(), "{name}");
    }
}

#[test]
fn tasks_for_a_player_that_is_not_there_are_refused() {
    for (name, mut w) in worlds() {
        let before = state(&w);
        let events = change(&mut w, PlayerId(200), Change::TidyMusters);
        assert!(
            matches!(events[..], [Event::Rejected(_)]),
            "{name}: {events:?}"
        );
        assert_eq!(state(&w), before, "{name}");
    }
}
