//! Wounded units rest (task 502). A unit hurt below `rest_percent` percent of its hit points is no
//! use in a raid or a march: it heals 5% a turn in the field, 10% in a city, 17% in a city with an
//! Animists' Guild and 20% more beside a Healer. It takes the task `Rest` with what it does
//! decided (a healer, a city, out of the enemy's range, or staying where it is), and the plans
//! that pick troops (`military.rs`, `site.rs`, `garrison.rs`) leave it out until it has mended.
//!
//! Tests: below, on shipped units.

use crate::lever::Levered;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event};
use f4x_def::rules::Ability;

use crate::roles::Role;
use crate::view::{distance, is_fighter};
use crate::Ai;
use f4x_master::Game;

/// The Healer ability of the unit table.
pub(crate) const HEALER: Ability = Ability::new(27 /* healer */);

/// Is the unit hurt enough to rest?
pub(crate) fn needs_rest(view: &PlayerView, unit: hecs::Entity) -> bool {
    let (hits, max) = view.unit_hits(unit);
    max > 0 && hits * 100 < max * view.levers().care.rest_percent
}

/// What a wounded unit does while it mends.
#[derive(Debug, Clone, Copy)]
struct Choice {
    /// Where it rests: its own square when it stays.
    to: Position,
    /// The healer it goes to.
    healer: Option<Serial>,
}

/// Give each wounded unit out in the field the task to rest, with what it does decided
/// ([`choose`]), and let go of the raid or march it was on; end the rest of those that have
/// mended.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (mended, wounded): (Vec<hecs::Entity>, Vec<hecs::Entity>) = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        let (mut mended, mut wounded) = (Vec::new(), Vec::new());
        for u in view.my_units() {
            let serial = view.serial_of(u);
            let role = tasks.roles.role_of(serial);
            if !needs_rest(&view, u) {
                if role == Some(Role::Rest) {
                    mended.push(u);
                }
                continue;
            }
            // A unit with another task is left to it.
            let busy = role.is_some_and(|r| {
                !(r.is_base() || matches!(r, Role::Raid | Role::March | Role::Rest))
            });
            // In a city it mends where it stands, with the task it has.
            let sheltered = role != Some(Role::Rest)
                && view.unit_position(u).is_some_and(|p| {
                    view.city_at(p)
                        .is_some_and(|c| view.city_owner(c) == Some(me))
                });
            if is_fighter(&view, u) && !busy && !sheltered {
                wounded.push(u);
            }
        }
        (mended, wounded)
    };
    for unit in mended {
        let serial = PlayerView::new(game, me).serial_of(unit);
        ai.write(game).release(serial, Role::Rest);
        ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
    }
    for unit in wounded {
        let Some(choice) = choose(&PlayerView::new(game, me), unit) else {
            continue;
        };
        rest(ai, game, unit, choice, events);
    }
}

/// Record the choice and walk the unit, when it has somewhere to go.
fn rest(ai: &mut Ai, game: &mut Game, unit: hecs::Entity, choice: Choice, events: &mut Vec<Event>) {
    let (serial, at, ordered) = {
        let view = PlayerView::new(game, ai.player);
        (
            view.serial_of(unit),
            view.unit_position(unit),
            view.unit_has_order(unit),
        )
    };
    let same = {
        let tasks = ai.tasks(game);
        tasks.roles.target_of(serial, Role::Rest) == Some(choice.to)
            && tasks.roles.with_of(serial) == choice.healer
    };
    // On its way as chosen before: left alone.
    if same && (ordered || at == Some(choice.to)) {
        return;
    }
    ai.write(game).release_many(&[serial], Role::Raid);
    ai.write(game).release_many(&[serial], Role::March);
    if ai
        .write(game)
        .claim(serial, Role::Rest, Some(choice.to))
        .is_err()
    {
        return;
    }
    ai.write(game).unboard(serial);
    if let Some(healer) = choice.healer {
        ai.write(game).tie(serial, healer);
    }
    if ordered {
        ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
    }
    if at == Some(choice.to) {
        return;
    }
    if let Err(why) = ai.walk(game, vec![unit], events) {
        // No way there: it stays where it is. A full square on the way is tried again.
        if let (false, Some(here)) = (why.is_no_room(), at) {
            ai.write(game).unboard(serial);
            ai.write(game).set_target(serial, Role::Rest, Some(here));
        }
    }
}

/// What the wounded unit does. With no enemy near (`rest_enemy_near`): to the nearest healer
/// within `healer_range`, else to the nearest own city within `rest_city_range`, else it stays.
/// With one near, a healer or a city only when it is farther from the enemy than the unit is,
/// else out of range, and it stays when no square is.
fn choose(view: &PlayerView, unit: hecs::Entity) -> Option<Choice> {
    let at = view.unit_position(unit)?;
    let care = view.levers().care;
    let enemies = enemies_around(view, at, care.rest_enemy_near);
    // How near the nearest of those enemies is to a square.
    let gap = |p: Position| {
        enemies
            .iter()
            .filter_map(|&e| distance(view, p, e))
            .min()
            .unwrap_or(u32::MAX)
    };
    let here = gap(at);
    let safer = |p: Position| enemies.is_empty() || gap(p) > here;
    let mut reached = crate::view::ReachCache::default();
    // A square with no room for one more is no place to send it (`StackFull`); one it stands on
    // already needs none.
    let mut open = |p: Position| {
        p == at || (view.garrison(p).len() < view.max_tile() && reached.can_reach(view, &[unit], p))
    };
    let near = |p: Position| distance(view, at, p).map(|d| (d, p.y, p.x));
    let healer = view
        .my_units()
        .into_iter()
        .filter(|&u| u != unit && view.unit_has_ability(u, HEALER) && !needs_rest(view, u))
        .filter_map(|u| Some((view.unit_position(u)?, view.serial_of(u))))
        .filter(|&(p, _)| near(p).is_some_and(|k| k.0 <= care.healer_range))
        .filter(|&(p, _)| safer(p) && open(p))
        .min_by_key(|&(p, s)| (near(p), s));
    if let Some((to, serial)) = healer {
        return Some(Choice {
            to,
            healer: Some(serial),
        });
    }
    let city = view
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .filter(|&p| near(p).is_some_and(|k| k.0 <= care.rest_city_range))
        .filter(|&p| safer(p) && open(p))
        .min_by_key(|&p| near(p));
    let to = match city {
        Some(city) => city,
        None if enemies.is_empty() => at,
        None => out_of_range(view, at, care.rest_enemy_near, &mut open).unwrap_or(at),
    };
    Some(Choice { to, healer: None })
}

/// The squares within `range` of `at` that an enemy the player sees stands on.
fn enemies_around(view: &PlayerView, at: Position, range: i32) -> Vec<Position> {
    (-range..=range)
        .flat_map(|dy| (-range..=range).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| view.shift(at, dx, dy))
        .filter(|&p| view.hostile_at(p).is_some_and(|units| !units.is_empty()))
        .collect()
}

/// The nearest square the unit can walk to with no enemy within `range` of it.
fn out_of_range(
    view: &PlayerView,
    at: Position,
    range: i32,
    open: &mut impl FnMut(Position) -> bool,
) -> Option<Position> {
    // Out of range of an enemy beside it is at most twice the range away.
    (1..=range * 2).find_map(|ring| {
        (-ring..=ring)
            .flat_map(|dy| (-ring..=ring).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs().max(dy.abs()) == ring)
            .filter_map(|(dx, dy)| view.shift(at, dx, dy))
            .filter(|&p| {
                view.city_at(p).is_none()
                    && !view.has_lair(p)
                    && enemies_around(view, p, range).is_empty()
            })
            .find(|&p| open(p))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_unit_below_sixty_percent_rests_and_one_above_does_not() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let u = spawn(&mut w, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        assert!(!needs_rest(&PlayerView::new(&w, P0), u));
        let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 2 / 5 + 1);
        assert!(needs_rest(&PlayerView::new(&w, P0), u));
        w.scenario_set_hurt(u, max / 5);
        assert!(!needs_rest(&PlayerView::new(&w, P0), u));
        Ok(())
    }

    #[test]
    fn a_wounded_unit_in_the_field_walks_home_and_drops_its_raid() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let u = spawn(&mut w, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 3 / 4);
        let serial = PlayerView::new(&w, P0).serial_of(u);
        let mut ai = Ai::new(P0);
        let _ = ai
            .write(&mut w)
            .claim(serial, crate::roles::Role::Raid, None);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::Raid),
            "the raid is dropped"
        );
        assert!(
            PlayerView::new(&w, P0).unit_has_order(u),
            "it is on its way"
        );
        Ok(())
    }

    #[test]
    fn a_wounded_unit_far_from_every_city_or_over_the_sea_mends_where_it_stands() -> TestResult {
        use crate::testworld::ENEMY_CITY;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // On the other island: a city of its own is over the sea and far.
        let spot = at(ENEMY_CITY.0 + 1, ENEMY_CITY.1 + 2);
        let u = spawn(&mut w, P0, spot, 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 3 / 4);
        run(&mut Ai::new(P0), &mut w, &mut Vec::new());
        assert!(
            !PlayerView::new(&w, P0).unit_has_order(u),
            "no retreat that needs a ship or is further than {} squares",
            w.levers().care.rest_city_range
        );
        Ok(())
    }

    #[test]
    fn a_wounded_unit_in_a_city_stays_to_mend() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let u = spawn(&mut w, P0, city, 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 3 / 4);
        run(&mut Ai::new(P0), &mut w, &mut Vec::new());
        assert!(!PlayerView::new(&w, P0).unit_has_order(u));
        Ok(())
    }

    #[test]
    fn a_wounded_unit_is_not_sent_to_a_full_city_and_not_ordered_twice() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let u = spawn(&mut w, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 3 / 4);
        // The only city has no room for one more.
        let room = PlayerView::new(&w, P0).max_tile();
        spawn(&mut w, P0, city, 104 /* high_men_spearmen */, room);
        run(&mut Ai::new(P0), &mut w, &mut Vec::new());
        assert!(
            !PlayerView::new(&w, P0).unit_has_order(u),
            "no room: it stays"
        );
        // With room again it is sent once, and a second turn leaves its order alone.
        for _ in 0..3 {
            let last = w.units().units_at(city).pop().ok_or("a unit in the city")?;
            w.cheat_remove_unit(last);
        }
        let mut events = Vec::new();
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut events);
        assert!(PlayerView::new(&w, P0).unit_has_order(u));
        let before = events.len();
        run(&mut ai, &mut w, &mut events);
        assert_eq!(events.len(), before, "the order is not given again");
        Ok(())
    }

    /// A wounded soldier of `P0` on `spot`, and its serial.
    fn wounded(w: &mut Game, spot: Position) -> (hecs::Entity, Serial) {
        let u = spawn(w, P0, spot, 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(w, P0).unit_hits(u);
        w.scenario_set_hurt(u, max * 3 / 4);
        (u, PlayerView::new(w, P0).serial_of(u))
    }

    /// Where the unit rests, and the healer it is tied to.
    fn resting(ai: &Ai, w: &Game, unit: Serial) -> Option<(Position, Option<Serial>)> {
        let tasks = ai.tasks(w);
        let to = tasks.roles.target_of(unit, Role::Rest)?;
        Some((to, tasks.roles.with_of(unit)))
    }

    #[test]
    fn a_wounded_unit_with_nowhere_to_go_rests_where_it_is_until_it_has_mended() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // On the other island: no city of its own is near.
        use crate::testworld::ENEMY_CITY;
        let spot = at(ENEMY_CITY.0 + 1, ENEMY_CITY.1 + 2);
        let (u, serial) = wounded(&mut w, spot);
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(resting(&ai, &w, serial), Some((spot, None)));
        assert!(!PlayerView::new(&w, P0).unit_has_order(u));
        w.scenario_set_hurt(u, 0);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(!ai.tasks(&w).roles.any(Role::Rest));
    }

    #[test]
    fn a_wounded_unit_goes_to_a_healer_before_a_city_and_follows_it() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let spot = at(7, 6);
        let (u, serial) = wounded(&mut w, spot);
        let healer = spawn(&mut w, P0, at(5, 6), 7 /* healer */, 1)[0];
        let tied = PlayerView::new(&w, P0).serial_of(healer);
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(resting(&ai, &w, serial), Some((at(5, 6), Some(tied))));
        // The healer is somewhere else the next time.
        w.cheat_move_unit(healer, at(5, 8));
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(resting(&ai, &w, serial), Some((at(5, 8), Some(tied))));
        let view = PlayerView::new(&w, P0);
        assert!(
            view.unit_position(u) == Some(at(5, 8)) || view.unit_destination(u) == Some(at(5, 8))
        );
        Ok(())
    }

    #[test]
    fn a_wounded_unit_with_an_enemy_near_and_no_safer_refuge_gets_out_of_range() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let spot = at(6, 6);
        // The enemy stands between the unit and the city.
        let enemy = at(8, 6);
        spawn(&mut w, P1, enemy, 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        let (_, serial) = wounded(&mut w, spot);
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let range = w.levers().care.rest_enemy_near;
        let (to, healer) = resting(&ai, &w, serial).ok_or("it rests")?;
        assert_eq!(healer, None);
        let away = PlayerView::new(&w, P0)
            .distance(to, enemy)
            .ok_or("one plane")?;
        assert!(i64::from(away) > i64::from(range), "{to:?}");
        Ok(())
    }

    #[test]
    fn a_wounded_unit_with_an_enemy_near_takes_the_city_that_lies_away_from_it() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let spot = at(7, 6);
        spawn(&mut w, P1, at(5, 6), 104 /* high_men_spearmen */, 1);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        let (_, serial) = wounded(&mut w, spot);
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let city = at(COAST_CITY.0, COAST_CITY.1);
        assert_eq!(resting(&ai, &w, serial), Some((city, None)));
    }
}
