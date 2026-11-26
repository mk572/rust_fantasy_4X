//! The job board (task 645, `docs/ai-design.md` section 14). A unit with nothing to do awaits
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

use crate::Ai;
use f4x_core::world::{Event, World};
use f4x_master::Game;

/// What waits for a job: the units of the pool, by kind (read by the jobs of task 646; for now
/// only the tests count it).
#[cfg(test)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pool {
    /// Soldiers (fighters) standing free, in a city beyond its garrison or in the field.
    pub soldiers: usize,
    /// Free units that are not fighters and not ships: settlers, engineers, spirits.
    pub others: usize,
    /// Ships with no mission.
    pub ships: usize,
}

/// Count the pool as the registry holds it (`roles::register` has run).
#[cfg(test)]
pub fn pool(ai: &Ai, world: &World) -> Pool {
    use crate::view::{is_fighter, is_ship};
    let view = f4x_core::player::view::PlayerView::new(world, ai.player);
    let mut pool = Pool::default();
    for unit in view.my_units() {
        let serial = view.serial_of(unit);
        if is_ship(&view, unit) {
            pool.ships += usize::from(ai.roles.is(serial, crate::roles::Role::TransportFree));
        } else if ai.roles.is_free(serial) {
            if is_fighter(&view, unit) {
                pool.soldiers += 1;
            } else {
                pool.others += 1;
            }
        }
    }
    pool
}

/// Whether the mission a unit holds is still worth pursuing: the lair a raid went for is still
/// there (or the tower intact, the neutral city standing), the ground a scout went to is still
/// unexplored, the node a spirit went to is still open. Missions of other roles are checked by
/// their own modules each turn.
fn mission_valid(
    ai: &Ai,
    view: &f4x_core::player::view::PlayerView,
    role: crate::roles::Role,
    target: f4x_core::map::Position,
    round: u32,
) -> bool {
    use crate::roles::Role;
    match role {
        Role::Raid => {
            view.has_lair(target)
                || view
                    .towers()
                    .into_iter()
                    .any(|(p, intact)| p == target && intact)
                || view.known_cities().into_iter().any(|(_, owner, p)| {
                    p == target && !view.is_rival_wizard(owner) && owner != ai.player
                })
        }
        // A target with a city on it is dropped (a landing square of a voyage is no site: the settler
        // looks for one ashore).
        // The site must still take a city and a way lead to it (on foot or by ship); the way is
        // worked out on the day of the site check only (`city_site.rs`).
        Role::Settle => {
            view.city_at(target).is_none()
                && view.can_found_city(target)
                && (!ai.sites.rebuilt_on(round)
                    || crate::city_site::site_reach(view, target).is_some())
        }
        Role::Scout => !view.explored(target),
        Role::Meld => crate::meld::open_nodes(ai, view).contains(&target),
        Role::Reinforce => view.my_cities().into_iter().any(|(_, p)| p == target),
        _ => true,
    }
}

/// Send units that lost their walk to their target again. A unit the game says no walk leads to
/// is tagged with what it needs (`Crossing`: another plane, or the sea when the target is on this
/// plane with no known land route) and keeps its role. The units to give up are returned: those
/// refused for another reason (no moves left, say).
fn resend(
    ai: &mut Ai,
    game: &mut Game,
    lost: Vec<(hecs::Entity, crate::roles::Role, f4x_core::map::Position)>,
    events: &mut Vec<Event>,
) -> Vec<(hecs::Entity, crate::roles::Role)> {
    use crate::roles::Role;
    use crate::SendError;
    use f4x_core::unit::movement::MoveError;
    use f4x_core::world::Rejection;
    let mut refused = Vec::new();
    for (unit, role, target) in lost {
        // A unit with planar travel of its own (a shadow demon, or any unit with the spell or an
        // item) changes plane where it stands, with no tower, gate or spell to wait for.
        shift_by_itself(ai, game, unit, target, events);
        // `Err(true)`: no walk leads there; `Err(false)`: refused for another reason.
        let sent: Result<(), bool> = match role {
            Role::Raid | Role::March => crate::assault::march(ai, game, &[unit], target, events)
                .map_err(|why| why == crate::assault::MarchError::Unreachable),
            _ => ai
                .probe(
                    game,
                    f4x_core::world::Command::GoTo {
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
                }),
        };
        let unreachable = match sent {
            Ok(()) => continue,
            Err(no_walk) => no_walk,
        };
        let (serial, here) = {
            let view = f4x_core::player::view::PlayerView::new(game, ai.player);
            (view.serial_of(unit), view.unit_position(unit))
        };
        match here {
            Some(h) if h.plane != target.plane => {
                ai.roles
                    .set_crossing(serial, Some(crate::roles::Crossing::Plane));
            }
            // Only a walker waits for a ship; a flier or a swimmer crosses the water itself.
            Some(_)
                if unreachable
                    && f4x_core::player::view::PlayerView::new(game, ai.player).is_walker(unit) =>
            {
                ai.roles
                    .set_crossing(serial, Some(crate::roles::Crossing::Sea));
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
    ai: &mut Ai,
    game: &mut Game,
    unit: hecs::Entity,
    target: f4x_core::map::Position,
    events: &mut Vec<Event>,
) {
    let shifts = {
        let view = f4x_core::player::view::PlayerView::new(game, ai.player);
        view.unit_position(unit)
            .is_some_and(|p| p.plane != target.plane)
            && view.unit_has_flag(unit, f4x_def::rules::unit_flag::UnitFlag::PlanarTravel)
            && view.can_shift_plane(&[unit])
    };
    if shifts {
        let _ = ai.send(
            game,
            f4x_core::world::Command::ShiftPlane { units: vec![unit] },
            events,
        );
    }
}

/// An escort that fell behind its charge (the stack was split by a fight, or the charge was
/// ordered on by another job) goes to where the charge stands now. Run after the jobs that move
/// the charges, so it follows their new square. An escort the game will not send keeps its role.
pub fn repair_escorts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    use crate::roles::Role;
    let lost: Vec<_> = {
        let view = f4x_core::player::view::PlayerView::new(game, ai.player);
        let find = |s| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        };
        ai.roles
            .members(Role::Escort)
            .into_iter()
            .filter(|&e| ai.roles.crossing_of(e).is_none())
            .filter_map(|e| {
                let charge = find(ai.roles.with_of(e)?)?;
                let unit = find(e)?;
                let (there, here) = (view.unit_position(charge)?, view.unit_position(unit)?);
                (there != here && !view.unit_has_order(unit)).then_some((unit, Role::Escort, there))
            })
            .collect()
    };
    resend(ai, game, lost, events);
}

/// Before any new job is looked for: a unit sent on a mission that can no longer be fulfilled
/// (a rival cleared the lair first, the ground was explored by another) is given up. A unit whose
/// walk is gone (it was moved by a strike, a battle, a refused step) but whose target stands in
/// the registry is sent again; it is given up only when the game refuses. It loses its role and its order and awaits orders where it
/// stands, so the jobs below see it in the pool. Returns how many were reset.
pub fn revalidate(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) -> u32 {
    use crate::roles::Role;
    let me = ai.player;
    let mut reset: Vec<(hecs::Entity, crate::roles::Role)> = Vec::new();
    let mut lost: Vec<(hecs::Entity, crate::roles::Role, f4x_core::map::Position)> = Vec::new();
    {
        let view = f4x_core::player::view::PlayerView::new(game, me);
        for unit in view.my_units() {
            let serial = view.serial_of(unit);
            for role in [
                Role::Raid,
                Role::Scout,
                Role::Meld,
                Role::March,
                Role::Settle,
                Role::Road,
                Role::Reinforce,
            ] {
                if !ai.roles.is(serial, role) {
                    continue;
                }
                let Some(target) = ai.roles.target_of(serial, role) else {
                    continue;
                };
                let here = view.unit_position(unit);
                // Failed: no order is left and the unit is not where it was going.
                // A raid fights the site from the square beside it, so that counts as arrived.
                let reach = u32::from(matches!(role, Role::Raid | Role::March));
                let arrived = here.is_some_and(|h| {
                    h == target
                        || (reach > 0
                            && crate::view::distance(&view, h, target).is_some_and(|d| d <= reach))
                });
                // A ship that has sailed to its scouting target has explored it by arriving, and its
                // mission is not over: `scout::scout` puts the party aboard ashore and picks the
                // next target. Resetting it here left the party waiting for a ship that no longer
                // scouted.
                let arrived_ship =
                    role == Role::Scout && arrived && crate::view::is_ship(&view, unit);
                if !arrived_ship && !mission_valid(ai, &view, role, target, game.round()) {
                    if role == Role::Settle {
                        ai.settler_fates.note(if view.city_at(target).is_some() {
                            crate::settler_fate::SettlerFate::SiteTaken
                        } else {
                            crate::settler_fate::SettlerFate::SiteUnusable
                        });
                    }
                    reset.push((unit, role));
                } else if !view.unit_has_order(unit)
                    && !view.unit_builds_road(unit)
                    && !arrived
                    && ai.roles.crossing_of(serial).is_none()
                {
                    // A unit can lose its walk for many reasons (a strike moved it, a battle,
                    // a refused step): the registry still knows where it goes.
                    lost.push((unit, role, target));
                }
            }
        }
    }
    reset.extend(resend(ai, game, lost, events));
    let mut serials = Vec::new();
    {
        let view = f4x_core::player::view::PlayerView::new(game, me);
        for &(unit, role) in &reset {
            let serial = view.serial_of(unit);
            ai.roles.release(serial, role);
            serials.push(unit);
        }
    }
    let count = serials.len() as u32;
    if !serials.is_empty() {
        ai.order(
            game,
            f4x_core::world::Command::ClearOrders { units: serials },
            events,
        );
    }
    ai.missions_reset += count;
    count
}

/// The fighters that are rested and stand somewhere, of the roles `role_ok` allows.
pub(crate) fn soldiers(
    ai: &Ai,
    world: &World,
    role_ok: impl Fn(&crate::roles::Registry, f4x_core::world::serial::Serial) -> bool,
) -> Vec<hecs::Entity> {
    let view = f4x_core::player::view::PlayerView::new(world, ai.player);
    view.my_units()
        .into_iter()
        .filter(|&u| crate::view::is_fighter(&view, u) && view.unit_position(u).is_some())
        .filter(|&u| !crate::recover::needs_rest(&view, u))
        .filter(|&u| role_ok(&ai.roles, view.serial_of(u)))
        .collect()
}

/// The soldiers of the pool that the danger job (job 0) may take: the free ones, those that
/// gather for the campaign and those that hold the perimeter already.
fn free_soldiers(ai: &Ai, world: &World) -> Vec<hecs::Entity> {
    use crate::roles::{MusterKind, Role};
    soldiers(ai, world, |roles, s| {
        roles.is_free(s) || roles.is(s, Role::Perimeter) || roles.in_muster(s, MusterKind::Campaign)
    })
}

/// One turn of the jobs, in the order of the checklist.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    crate::roles::register(ai, game, game.round());
    // The sweep of the ship ties (and the sanity check of the passengers), before any job moves
    // one of them.
    crate::pickup::tidy_unit_registry(ai, game);
    // Quick strikes come first: no unit has used a move or got an order yet.
    crate::strike::run(ai, game, events);
    // First the units whose mission is over, so the jobs below see them waiting.
    revalidate(ai, game, events);
    crate::recover::run(ai, game, events);
    crate::garrison::thin_out(ai, game, events);
    // 0. a city in danger: the free soldiers deal with the enemy in its zone, before anything else
    {
        let free = free_soldiers(ai, game);
        crate::perimeter::run(ai, game, free, events);
    }
    // 0b. an own city that holds less than it wants: the nearest free soldiers go in
    crate::reinforce::run(ai, game, events);
    // 0b. an own city that holds less than it wants: the nearest free soldiers go in
    // 1. settlers, engineers, spirits
    crate::expand::settlers(ai, game, events);
    crate::road::run(ai, game, events);
    crate::meld::run(ai, game, events);
    // 2. escorts
    crate::escort::run(ai, game, events);
    repair_escorts(ai, game, events);
    // 3. scouts
    crate::scout::scout(ai, game, events);
    // 4. the war (task 646): the army takes the soldiers its target needs, none at peace
    crate::war::run(ai, game, events);
    // 5. raiding parties from the soldiers the war left over
    crate::raid::run(ai, game, events);
    // The ships (task 647): the bridges first, since a free ship goes to the bridge before it
    // scouts (task 739)
    crate::bridge::run(ai, game, events);
    crate::naval::run(ai, game, events);
    crate::pickup::run(ai, game, events);
    // The orders a full square stopped go again, now that the units that could move have moved.
    ai.retry_blocked(game, events);
    crate::roles::audit(ai, game, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, ENEMY_CITY, P0, P1};
    use f4x_master::Game;

    /// A city keeps the soldiers its defence needs; the rest, and a lone soldier in the field,
    /// are the pool.
    #[test]
    fn the_pool_is_the_soldiers_beyond_the_garrison_and_the_idle_others() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut w, P0, city, 104 /* high_men_spearmen */, 3);
        spawn(&mut w, P0, at(8, 6), 104, 1);
        spawn(&mut w, P0, at(9, 6), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let wanted =
            crate::view::garrison_wanted(&f4x_core::player::view::PlayerView::new(&w, P0), city);
        let counted = pool(&ai, &w);
        assert_eq!(counted.soldiers, 3 - wanted + 1, "{counted:?}");
        assert_eq!(counted.others, 1);
        assert_eq!(counted.ships, 0);
    }

    /// A unit with nothing to do stays where it is: the turn sends it nowhere.
    #[test]
    fn a_waiting_soldier_with_no_job_is_left_where_it_stands() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // The city holds the one soldier its defence wants: the lone one is not needed there.
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 1);
        let lone = spawn(&mut w, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        // The map is fully known, so there is nothing to scout; nobody is at war.
        assert!(!view.unit_has_order(lone), "{events:?}");
        assert_eq!(view.unit_position(lone), Some(at(8, 6)));
    }

    /// With the same soldiers and the same lair, a raid party forms at peace, and at war the
    /// campaign holds the soldiers and the lair is left alone. (This does not tell the order of
    /// the two jobs: the raid needs a few turns to form, the muster none; the order is in
    /// `run`.)
    #[test]
    fn at_war_the_soldiers_gather_for_the_campaign_and_leave_the_lair_alone(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::roles::{MusterKind, Role};
        let build = |war: bool| -> Result<(Game, Ai), Box<dyn std::error::Error>> {
            let mut w = game(COAST_CITY, 8 /* high_men */);
            if war {
                // A rival's town that P0 knows and is at war with.
                w.make_contact(P0, P1);
                let town = at(4, 4);
                w.apply(f4x_core::world::Command::FoundCity {
                    owner: P1,
                    pos: town,
                    name: "Town".into(),
                    race: w.content().rules.races.id_numbered(8),
                    population: 8,
                });
                w.explore(P0, town, f4x_def::half_int::HalfInt::from_raw(3));
                w.declare_war(P0, P1);
            }
            spawn(
                &mut w,
                P0,
                at(COAST_CITY.0, COAST_CITY.1),
                113, /* paladins */
                6,
            );
            crate::testworld::spawn_site(
                &mut w,
                at(COAST_CITY.0 - 2, COAST_CITY.1),
                vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
                Vec::new(),
            )?;
            w.reveal_map(P0);
            Ok((w, Ai::new(P0)))
        };
        // Whether a raid party was formed in any of the turns (a short one may be over by the end).
        let turns = |w: &mut Game, ai: &mut Ai, turns_run: usize| {
            let mut raided = false;
            for _ in 0..turns_run {
                w.balance_farmers(P0);
                run(ai, w, &mut Vec::new());
                raided |= ai.roles.any(Role::Raid);
                w.play_round();
            }
            raided
        };
        let (mut w, mut ai) = build(false)?;
        assert!(turns(&mut w, &mut ai, 6), "at peace the soldiers raid");
        assert!(!ai.roles.any_muster(MusterKind::Campaign));
        let (mut w, mut ai) = build(true)?;
        let raided = turns(&mut w, &mut ai, 6);
        assert!(!raided, "at war the lair is a distraction");
        assert!(
            ai.roles.any_muster(MusterKind::Campaign) || ai.roles.any(Role::March),
            "at war the campaign holds them"
        );
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        for s in ai.roles.members(Role::Raid) {
            assert!(!ai.roles.in_muster(s, MusterKind::Campaign));
            let _ = &view;
        }
        Ok(())
    }

    /// A scout whose ground was explored meanwhile is given up and awaits orders; one still on
    /// its way keeps its mission, and one with no walk to its target keeps it too, tagged with
    /// the ship it needs.
    #[test]
    fn a_unit_whose_mission_is_over_awaits_orders_and_one_that_cannot_walk_waits_for_a_ship() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(30, 10);
        let walker = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let stopped = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (a, b) = (view.serial_of(walker), view.serial_of(stopped));
        assert!(!view.explored(target), "unexplored ground to scout");
        for s in [a, b] {
            let _ = ai.roles.claim(s, Role::Scout, 1, Some(target));
        }
        let mut events = Vec::new();
        ai.send(
            &mut w,
            f4x_core::world::Command::GoTo {
                units: vec![walker],
                to: at(5, 9),
            },
            &mut events,
        )
        .expect("the walker can walk");
        // `walker` has an order and valid ground; `stopped` has no order and no walk leads to
        // its target (across the sea): it keeps its mission and waits for a ship.
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert!(ai.roles.is(a, Role::Scout));
        assert!(ai.roles.is(b, Role::Scout));
        assert_eq!(ai.roles.crossing_of(b), Some(crate::roles::Crossing::Sea));
        assert_eq!(ai.roles.crossing_of(a), None);
        // The ground becomes known: both missions are over, the flag goes with the role.
        w.reveal_map(P0);
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 2);
        assert!(!ai.roles.is(a, Role::Scout));
        assert!(!ai.roles.is(b, Role::Scout));
        assert_eq!(ai.roles.crossing_of(b), None);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(!view.unit_has_order(walker));
        assert_eq!(ai.missions_reset, 2);
    }

    /// A unit that lost its walk (a strike moved it, a battle) but whose target is in the
    /// registry is sent on again, not given up.
    #[test]
    fn a_unit_that_lost_its_walk_is_sent_on_to_the_target_in_the_registry() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let target = at(5, 9);
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let serial = view.serial_of(scout);
        assert!(!view.explored(target), "unexplored ground to scout");
        let _ = ai.roles.claim(serial, Role::Scout, 1, Some(target));
        assert!(!view.unit_has_order(scout), "no walk");
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0, "not given up");
        assert!(ai.roles.is(serial, Role::Scout));
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
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
        use crate::roles::Role;
        use f4x_def::half_int::HalfInt;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // Off the east island's west shore, as in `scout`'s own landing test.
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (ship, rider) = (view.serial_of(boat), view.serial_of(walker));
        assert!(view.explored(sea), "arriving explored the target");
        let _ = ai.roles.claim(ship, Role::Scout, 1, Some(sea));
        let _ = ai.roles.claim(rider, Role::SeaParty, 1, None);
        ai.sea_scouting = true;
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 0);
        assert!(ai.roles.is(ship, Role::Scout), "the ship goes on scouting");
        crate::scout::scout(&mut ai, &mut w, &mut Vec::new());
        assert!(!ai.roles.is(rider, Role::SeaParty), "the rider went ashore");
        assert_ne!(w.units().unit_position(walker), Some(sea));
    }

    /// A target on the other plane needs a crossing of the planes; the unit keeps its role and its
    /// target and is not sent again every turn.
    #[test]
    fn a_unit_whose_target_is_on_the_other_plane_is_tagged_with_a_plane_crossing() {
        use crate::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = f4x_core::map::Plane::Myrror;
        let scout = spawn(&mut w, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let serial = view.serial_of(scout);
        assert!(!view.explored(target));
        let _ = ai.roles.claim(serial, Role::Scout, 1, Some(target));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert_eq!(ai.roles.crossing_of(serial), Some(Crossing::Plane));
        assert_eq!(ai.roles.needing(Crossing::Plane), vec![(serial, target)]);
        // A new target clears the tag.
        ai.roles.set_target(serial, Role::Scout, Some(at(5, 9)));
        assert_eq!(ai.roles.crossing_of(serial), None);
    }

    /// A unit that travels between the planes by itself shifts and walks on: it is not tagged.
    #[test]
    fn a_unit_with_planar_travel_shifts_itself_instead_of_waiting_for_a_crossing() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let mut target = at(5, 9);
        target.plane = f4x_core::map::Plane::Myrror;
        let demon = spawn(&mut w, P0, at(9, 7), 171 /* shadow_demons */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(demon);
        let _ = ai.roles.claim(serial, Role::Scout, 1, Some(target));
        let mut events = Vec::new();
        revalidate(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.roles.crossing_of(serial),
            None,
            "no crossing to wait for"
        );
        assert_eq!(
            w.units().unit_position(demon).map(|p| p.plane),
            Some(f4x_core::map::Plane::Myrror),
            "it is across"
        );
    }

    /// A settler whose site no walk leads to waits for a ship with its site kept; the settler job
    /// does not choose another site for it.
    #[test]
    fn a_settler_with_no_walk_to_its_site_waits_for_a_ship_and_keeps_the_site() {
        use crate::roles::{Crossing, Role};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        // A site over the sea: a place for a city that no walk leads to.
        let site = crate::city_site::scan(&f4x_core::player::view::PlayerView::new(&w, P0))
            .into_iter()
            .find(|s| s.reach == crate::city_site::Reach::Sea)
            .expect("a site over the sea")
            .at;
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.roles.claim(serial, Role::Settle, 1, Some(site));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        assert_eq!(ai.roles.crossing_of(serial), Some(Crossing::Sea));
        crate::expand::settlers(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.roles.target_of(serial, Role::Settle),
            Some(site),
            "{events:?}"
        );
        assert_eq!(ai.roles.needing(Crossing::Sea), vec![(serial, site)]);
    }

    /// A settler whose site now holds a city (a rival settled first) is given up, and the soak
    /// is told that is why.
    #[test]
    fn a_settler_whose_site_a_rival_settled_first_is_given_up_for_that_reason() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.roles.claim(serial, Role::Settle, 1, Some(site));
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 1);
        let said: Vec<_> = ai
            .settler_failures()
            .into_iter()
            .filter(|&(_, n)| n > 0)
            .collect();
        assert_eq!(said, [("site settled by someone else", 1)]);
    }

    /// A settler whose site can no longer take a city (it lies by a city of its own) loses the site
    /// and the role together and awaits orders.
    #[test]
    fn a_settler_whose_site_cannot_take_a_city_loses_the_role_with_the_site() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = at(COAST_CITY.0 + 1, COAST_CITY.1);
        let settler = spawn(&mut w, P0, at(9, 7), 111 /* high_men_settlers */, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let serial = f4x_core::player::view::PlayerView::new(&w, P0).serial_of(settler);
        let _ = ai.roles.claim(serial, Role::Settle, 1, Some(site));
        assert_eq!(revalidate(&mut ai, &mut w, &mut Vec::new()), 1);
        assert_eq!(ai.roles.role_of(serial), Some(Role::AwaitingOrders));
        assert!(ai.roles.targets(Role::Settle).is_empty());
    }

    /// An engineer that is not building and has no walk goes on to its square, one that is
    /// building is left alone, and an escort apart from its charge walks to it.
    #[test]
    fn an_engineer_and_an_escort_that_lost_their_walk_are_sent_on() {
        use crate::roles::Role;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = crate::city_site::scan(&f4x_core::player::view::PlayerView::new(&w, P0))
            .first()
            .expect("a site")
            .at;
        let engineer = spawn(&mut w, P0, at(9, 7), 110 /* high_men_engineers */, 1)[0];
        let charge = spawn(&mut w, P0, at(9, 6), 104, 1)[0];
        let escort = spawn(&mut w, P0, at(7, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let (e, c, s) = (
            view.serial_of(engineer),
            view.serial_of(charge),
            view.serial_of(escort),
        );
        let _ = ai.roles.claim(e, Role::Road, 1, Some(at(5, 9)));
        let _ = ai.roles.claim(c, Role::Settle, 1, Some(site));
        let _ = ai.roles.claim(s, Role::Escort, 1, None);
        ai.roles.group(&[s], Role::Escort, 1, Some(c));
        let mut events = Vec::new();
        assert_eq!(revalidate(&mut ai, &mut w, &mut events), 0);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        let moved = |u| view.unit_has_order(u) || view.unit_position(u) != Some(at(9, 7));
        assert!(moved(engineer), "the engineer walks again");
        assert_eq!(view.unit_position(escort), Some(at(7, 6)));
        repair_escorts(&mut ai, &mut w, &mut events);
        let view = f4x_core::player::view::PlayerView::new(&w, P0);
        assert!(
            view.unit_has_order(escort) || view.unit_position(escort) != Some(at(7, 6)),
            "the escort follows its charge"
        );
        assert!(ai.roles.is(s, Role::Escort));
    }
}
