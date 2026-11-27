//! A soldier on its way to a city holds `Role::Reinforce` with the city as its target; the walk
//! is repaired by `dispatch::revalidate`, and the role is given back here. Which city is sent whom
//! is the caller's choice.
//!
//! Tests: at the end of this file, and the full turns in `mod.rs`.

use crate::map::Position;
use crate::player::view::PlayerView;
use crate::world::World;

use super::roles::Role;
use super::Handler;

/// A soldier that stands in a city no enemy threatens, or whose city is no longer ours, has done
/// its task.
pub fn release(handler: &mut Handler, world: &World, threatened: &[(Position, f32)]) {
    let view = PlayerView::new(world, handler.player);
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    for u in view.my_units() {
        let serial = view.serial_of(u);
        let Some(to) = handler.roles.target_of(serial, Role::Reinforce) else {
            continue;
        };
        let arrived = view.unit_position(u) == Some(to);
        let danger = threatened.iter().any(|&(p, _)| p == to);
        if !cities.contains(&to) || (arrived && !danger) {
            handler.roles.release(serial, Role::Reinforce);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::handling::testworld::{at, game, handler, spawn, COAST_CITY, INLAND_CITY, P0};

    /// The task is over once the soldier stands in a city nobody threatens; it stays while the
    /// city is threatened.
    #[test]
    fn a_soldier_that_has_arrived_is_released_unless_the_city_is_threatened() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let unit = spawn(&mut w, P0, city, 104 /* high_men_spearmen */, 1)[0];
        let away = spawn(&mut w, P0, at(INLAND_CITY.0, INLAND_CITY.1), 104, 1)[0];
        let mut h = handler(P0);
        crate::unit::handling::roles::register(&mut h, &w, 1);
        let view = PlayerView::new(&w, P0);
        let (here, there) = (view.serial_of(unit), view.serial_of(away));
        // A garrison holds a better role than none: the two are sent as free soldiers are.
        for s in [here, there] {
            h.roles.release_any(s);
            h.roles
                .claim(s, Role::Reinforce, 1, Some(city))
                .expect("a free soldier");
        }
        release(&mut h, &w, &[(city, 1.0)]);
        assert!(h.roles.is(here, Role::Reinforce));
        release(&mut h, &w, &[]);
        assert!(!h.roles.is(here, Role::Reinforce));
        assert!(h.roles.is(there, Role::Reinforce), "still on its way");
    }
}
