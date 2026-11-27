//! What each unit is for. Every unit has one role at a time; the audit at the end of the turn goes
//! through all of them, finds the soldiers with none (adrift: no list, no order, no city) and
//! sends them home, counts the ships and other units with none, and the ones with two (a unit in
//! two role lists), so nothing is left standing on a tower or a road with no purpose.
//!
//! The roles live in the lists of [`Ai`] (`scouts`, `raiders`, `marching`, ...), and [`derive`]
//! reads them back as one table. The design is in `docs/ai-design.md`.

use crate::view::{is_fighter, is_ship};
use crate::Ai;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::Event;
use f4x_master::Game;

pub use f4x_core::unit::task::{Assignment, Crossing, Muster, MusterKind, Registry, Role};

/// What kind of unit it is, for the roles it can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A unit that fights (melee or ranged strength, not a ship).
    Soldier,
    /// A ship.
    Ship,
    /// Settlers, engineers, spirits and the rest: no role of theirs is built yet but settling.
    Other,
}

/// A unit and its role.
#[derive(Debug, Clone)]
pub struct Held {
    pub unit: hecs::Entity,
    pub class: Class,
    pub role: Role,
}

/// Give every unit the wizard owns exactly one role in the registry, and return the table. A
/// unit a routine has a task for keeps its role. Every other unit gets the base role its place and orders say, written
/// into the registry with its target:
///
/// - a soldier in an own city that its defence needs (`garrison::post`): `Garrison`, `to` = the
///   city; the soldiers beyond that await orders;
/// - a unit with an order of its own: `MoveTo`, `to` = the end of its path;
/// - a ship with no mission and no order: `TransportFree` (with a mission it holds
///   `TransportSpace`, `TransportReady` or `Transporting`, which `naval.rs` and `pickup.rs` set);
/// - anything else: `AwaitingOrders`.
pub fn sync(ai: &mut Ai, game: &mut Game) -> Vec<Held> {
    let bases = bases(&PlayerView::new(game, ai.player), game.round());
    ai.write(game).set_bases(
        bases
            .iter()
            .map(|&(_, _, serial, role, to)| (serial, role, to)),
    );
    let tasks = ai.tasks(game);
    bases
        .into_iter()
        .map(|(unit, class, serial, _, _)| {
            let role = tasks.roles.role_of(serial);
            let role = role.unwrap_or(Role::AwaitingOrders);
            Held { unit, class, role }
        })
        .collect()
}

/// Every unit of the player with the base role its place and orders say, and that role's target.
fn bases(
    view: &PlayerView,
    round: u32,
) -> Vec<(hecs::Entity, Class, Serial, Role, Option<Position>)> {
    let mut out = Vec::new();
    // The soldiers each city keeps as its garrison; the others standing in it await orders.
    let mut posts: std::collections::HashSet<hecs::Entity> = std::collections::HashSet::new();
    {
        let mut by_city: std::collections::BTreeMap<(u8, i32, i32), (Position, Vec<hecs::Entity>)> =
            std::collections::BTreeMap::new();
        for unit in view.my_units() {
            if is_ship(view, unit) || !is_fighter(view, unit) {
                continue;
            }
            if let Some(at) = view
                .unit_position(unit)
                .filter(|&p| view.city_at(p).is_some())
            {
                by_city
                    .entry((at.plane as u8, at.y as i32, at.x as i32))
                    .or_insert_with(|| (at, Vec::new()))
                    .1
                    .push(unit);
            }
        }
        for (at, soldiers) in by_city.into_values() {
            posts.extend(crate::garrison::post(view, at, &soldiers, round));
        }
    }
    for unit in view.my_units() {
        let class = if is_ship(view, unit) {
            Class::Ship
        } else if is_fighter(view, unit) {
            Class::Soldier
        } else {
            Class::Other
        };
        let serial = view.serial_of(unit);
        let ordered = view.unit_has_order(unit);
        let at = view.unit_position(unit);
        let city = at.filter(|&p| view.city_at(p).is_some());
        let target = view.unit_destination(unit);
        let (role, to) = match class {
            Class::Soldier if city.is_some() && posts.contains(&unit) => (Role::Garrison, city),
            // A ship with no mission is free; its transport roles (space, ready) are the ones
            // `naval.rs` and `pickup.rs` give it with a target.
            Class::Ship if !ordered => (Role::TransportFree, None),
            _ if ordered => (Role::MoveTo, target),
            _ => (Role::AwaitingOrders, None),
        };
        out.push((unit, class, serial, role, to));
    }
    out
}

/// What the table shows: the soldiers awaiting orders (adrift), and the ships and other units
/// awaiting orders (no role of theirs is built yet).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub adrift: Vec<hecs::Entity>,
    pub unassigned: u32,
}

/// Count the table.
pub fn tally(held: &[Held]) -> Tally {
    let mut t = Tally::default();
    for h in held.iter().filter(|h| h.role == Role::AwaitingOrders) {
        match h.class {
            Class::Soldier => t.adrift.push(h.unit),
            _ => t.unassigned += 1,
        }
    }
    t
}

/// The start of the turn: units that appeared since the last audit (built, summoned, hired, come
/// off a ship) get their role at once, and the musters with nobody left go.
pub fn register(ai: &mut Ai, game: &mut Game) {
    ai.write(game).tidy_musters();
    sync(ai, game);
}

/// The end-of-turn check: every unit the wizard owns is looked at, each turn, and has a role. A
/// unit with nothing to do awaits orders and stays where it stands: the soldiers among them are
/// counted (adrift), as are the ships and the other units (unassigned). The audit is counted in an
/// [`Event::RolesAudited`].
pub fn audit(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // The musters with nobody left go; the units each get their role.
    ai.write(game).tidy_musters();
    // Units with no entry yet came into being since the start of the turn.
    let unregistered = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        view.my_units()
            .into_iter()
            .filter(|&u| tasks.roles.get(view.serial_of(u)).is_none())
            .count() as u32
    };
    let counted = tally(&sync(ai, game));
    let reset = std::mem::take(&mut ai.missions_reset);
    let (waiting, bridges, waiting_bridge) = {
        let tasks = ai.tasks(game);
        (
            tasks.roles.waiting_for_ship().len() as u32,
            tasks.roles.count(Role::Bridge) as u32,
            tasks.roles.needing(Crossing::Bridge).len() as u32,
        )
    };
    if counted.adrift.is_empty()
        && counted.unassigned == 0
        && unregistered == 0
        && reset == 0
        && waiting == 0
        && bridges == 0
        && waiting_bridge == 0
    {
        return;
    }
    events.push(Event::RolesAudited {
        player: me,
        adrift: counted.adrift.len() as u32,
        unassigned: counted.unassigned,
        unregistered,
        reset,
        waiting,
        bridges,
        waiting_bridge,
    });
    // Nothing more: a unit awaiting orders stays where it stands (task 645). The dispatcher
    // gives the waiting units their jobs.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};
    use f4x_core::world::Command;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The base roles go into the ledger in one command, and only when one of them has changed.
    #[test]
    fn a_sync_that_finds_nothing_new_writes_no_base_role() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(8, 6), 104 /* spearmen */, 3);
        let mut ai = Ai::new(P0);
        let writes = |w: &mut Game, ai: &mut Ai| -> Result<usize, &'static str> {
            w.start_recording();
            sync(ai, w);
            let replay = w.stop_recording().ok_or("a recording")?;
            Ok(replay.entries.len())
        };
        assert_eq!(writes(&mut w, &mut ai)?, 1, "all of them at once");
        assert_eq!(writes(&mut w, &mut ai)?, 0, "nothing has changed");
        spawn(&mut w, P0, at(8, 6), 104 /* spearmen */, 1);
        assert_eq!(writes(&mut w, &mut ai)?, 1, "the new unit");
        Ok(())
    }

    /// A soldier standing outside a city with no list, no order and no city under it is adrift:
    /// the audit counts it and leaves it where it stands.
    #[test]
    fn a_lone_soldier_outside_a_city_is_counted_and_left_where_it_stands() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        w.reveal_map(P0);
        let lone = spawn(&mut w, P0, tower, 104 /* spearmen */, 1);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        audit(&mut ai, &mut w, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::RolesAudited { adrift: 1, .. })),
            "{events:?}"
        );
        let view = PlayerView::new(&w, P0);
        assert!(lone.iter().all(|&u| !view.unit_has_order(u)));
        assert!(events.iter().all(|e| !matches!(e, Event::Rejected(_))));
        let held = sync(&mut ai, &mut w);
        let roles = held.iter().find(|h| h.unit == lone[0]).map(|h| h.role);
        assert_eq!(roles, Some(Role::AwaitingOrders));
        Ok(())
    }

    /// A unit in a role list, or in a city, or with an order, is not adrift.
    #[test]
    fn a_unit_with_a_list_a_city_or_an_order_is_not_adrift() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let home = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 1);
        let scout = spawn(&mut w, P0, at(8, 6), 104, 1);
        let mut ai = Ai::new(P0);
        let serial = PlayerView::new(&w, P0).serial_of(scout[0]);
        let _ = ai.write(&mut w).claim(serial, Role::Scout, None);
        let held = sync(&mut ai, &mut w);
        let counted = tally(&held);
        assert!(counted.adrift.is_empty(), "{held:?}");
        let roles_of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).map(|h| h.role);
        assert_eq!(roles_of(home[0]), Some(Role::Garrison));
        assert_eq!(roles_of(scout[0]), Some(Role::Scout));
        Ok(())
    }

    /// Soldiers with no own city they can walk to stay where they stand and await orders: the
    /// audit sends them nowhere (the transport job of task 647 takes them to a ship).
    #[test]
    fn soldiers_with_no_city_in_reach_await_orders_where_they_stand() -> TestResult {
        use crate::testworld::ENEMY_CITY;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let far = {
            let view = PlayerView::new(&w, P0);
            crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
                .ok_or("a shore tile near the far city")?
                .0
        };
        let stranded = spawn(&mut w, P0, far, 104 /* spearmen */, 2);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        audit(&mut ai, &mut w, &mut events);
        assert!(ai.tasks(&w).roles.members(Role::Reinforce).is_empty());
        let held = sync(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        for &u in &stranded {
            let roles = held.iter().find(|h| h.unit == u).map(|h| h.role);
            assert_eq!(roles, Some(Role::AwaitingOrders));
            assert!(!view.unit_has_order(u));
        }
        Ok(())
    }

    /// Every unit is looked at, not only soldiers: a ship in port is docked, a settler has its
    /// role, and a unit of another kind with nothing to do is counted as unassigned.
    #[test]
    fn ships_settlers_and_other_units_are_in_the_table_too() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        let ship = spawn(&mut w, P0, port, 36 /* galley */, 1)[0];
        let other = spawn(&mut w, P0, at(8, 6), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        let held = sync(&mut ai, &mut w);
        let of = |u: hecs::Entity| held.iter().find(|h| h.unit == u).cloned();
        let ship_held = of(ship).ok_or("the ship is in the table")?;
        assert_eq!(
            (ship_held.class, ship_held.role),
            (Class::Ship, Role::TransportFree)
        );
        let other_held = of(other).ok_or("the settlers are in the table")?;
        assert_eq!(other_held.class, Class::Other);
        assert_eq!(other_held.role, Role::AwaitingOrders);
        assert_eq!(tally(&held).unassigned, 1);
        Ok(())
    }

    fn serial(n: u64) -> f4x_core::world::serial::Serial {
        f4x_core::world::serial::Serial(n)
    }

    /// The registry holds one role per unit: a lower-priority claim on a held role is refused, a
    /// better one takes over, and a release only gives back the role actually held.
    #[test]
    fn a_muster_holds_its_members_to_one_rally_and_ends_as_one() {
        let mut r = Registry::default();
        let (a, b, c) = (serial(1), serial(2), serial(3));
        let rally = Position {
            plane: f4x_core::map::Plane::Arcanus,
            x: 4,
            y: 5,
        };
        // A raider keeps its better role: only two join.
        assert_eq!(r.claim(c, Role::Raid, 1, None), Ok(()));
        let party = r
            .begin_muster(MusterKind::Raid, rally, Some(rally), &[a, b, c], 7)
            .expect("two units took the role");
        assert_eq!(r.members_of(party), vec![a, b]);
        assert_eq!(r.target_of(a, Role::Muster), Some(rally));
        assert!(r.any_muster(MusterKind::Raid) && !r.any_muster(MusterKind::Campaign));
        assert_eq!(
            r.muster_of(b).map(|(id, m)| (id, m.since)),
            Some((party, 7))
        );
        assert_eq!(r.new_party(), party + 1);
        r.end_muster(party, 8);
        assert_eq!(r.role_of(a), Some(Role::AwaitingOrders));
        assert!(r.musters().is_empty());
        assert_eq!(r.role_of(c), Some(Role::Raid));
    }

    #[test]
    fn the_registry_holds_one_role_per_unit() {
        let mut r = Registry::default();
        let (a, b) = (serial(1), serial(2));
        assert_eq!(r.claim(a, Role::Raid, 3, None), Ok(()));
        assert_eq!(
            r.claim(a, Role::Road, 4, None),
            Err(Role::Raid),
            "a raider is not sent to build a road by a lower claim"
        );
        assert_eq!(r.role_of(a), Some(Role::Raid));
        assert_eq!(r.claim(b, Role::Road, 4, None), Ok(()));
        assert_eq!(
            r.claim(b, Role::Reinforce, 5, None),
            Ok(()),
            "a better role takes over"
        );
        assert_eq!(r.role_of(b), Some(Role::Reinforce));
        assert!(!r.release(b, Role::Road, 6), "it does not hold that role");
        assert!(r.release(b, Role::Reinforce, 6));
        assert!(!r.has_role(b));
        assert_eq!(r.members(Role::Raid), vec![a]);
        assert_eq!(r.assigned(), vec![a]);
    }

    /// The claim keeps a unit to one role: a better role refuses a worse claim, a worse one is
    /// taken over.
    #[test]
    fn a_claim_keeps_a_unit_to_one_role() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut w, P0, at(8, 6), 104 /* spearmen */, 2);
        let ai = Ai::new(P0);
        let (raider, scout) = {
            let view = PlayerView::new(&w, P0);
            (view.serial_of(units[0]), view.serial_of(units[1]))
        };
        let _ = ai.write(&mut w).claim(raider, Role::Raid, None);
        let _ = ai.write(&mut w).claim(scout, Role::Scout, None);
        assert_eq!(
            ai.write(&mut w).claim(raider, Role::Road, None),
            Err(Role::Raid),
            "a raider is not sent to build a road by the lower claim"
        );
        assert_eq!(ai.write(&mut w).claim(scout, Role::Reinforce, None), Ok(()));
        assert_eq!(ai.tasks(&w).roles.role_of(scout), Some(Role::Reinforce));
        assert_eq!(ai.tasks(&w).roles.role_of(raider), Some(Role::Raid));
    }

    /// Units are grouped in the registry: a party lists its members, and an escort knows the
    /// unit it guards.
    #[test]
    fn units_are_grouped_by_party_and_an_escort_knows_its_charge() {
        let mut r = Registry::default();
        let (settler, guard, other) = (serial(1), serial(2), serial(3));
        for s in [settler, guard, other] {
            let _ = r.claim(s, Role::Settle, 1, None);
        }
        let party = r.new_party();
        r.group(&[settler, guard], Role::Settle, party, None);
        r.group(&[guard], Role::Settle, party, Some(settler));
        assert_eq!(r.members_of(party), vec![settler, guard]);
        assert_eq!(r.with_of(guard), Some(settler));
        assert_eq!(r.party_of(other), None);
        assert_ne!(r.new_party(), party);
        // Released, the unit leaves the party.
        r.release(guard, Role::Settle, 2);
        assert_eq!(r.members_of(party), vec![settler]);
    }

    /// No unit has an empty role: whatever has nothing to do awaits orders.
    #[test]
    fn no_unit_is_left_without_a_role() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 2);
        spawn(&mut w, P0, at(8, 6), 104, 1);
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            36, /* galley */
            1,
        );
        spawn(&mut w, P0, at(9, 6), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        let held = sync(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        assert_eq!(
            held.len(),
            view.my_units().len(),
            "every unit is in the table"
        );
        Ok(())
    }

    /// Every unit has its role written in the registry, with its target: a garrison soldier the
    /// city, a unit with an order its destination, a ship its state.
    #[test]
    fn every_unit_has_an_explicit_role_with_its_target() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let home = spawn(&mut w, P0, city, 104, 1)[0];
        let walker = spawn(&mut w, P0, city, 104, 1)[0];
        let ship = spawn(&mut w, P0, city, 36 /* galley */, 1)[0];
        let idle = spawn(&mut w, P0, at(8, 6), 111 /* high_men_settlers */, 1)[0];
        let mut events = Vec::new();
        let mut ai = Ai::new(P0);
        ai.send(
            &mut w,
            Command::GoTo {
                units: vec![walker],
                to: at(8, 6),
            },
            &mut events,
        )
        .map_err(|e| format!("{e:?}"))?;
        sync(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        let get = |u: hecs::Entity| ai.tasks(&w).roles.get(view.serial_of(u)).cloned();
        let of = |u: hecs::Entity| get(u).map(|a| (a.role, a.to));
        assert_eq!(of(home), Some((Role::Garrison, Some(city))));
        assert_eq!(of(walker).map(|r| r.0), Some(Role::MoveTo));
        assert!(of(walker).and_then(|r| r.1).is_some());
        assert_eq!(of(ship), Some((Role::TransportFree, None)));
        assert_eq!(of(idle), Some((Role::AwaitingOrders, None)));
        for u in view.my_units() {
            assert!(ai.tasks(&w).roles.has_role(view.serial_of(u)) || get(u).is_some());
        }
        // A task released goes back to a role at once, not to none.
        let s = view.serial_of(home);
        let _ = ai.write(&mut w).claim(s, Role::Raid, None);
        ai.write(&mut w).release(s, Role::Raid);
        assert_eq!(ai.tasks(&w).roles.role_of(s), Some(Role::AwaitingOrders));
        Ok(())
    }

    /// A task takes a free ship: space while fewer units are planned than it holds, ready when
    /// they fill it, transporting once it sails.
    #[test]
    fn a_ship_taken_by_a_task_is_space_until_its_load_is_full_then_ready() {
        let mut r = Registry::default();
        let ship = serial(1);
        let to = at(5, 5);
        assert!(r.plan_ship(ship, 1, 2, 1, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        assert_eq!(r.target_of(ship, Role::TransportSpace), Some(to));
        assert!(r.plan_ship(ship, 2, 2, 2, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportReady));
        // A smaller plan gives the ship room again.
        assert!(r.plan_ship(ship, 1, 2, 3, to));
        assert_eq!(r.role_of(ship), Some(Role::TransportSpace));
        r.depart(ship, to);
        assert_eq!(r.role_of(ship), Some(Role::Transporting));
    }

    /// Units tagged for a ship are counted by the audit, so a soak can tell how long they wait.
    #[test]
    fn units_waiting_for_a_ship_are_counted_by_the_audit() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let home = at(COAST_CITY.0, COAST_CITY.1);
        let soldiers = spawn(&mut w, P0, home, 104 /* spearmen */, 2);
        let mut ai = Ai::new(P0);
        register(&mut ai, &mut w);
        for &u in &soldiers {
            let s = PlayerView::new(&w, P0).serial_of(u);
            let _ = ai.write(&mut w).claim(s, Role::March, Some(at(30, 6)));
            ai.write(&mut w).set_crossing(s, Some(Crossing::Sea));
        }
        let mut events = Vec::new();
        audit(&mut ai, &mut w, &mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::RolesAudited { waiting: 2, .. })),
            "{events:?}"
        );
        Ok(())
    }
}
