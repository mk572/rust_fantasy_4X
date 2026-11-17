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
//! The order of the checklist:
//!
//! 0. a city in danger (the perimeter: the free soldiers go for the enemy in its zone, or join the
//!    garrison when it is the stronger);
//! 1. settlers to their sites, engineers to their roads, spirits to their nodes;
//! 2. escorts for the settlers and engineers whose way is not safe;
//! 3. scouts for unexplored land, the fast units first;
//! 4. the war: the rally, the army against an enemy city (task 646);
//! 5. raiding parties for a lair (646);
//! 6. the other under-defended cities (646);
//!
//! Then come the ships and the colonies (task 647) and the audit that counts what is left.

use crate::Ai;
use f4x_core::game::{Event, Game};

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
pub fn pool(ai: &Ai, game: &Game) -> Pool {
    use crate::view::{is_fighter, is_ship};
    let view = f4x_core::player::view::PlayerView::new(game, ai.player);
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
        Role::Scout => !view.explored(target),
        Role::Meld => crate::meld::open_nodes(ai, view).contains(&target),
        _ => true,
    }
}

/// Before any new job is looked for: a unit sent on a mission that can no longer be fulfilled
/// (a rival cleared the lair first, the order was stopped on a refused step, the ground was
/// explored by another) is given up. It loses its role and its order and awaits orders where it
/// stands, so the jobs below see it in the pool. Returns how many were reset.
pub fn revalidate(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) -> u32 {
    use crate::roles::Role;
    let me = ai.player;
    let mut reset: Vec<(hecs::Entity, crate::roles::Role)> = Vec::new();
    {
        let view = f4x_core::player::view::PlayerView::new(game, me);
        for unit in view.my_units() {
            let serial = view.serial_of(unit);
            for role in [Role::Raid, Role::Scout, Role::Meld] {
                if !ai.roles.is(serial, role) {
                    continue;
                }
                let Some(target) = ai.roles.target_of(serial, role) else {
                    continue;
                };
                let here = view.unit_position(unit);
                // Failed: no order is left and the unit is not where it was going.
                // A raid fights the site from the square beside it, so that counts as arrived.
                let reach = u32::from(role == Role::Raid);
                let arrived = here.is_some_and(|h| {
                    h == target
                        || (reach > 0
                            && crate::view::distance(&view, h, target).is_some_and(|d| d <= reach))
                });
                let stopped = !view.unit_has_order(unit) && !arrived;
                if stopped || !mission_valid(ai, &view, role, target) {
                    reset.push((unit, role));
                }
            }
        }
    }
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
            f4x_core::game::Command::ClearOrders { units: serials },
            events,
        );
    }
    ai.missions_reset += count;
    count
}

/// The soldiers of the pool that can act: fighters that are free (or gather for the campaign),
/// are not resting and stand somewhere.
fn free_soldiers(ai: &Ai, game: &Game) -> Vec<hecs::Entity> {
    use crate::roles::{MusterKind, Role};
    let view = f4x_core::player::view::PlayerView::new(game, ai.player);
    view.my_units()
        .into_iter()
        .filter(|&u| crate::view::is_fighter(&view, u) && view.unit_position(u).is_some())
        .filter(|&u| !crate::recover::needs_rest(&view, u))
        .filter(|&u| {
            let s = view.serial_of(u);
            ai.roles.is_free(s)
                || ai.roles.is(s, Role::Perimeter)
                || ai.roles.in_muster(s, MusterKind::Campaign)
        })
        .collect()
}

/// One turn of the jobs, in the order of the checklist.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    crate::roles::register(ai, game);
    // First the units whose mission is over, so the jobs below see them waiting.
    revalidate(ai, game, events);
    crate::recover::run(ai, game, events);
    crate::garrison::disperse(ai, game, events);
    crate::garrison::thin_out(ai, game, events);
    // 0. a city in danger: the free soldiers deal with the enemy in its zone, before anything else
    {
        let free = free_soldiers(ai, game);
        crate::perimeter::run(ai, game, free, events);
    }
    // 1. settlers, engineers, spirits
    crate::expand::settlers(ai, game, events);
    crate::road::run(ai, game, events);
    crate::meld::run(ai, game, events);
    // 2. escorts
    crate::escort::run(ai, game, events);
    // 3. scouts
    crate::scout::scout(ai, game, events);
    // 4 to 6 (until task 646): raids, the army, the war
    crate::site::run(ai, game, events);
    crate::military::army(ai, game, events);
    // The ships and the colonies (task 647)
    crate::naval::run(ai, game, events);
    crate::colony::run(ai, game, events);
    crate::pickup::run(ai, game, events);
    crate::roles::audit(ai, game, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    /// A city keeps the soldiers its defence needs; the rest, and a lone soldier in the field,
    /// are the pool.
    #[test]
    fn the_pool_is_the_soldiers_beyond_the_garrison_and_the_idle_others() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        spawn(&mut g, P0, city, 104 /* high_men_spearmen */, 3);
        spawn(&mut g, P0, at(8, 6), 104, 1);
        spawn(&mut g, P0, at(9, 6), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        let wanted =
            crate::view::garrison_wanted(&f4x_core::player::view::PlayerView::new(&g, P0), city);
        let counted = pool(&ai, &g);
        assert_eq!(counted.soldiers, 3 - wanted + 1, "{counted:?}");
        assert_eq!(counted.others, 1);
        assert_eq!(counted.ships, 0);
    }

    /// A unit with nothing to do stays where it is: the turn sends it nowhere.
    #[test]
    fn a_waiting_soldier_with_no_job_is_left_where_it_stands() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let lone = spawn(&mut g, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut g, &mut events);
        let view = f4x_core::player::view::PlayerView::new(&g, P0);
        // The map is fully known, so there is nothing to scout; nobody is at war.
        assert!(!view.unit_has_order(lone), "{events:?}");
        assert_eq!(view.unit_position(lone), Some(at(8, 6)));
    }

    /// A scout whose ground was explored meanwhile, and one whose order stopped short, are given
    /// up and await orders; one still on its way keeps its mission.
    #[test]
    fn a_unit_whose_mission_is_over_awaits_orders_and_one_on_its_way_keeps_it() {
        use crate::roles::Role;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let target = at(30, 10);
        let walker = spawn(&mut g, P0, at(9, 6), 104, 1)[0];
        let stopped = spawn(&mut g, P0, at(9, 7), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        let view = f4x_core::player::view::PlayerView::new(&g, P0);
        let (a, b) = (view.serial_of(walker), view.serial_of(stopped));
        assert!(!view.explored(target), "unexplored ground to scout");
        for s in [a, b] {
            let _ = ai.roles.claim(s, Role::Scout, 1, Some(target));
        }
        let mut events = Vec::new();
        ai.send(
            &mut g,
            f4x_core::game::Command::GoTo {
                units: vec![walker],
                to: at(5, 9),
            },
            &mut events,
        )
        .expect("the walker can walk");
        // `walker` has an order and valid ground; `stopped` has no order and is not there.
        assert_eq!(revalidate(&mut ai, &mut g, &mut events), 1);
        assert!(ai.roles.is(a, Role::Scout));
        assert!(!ai.roles.is(b, Role::Scout));
        // The ground becomes known: the walker's mission is over too.
        g.reveal_map(P0);
        assert_eq!(revalidate(&mut ai, &mut g, &mut events), 1);
        assert!(!ai.roles.is(a, Role::Scout));
        let view = f4x_core::player::view::PlayerView::new(&g, P0);
        assert!(!view.unit_has_order(walker));
        assert_eq!(ai.missions_reset, 2);
    }
}
