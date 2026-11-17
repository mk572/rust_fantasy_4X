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
//! 0. a city in danger (the perimeter, inside the army's turn until task 646 moves it here);
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

/// One turn of the jobs, in the order of the checklist.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    crate::roles::register(ai, game);
    crate::recover::run(ai, game, events);
    crate::garrison::disperse(ai, game, events);
    crate::garrison::thin_out(ai, game, events);
    // 1. settlers, engineers, spirits
    crate::expand::settlers(ai, game, events);
    crate::road::run(ai, game, events);
    crate::meld::run(ai, game, events);
    // 2. escorts
    crate::escort::run(ai, game, events);
    // 3. scouts
    crate::scout::scout(ai, game, events);
    // 0, 4 to 6 (until task 646): raids, the army with the perimeter, the war
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
}
