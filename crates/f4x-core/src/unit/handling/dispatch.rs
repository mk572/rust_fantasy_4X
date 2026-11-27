//! The job board (task 645, `docs/handler-design.md` section 14). A unit with nothing to do awaits
//! orders where it stands; the dispatcher, once at the end of the wizard's turn, goes through the
//! open jobs in a fixed order and hands each the waiting units it needs. What no job takes stays
//! waiting.
//!
//! The pool is the units that are free (`Roles::is_free`): no routine has a task for them and they
//! are not a city's garrison (`garrison::post`, which gives a city the soldiers its defence
//! needs, slow defensive ones first). A job asks the game whether it is open, takes its units from
//! the pool by the registry's claim (a unit holds one role, so two jobs never have the same unit),
//! and gives them back when it is done.
//!
//! The order of the checklist (before it, the quick strikes of `strike.rs`: a stack next to an
//! enemy it can clearly beat fights it, before any unit has moved):
//!
//! 0. a city in danger (the perimeter: the free soldiers go for the enemy in its zone, or join the
//!    garrison when it is the stronger);
//! 1. settlers to their sites, engineers to their roads, spirits to their nodes;
//! 2. escorts for the settlers and engineers whose way is not safe;
//! 3. scouts for unexplored land, the fast units first;
//! 4. the war: the rally, the army against an enemy city (task 646); at peace it asks for nothing
//!    and is skipped, at war it picks its soldiers before the raids do;
//! 5. raiding parties for a lair, from the soldiers the war left over (646);
//! 6. the other under-defended cities (646);
//!
//! Then come the ships and the colonies (task 647) and the audit that counts what is left.

use super::Handler;
use super::Host;
use crate::player::view::PlayerView;
use crate::world::Event;

/// Whether the mission a unit holds is still worth pursuing: the lair a raid went for is still
/// there (or the tower intact, the neutral city standing), the ground a scout went to is still
/// unexplored, the node a spirit went to is still open. Missions of other roles are checked by
/// their own modules each turn.
fn mission_valid(
    handler: &Handler,
    view: &crate::player::view::PlayerView,
    role: super::roles::Role,
    target: crate::map::Position,
    round: u32,
) -> bool {
    use super::roles::Role;
    match role {
        // A target with a city on it is dropped (a landing square of a voyage is no site: the settler
        // looks for one ashore).
        // The site must still take a city and a way lead to it (on foot or by ship); the way is
        // worked out on the day of the site check only (`city_site.rs`).
        Role::Settle => {
            view.city_at(target).is_none()
                && view.can_found_city(target)
                && handler.judge.site_reachable(view, target, round)
        }
        Role::Scout => !view.explored(target),
        Role::Meld => super::meld::open_nodes(handler, view).contains(&target),
        Role::Reinforce => view.my_cities().into_iter().any(|(_, p)| p == target),
        _ => true,
    }
}

/// Send units that lost their walk to their target again. A unit the game says no walk leads to
/// is tagged with what it needs (`Crossing`: another plane, or the sea when the target is on this
/// plane with no known land route) and keeps its role. The units to give up are returned: those
/// refused for another reason (no moves left, say).
fn resend(
    handler: &mut Handler,
    game: &mut dyn Host,
    lost: Vec<(hecs::Entity, super::roles::Role, crate::map::Position)>,
    events: &mut Vec<Event>,
) -> Vec<(hecs::Entity, super::roles::Role)> {
    use super::SendError;
    use crate::unit::movement::MoveError;
    use crate::world::Rejection;
    let mut refused = Vec::new();
    for (unit, role, target) in lost {
        // A unit with planar travel of its own (a shadow demon, or any unit with the spell or an
        // item) changes plane where it stands, with no tower, gate or spell to wait for.
        shift_by_itself(handler, game, unit, target, events);
        // `Err(true)`: no walk leads there; `Err(false)`: refused for another reason.
        let sent: Result<(), bool> = handler
            .probe(
                game,
                crate::world::Command::GoTo {
                    units: vec![unit],
                    to: target,
                },
                events,
            )
            .map_err(|why| {
                matches!(
                    why,
                    SendError::Rejected(
                        Rejection::Move(MoveError::Impassable) | Rejection::NoPath(_)
                    ) | SendError::Stopped(MoveError::Impassable)
                )
            });
        let unreachable = match sent {
            Ok(()) => continue,
            Err(no_walk) => no_walk,
        };
        let (serial, here) = {
            let view = crate::player::view::PlayerView::new(game, handler.player);
            (view.serial_of(unit), view.unit_position(unit))
        };
        match here {
            Some(h) if h.plane != target.plane => {
                handler
                    .roles
                    .set_crossing(serial, Some(super::roles::Crossing::Plane));
            }
            // Only a walker waits for a ship; a flier or a swimmer crosses the water itself.
            Some(_)
                if unreachable
                    && crate::player::view::PlayerView::new(game, handler.player)
                        .is_walker(unit) =>
            {
                handler
                    .roles
                    .set_crossing(serial, Some(super::roles::Crossing::Sea));
            }
            _ => refused.push((unit, role)),
        }
    }
    refused
}

/// A unit on the other plane from `target` that can travel between the planes by itself shifts
/// now (`UnitFlag::PlanarTravel`, its own, its enchantments' or its items'), so that its walk is an
/// ordinary one and it is never tagged `Crossing::Plane`. Does nothing for any other unit, or
/// while the game will not let it shift (Planar Seal, no moves left).
fn shift_by_itself(
    handler: &mut Handler,
    game: &mut dyn Host,
    unit: hecs::Entity,
    target: crate::map::Position,
    events: &mut Vec<Event>,
) {
    let shifts = {
        let view = crate::player::view::PlayerView::new(game, handler.player);
        view.unit_position(unit)
            .is_some_and(|p| p.plane != target.plane)
            && view.unit_has_flag(unit, f4x_def::rules::unit_flag::UnitFlag::PlanarTravel)
            && view.can_shift_plane(&[unit])
    };
    if shifts {
        let _ = handler.send(
            game,
            crate::world::Command::ShiftPlane { units: vec![unit] },
            events,
        );
    }
}

/// An escort that fell behind its charge (the stack was split by a fight, or the charge was
/// ordered on by another job) goes to where the charge stands now. Run after the jobs that move
/// the charges, so it follows their new square. An escort the game will not send keeps its role.
pub fn repair_escorts(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    use super::roles::Role;
    let lost: Vec<_> = {
        let view = crate::player::view::PlayerView::new(game, handler.player);
        let find = |s| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        };
        handler
            .roles
            .members(Role::Escort)
            .into_iter()
            .filter(|&e| handler.roles.crossing_of(e).is_none())
            .filter_map(|e| {
                let charge = find(handler.roles.with_of(e)?)?;
                let unit = find(e)?;
                let (there, here) = (view.unit_position(charge)?, view.unit_position(unit)?);
                (there != here && !view.unit_has_order(unit)).then_some((unit, Role::Escort, there))
            })
            .collect()
    };
    resend(handler, game, lost, events);
}

/// Before any new job is looked for: a unit sent on a mission that can no longer be fulfilled
/// (a rival cleared the lair first, the ground was explored by another) is given up. A unit whose
/// walk is gone (it was moved by a strike, a battle, a refused step) but whose target stands in
/// the registry is sent again; it is given up only when the game refuses. It loses its role and its order and awaits orders where it
/// stands, so the jobs below see it in the pool. Returns how many were reset.
pub fn revalidate(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) -> u32 {
    use super::roles::Role;
    let me = handler.player;
    let mut reset: Vec<(hecs::Entity, super::roles::Role)> = Vec::new();
    let mut lost: Vec<(hecs::Entity, super::roles::Role, crate::map::Position)> = Vec::new();
    {
        let view = crate::player::view::PlayerView::new(game, me);
        for unit in view.my_units() {
            let serial = view.serial_of(unit);
            for role in [
                Role::Scout,
                Role::Meld,
                Role::Settle,
                Role::Road,
                Role::Reinforce,
            ] {
                if !handler.roles.is(serial, role) {
                    continue;
                }
                let Some(target) = handler.roles.target_of(serial, role) else {
                    continue;
                };
                let here = view.unit_position(unit);
                // Failed: no order is left and the unit is not where it was going.
                let arrived = here == Some(target);
                // A ship that has sailed to its scouting target has explored it by arriving, and its
                // mission is not over: `scout::scout` puts the party aboard ashore and picks the
                // next target. Resetting it here left the party waiting for a ship that no longer
                // scouted.
                let arrived_ship =
                    role == Role::Scout && arrived && super::view::is_ship(&view, unit);
                if !arrived_ship && !mission_valid(handler, &view, role, target, game.round()) {
                    if role == Role::Settle {
                        handler
                            .judge
                            .settler_lost(if view.city_at(target).is_some() {
                                super::SettlerFate::SiteTaken
                            } else {
                                super::SettlerFate::SiteUnusable
                            });
                    }
                    reset.push((unit, role));
                } else if !view.unit_has_order(unit)
                    && !view.unit_builds_road(unit)
                    && !arrived
                    && handler.roles.crossing_of(serial).is_none()
                {
                    // A unit can lose its walk for many reasons (a strike moved it, a battle,
                    // a refused step): the registry still knows where it goes.
                    lost.push((unit, role, target));
                }
            }
        }
    }
    reset.extend(resend(handler, game, lost, events));
    let mut serials = Vec::new();
    {
        let view = crate::player::view::PlayerView::new(game, me);
        for &(unit, role) in &reset {
            let serial = view.serial_of(unit);
            handler.roles.release(serial, role);
            serials.push(unit);
        }
    }
    let count = serials.len() as u32;
    if !serials.is_empty() {
        handler.order(
            game,
            crate::world::Command::ClearOrders { units: serials },
            events,
        );
    }
    handler.missions_reset += count;
    count
}

/// One turn of the unit handling, in the order the jobs take their turns.
pub fn run(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    super::roles::register(handler, game, game.round());
    // The sweep of the ship ties (and the sanity check of the passengers), before any job moves
    // one of them.
    super::pickup::tidy_unit_registry(handler, game);
    // First the units whose mission is over, so the jobs below see them waiting.
    revalidate(handler, game, events);
    // The soldiers that are no longer needed in a city.
    super::reinforce::release(
        handler,
        game,
        &handler
            .judge
            .threats(&PlayerView::new(game, handler.player)),
    );
    // settlers, engineers, spirits
    super::expand::settlers(handler, game, events);
    super::road::run(handler, game, events);
    super::meld::run(handler, game, events);
    // escorts
    super::escort::run(handler, game, events);
    repair_escorts(handler, game, events);
    // scouts
    super::scout::scout(handler, game, events);
    // The ships: the bridges first, since a free ship goes to the bridge before it scouts
    super::bridge::run(handler, game, events);
    super::pickup::run(handler, game, events);
    // The orders a full square stopped go again, now that the units that could move have moved.
    handler.retry_blocked(game, events);
    super::roles::audit(handler, game, events);
}

#[cfg(test)]
mod tests {
    use super::super::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0};
    use super::*;

    /// A unit with nothing to do stays where it is: the turn sends it nowhere.
    #[test]
    fn a_waiting_soldier_with_no_job_is_left_where_it_stands() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // The city holds the one soldier its defence wants: the lone one is not needed there.
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 1);
        let lone = spawn(&mut w, P0, at(8, 6), 104, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        let mut events = Vec::new();
        run(&mut handler, &mut w, &mut events);
        let view = crate::player::view::PlayerView::new(&w, P0);
        // The map is fully known, so there is nothing to scout; nobody is at war.
        assert!(!view.unit_has_order(lone), "{events:?}");
        assert_eq!(view.unit_position(lone), Some(at(8, 6)));
    }

    /// A scout whose ground was explored meanwhile is given up and awaits orders; one still on
    /// its way keeps its mission, and one with no walk to its target keeps it too, tagged with
    /// the ship it needs.
    #[test]
    fn a_unit_whose_mission_is_over_awaits_orders_and_one_that_cannot_walk_waits_for_a_ship() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(30, 10);
        let walker = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let stopped = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let view = crate::player::view::PlayerView::new(&w, P0);
        let (a, b) = (view.serial_of(walker), view.serial_of(stopped));
        assert!(!view.explored(target), "unexplored ground to scout");
        for s in [a, b] {
            let _ = handler.roles.claim(s, Role::Scout, 1, Some(target));
        }
        let mut events = Vec::new();
        handler
            .send(
                &mut w,
                crate::world::Command::GoTo {
                    units: vec![walker],
                    to: at(5, 9),
                },
                &mut events,
            )
            .expect("the walker can walk");
        // `walker` has an order and valid ground; `stopped` has no order and no walk leads to
        // its target (across the sea): it keeps its mission and waits for a ship.
        assert_eq!(revalidate(&mut handler, &mut w, &mut events), 0);
        assert!(handler.roles.is(a, Role::Scout));
        assert!(handler.roles.is(b, Role::Scout));
        assert_eq!(
            handler.roles.crossing_of(b),
            Some(super::super::roles::Crossing::Sea)
        );
        assert_eq!(handler.roles.crossing_of(a), None);
        // The ground becomes known: both missions are over, the flag goes with the role.
        w.reveal_map(P0);
        assert_eq!(revalidate(&mut handler, &mut w, &mut events), 2);
        assert!(!handler.roles.is(a, Role::Scout));
        assert!(!handler.roles.is(b, Role::Scout));
        assert_eq!(handler.roles.crossing_of(b), None);
        let view = crate::player::view::PlayerView::new(&w, P0);
        assert!(!view.unit_has_order(walker));
        assert_eq!(handler.missions_reset, 2);
    }

    /// A unit that lost its walk (a strike moved it, a battle) but whose target is in the
    /// registry is sent on again, not given up.
    #[test]
    fn a_unit_that_lost_its_walk_is_sent_on_to_the_target_in_the_registry() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(5, 9);
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let view = crate::player::view::PlayerView::new(&w, P0);
        let serial = view.serial_of(scout);
        assert!(!view.explored(target), "unexplored ground to scout");
        let _ = handler.roles.claim(serial, Role::Scout, 1, Some(target));
        assert!(!view.unit_has_order(scout), "no walk");
        let mut events = Vec::new();
        assert_eq!(
            revalidate(&mut handler, &mut w, &mut events),
            0,
            "not given up"
        );
        assert!(handler.roles.is(serial, Role::Scout));
        let view = crate::player::view::PlayerView::new(&w, P0);
        assert!(
            view.unit_has_order(scout) || view.unit_position(scout) != Some(at(9, 7)),
            "walking again: {events:?}"
        );
    }

    /// A sea scout that arrives at its target has explored it by arriving. It keeps the scouting
    /// (it is the one that puts its party ashore, `scout::scout`), where a walker on the same
    /// ground is done and gives the mission up.
    #[test]
    fn a_ship_that_arrived_at_its_scouting_target_keeps_the_role_and_puts_its_party_ashore() {
        use super::super::roles::Role;
        use f4x_def::half_int::HalfInt;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // Off the east island's west shore, as in `scout`'s own landing test.
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let view = crate::player::view::PlayerView::new(&w, P0);
        let (ship, rider) = (view.serial_of(boat), view.serial_of(walker));
        assert!(view.explored(sea), "arriving explored the target");
        let _ = handler.roles.claim(ship, Role::Scout, 1, Some(sea));
        let _ = handler.roles.claim(rider, Role::SeaParty, 1, None);
        handler.sea_scouting = true;
        assert_eq!(revalidate(&mut handler, &mut w, &mut Vec::new()), 0);
        assert!(
            handler.roles.is(ship, Role::Scout),
            "the ship goes on scouting"
        );
        super::super::scout::scout(&mut handler, &mut w, &mut Vec::new());
        assert!(
            !handler.roles.is(rider, Role::SeaParty),
            "the rider went ashore"
        );
        assert_ne!(w.units().unit_position(walker), Some(sea));
    }

    /// A target on the other plane needs a crossing of the planes; the unit keeps its role and its
    /// target and is not sent again every turn.
    #[test]
    fn a_unit_whose_target_is_on_the_other_plane_is_tagged_with_a_plane_crossing() {
        use super::super::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = crate::map::Plane::Myrror;
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let view = crate::player::view::PlayerView::new(&w, P0);
        let serial = view.serial_of(scout);
        assert!(!view.explored(target));
        let _ = handler.roles.claim(serial, Role::Scout, 1, Some(target));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut handler, &mut w, &mut events), 0);
        assert_eq!(handler.roles.crossing_of(serial), Some(Crossing::Plane));
        assert_eq!(
            handler.roles.needing(Crossing::Plane),
            vec![(serial, target)]
        );
        // A new target clears the tag.
        handler
            .roles
            .set_target(serial, Role::Scout, Some(at(5, 9)));
        assert_eq!(handler.roles.crossing_of(serial), None);
    }

    /// A unit that travels between the planes by itself shifts and walks on: it is not tagged.
    #[test]
    fn a_unit_with_planar_travel_shifts_itself_instead_of_waiting_for_a_crossing() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = crate::map::Plane::Myrror;
        let demon = spawn(&mut w, P0, at(9, 7), 171 /* shadow_demons */, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let serial = crate::player::view::PlayerView::new(&w, P0).serial_of(demon);
        let _ = handler.roles.claim(serial, Role::Scout, 1, Some(target));
        let mut events = Vec::new();
        revalidate(&mut handler, &mut w, &mut events);
        assert_eq!(
            handler.roles.crossing_of(serial),
            None,
            "no crossing to wait for"
        );
        assert_eq!(
            w.units().unit_position(demon).map(|p| p.plane),
            Some(crate::map::Plane::Myrror),
            "it is across"
        );
    }

    /// A settler whose site no walk leads to waits for a ship with its site kept; the settler job
    /// does not choose another site for it.
    #[test]
    fn a_settler_with_no_walk_to_its_site_waits_for_a_ship_and_keeps_the_site() {
        use super::super::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // A site over the sea: a place for a city that no walk leads to.
        let site = super::super::testworld::site_over_the_sea(&w);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let serial = crate::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = handler.roles.claim(serial, Role::Settle, 1, Some(site));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut handler, &mut w, &mut events), 0);
        assert_eq!(handler.roles.crossing_of(serial), Some(Crossing::Sea));
        super::super::expand::settlers(&mut handler, &mut w, &mut events);
        assert_eq!(
            handler.roles.target_of(serial, Role::Settle),
            Some(site),
            "{events:?}"
        );
        assert_eq!(handler.roles.needing(Crossing::Sea), vec![(serial, site)]);
    }

    /// A settler whose site now holds a city (a rival settled first) is given up, and the soak
    /// is told that is why.
    #[test]
    fn a_settler_whose_site_a_rival_settled_first_is_given_up_for_that_reason() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let plain = std::rc::Rc::new(super::super::testworld::Plain::default());
        let mut handler = super::super::Handler::judged(P0, plain.clone());
        super::super::roles::register(&mut handler, &w, w.round());
        let serial = crate::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = handler.roles.claim(serial, Role::Settle, 1, Some(site));
        assert_eq!(revalidate(&mut handler, &mut w, &mut Vec::new()), 1);
        assert_eq!(
            *plain.fates.borrow(),
            [super::super::SettlerFate::SiteTaken]
        );
    }

    /// A settler whose site can no longer take a city (it lies by a city of its own) loses the site
    /// and the role together and awaits orders.
    #[test]
    fn a_settler_whose_site_cannot_take_a_city_loses_the_role_with_the_site() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(COAST_CITY.0 + 1, COAST_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let serial = crate::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = handler.roles.claim(serial, Role::Settle, 1, Some(site));
        assert_eq!(revalidate(&mut handler, &mut w, &mut Vec::new()), 1);
        assert_eq!(handler.roles.role_of(serial), Some(Role::AwaitingOrders));
        assert!(handler.roles.targets(Role::Settle).is_empty());
    }

    /// An engineer that is not building and has no walk goes on to its square, one that is
    /// building is left alone, and an escort apart from its charge walks to it.
    #[test]
    fn an_engineer_and_an_escort_that_lost_their_walk_are_sent_on() {
        use super::super::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = super::super::testworld::site_at_home(&w);
        let engineer = spawn(&mut w, P0, at(9, 7), 110 /* high_men_engineers */, 1)[0];
        let charge = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let escort = spawn(&mut w, P0, at(7, 6), 104, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        let view = crate::player::view::PlayerView::new(&w, P0);
        let (e, c, s) = (
            view.serial_of(engineer),
            view.serial_of(charge),
            view.serial_of(escort),
        );
        let _ = handler.roles.claim(e, Role::Road, 1, Some(at(5, 9)));
        let _ = handler.roles.claim(c, Role::Settle, 1, Some(site));
        let _ = handler.roles.claim(s, Role::Escort, 1, None);
        handler.roles.group(&[s], Role::Escort, 1, Some(c));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut handler, &mut w, &mut events), 0);
        let view = crate::player::view::PlayerView::new(&w, P0);
        let moved = |u| view.unit_has_order(u) || view.unit_position(u) != Some(at(9, 7));
        assert!(moved(engineer), "the engineer walks again");
        assert_eq!(view.unit_position(escort), Some(at(7, 6)));
        repair_escorts(&mut handler, &mut w, &mut events);
        let view = crate::player::view::PlayerView::new(&w, P0);
        assert!(
            view.unit_has_order(escort) || view.unit_position(escort) != Some(at(7, 6)),
            "the escort follows its charge"
        );
        assert!(handler.roles.is(s, Role::Escort));
    }
}
