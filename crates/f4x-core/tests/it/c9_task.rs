//! Class 9, the unit tasks: what a player writes into its army ledger is kept and saved with the
//! game, and says nothing of a unit that is not in the army.

use crate::support::{content, state, worlds};
use f4x_core::map::Position;
use f4x_core::player::PlayerId;
use f4x_core::rules::UnitId;
use f4x_core::unit::task::{Assembly, MusterKind, Role, Tasks};
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

/// Tasks with a scout, a muster of one and a gathering, for the units given.
fn tasks_for(scout: Serial, gathering: Serial, at: Position) -> Tasks {
    let mut tasks = Tasks::default();
    tasks
        .roles
        .claim(scout, Role::Scout, 3, Some(at))
        .expect("a free unit");
    tasks
        .roles
        .begin_muster(MusterKind::Raid, at, Some(at), &[gathering], 3)
        .expect("a free unit");
    tasks.assemblies.push(Assembly {
        target: at,
        beside: Some(at),
        rally: None,
    });
    tasks
}

fn set(w: &mut Table, player: PlayerId, tasks: Tasks) -> Vec<Event> {
    w.apply(Command::SetTasks {
        player,
        tasks: Box::new(tasks),
    })
}

#[test]
fn tasks_are_kept_for_the_player_and_saved_with_the_game() -> Result<(), Box<dyn std::error::Error>>
{
    for (name, mut w) in worlds() {
        let (p, other) = (w.wizards()[0], w.wizards()[1]);
        assert_eq!(w.player_tasks(p), None, "{name}");
        let (scout, scout_serial) = create(&mut w, p);
        let (_, gathering) = create(&mut w, p);
        let at = w.units().unit_position(scout).expect("a square");
        let tasks = tasks_for(scout_serial, gathering, at);
        let moves = w.units().unit_moves_left(scout);
        assert_eq!(set(&mut w, p, tasks.clone()), Vec::new(), "{name}");
        assert_eq!(w.player_tasks(p), Some(tasks.clone()), "{name}");
        assert_eq!(w.player_tasks(other), None, "{name}");
        // A task orders no unit about.
        assert_eq!(w.units().unit_position(scout), Some(at), "{name}");
        assert_eq!(w.units().unit_moves_left(scout), moves, "{name}");
        assert!(!w.unit_has_order(scout), "{name}");
        let back = Snapshot::from_text(&w.snapshot().to_text()?)?.restore(content())?;
        assert_eq!(back.player_tasks(p), Some(tasks), "{name}");
        assert_eq!(back.player_tasks(other), None, "{name}");
    }
    Ok(())
}

#[test]
fn a_replayed_command_writes_the_same_tasks() {
    for (name, mut w) in worlds() {
        let p = w.wizards()[0];
        let (unit, serial) = create(&mut w, p);
        let (_, gathering) = create(&mut w, p);
        let at = w.units().unit_position(unit).expect("a square");
        let command = Command::SetTasks {
            player: p,
            tasks: Box::new(tasks_for(serial, gathering, at)),
        };
        let text = w.command_to_text(&command).expect("a command as text");
        assert_eq!(w.command_from_text(&text), Some(command), "{name}");
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
        let mut tasks = tasks_for(stayer_serial, leaver_serial, at);
        tasks
            .roles
            .claim(foreign, Role::Meld, 3, Some(at))
            .expect("a free unit");
        set(&mut w, p, tasks);
        let held = w.player_tasks(p).expect("the tasks");
        assert!(held.roles.get(foreign).is_none(), "{name}: another's unit");
        assert!(held.roles.get(leaver_serial).is_some(), "{name}");
        assert_eq!(held.roles.musters().len(), 1, "{name}");

        // The unit leaves: its task goes, and the muster it was the last of.
        w.apply(Command::DismissUnit { unit: leaver });
        let held = w.player_tasks(p).expect("the tasks");
        assert!(held.roles.get(leaver_serial).is_none(), "{name}");
        assert!(held.roles.musters().is_empty(), "{name}");
        assert!(held.roles.get(stayer_serial).is_some(), "{name}");
    }
}

#[test]
fn tasks_for_a_player_that_is_not_there_are_refused() {
    for (name, mut w) in worlds() {
        let before = state(&w);
        let events = set(&mut w, PlayerId(200), Tasks::default());
        assert!(
            matches!(events[..], [Event::Rejected(_)]),
            "{name}: {events:?}"
        );
        assert_eq!(state(&w), before, "{name}");
    }
}
