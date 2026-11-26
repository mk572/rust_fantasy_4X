//! The under-defended cities (task 646, job 0b of the dispatcher, after the perimeter): a city
//! that holds less than it wants, or less than the enemy near it is worth, is sent soldiers from
//! the pool, the nearest first. They hold `Role::Reinforce` with the city as their target until
//! they stand in it; the registry's walk repair (`dispatch::revalidate`) keeps them going.
//!
//! What a city wants is the larger of its defence strength in peace (`garrison::wanted_strength`)
//! and what the enemy near it is worth (`military::threats`). The soldiers on their way count as
//! held, so a city is not sent the same help twice. While an enemy threatens the city, the free
//! soldiers standing in it hold the role too, so that no raid or scout takes them out. A city
//! that cannot be saved is helped all the same: the AI has no better judgement of a lost cause.

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event, World};

use crate::roles::Role;
use crate::view::{fighting_strength, is_fighter, unit_strength, ReachCache};
use crate::Ai;

/// One turn: the soldiers that are no longer needed are released, the free ones in a threatened
/// city stay, and each city that wants more is sent some.
pub fn run(ai: &mut Ai, world: &mut World, events: &mut Vec<Event>) {
    let me = ai.player;
    let threatened: Vec<(Position, f32)> = crate::military::threats(&PlayerView::new(world, me));
    release(ai, world, &threatened);
    keep_home(ai, world, &threatened);
    let mut pool = crate::dispatch::soldiers(ai, world, |roles, s| roles.is_free(s));
    let needs: Vec<(Position, f32)> = {
        let view = PlayerView::new(world, me);
        let mut cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
        cities.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        cities
            .into_iter()
            .filter_map(|city| {
                let fighters: Vec<hecs::Entity> = view
                    .garrison(city)
                    .into_iter()
                    .filter(|&u| is_fighter(&view, u))
                    .collect();
                // On their way: the ones that hold the role and are not in the city yet.
                let coming: Vec<hecs::Entity> = view
                    .my_units()
                    .into_iter()
                    .filter(|&u| {
                        ai.roles.target_of(view.serial_of(u), Role::Reinforce) == Some(city)
                            && view.unit_position(u) != Some(city)
                    })
                    .collect();
                let held: f32 = fighters
                    .iter()
                    .chain(&coming)
                    .map(|&u| unit_strength(&view, u))
                    .sum();
                let peace = crate::garrison::wanted_strength(&view, city) - held;
                let danger = threatened
                    .iter()
                    .find(|&&(p, _)| p == city)
                    .map_or(0.0, |&(_, short)| short - fighting_strength(&view, &coming));
                let need = peace.max(danger);
                (need > 0.0).then_some((city, need))
            })
            .collect()
    };
    let mut reached = ReachCache::default();
    let mut sent_to: Vec<(Position, usize)> = Vec::new();
    for (city, mut need) in needs {
        // The pool by distance from the city, the nearest first.
        let order: Vec<hecs::Entity> = {
            let view = PlayerView::new(world, me);
            let mut near: Vec<(u32, f4x_core::world::serial::Serial, hecs::Entity)> = pool
                .iter()
                .copied()
                .filter(|&u| view.unit_position(u) != Some(city))
                .filter_map(|u| {
                    let d = view.distance(view.unit_position(u)?, city)?;
                    Some((d, view.serial_of(u), u))
                })
                .collect();
            near.sort_by_key(|&(d, serial, _)| (d, serial));
            near.into_iter().map(|(_, _, u)| u).collect()
        };
        // Soldiers that can walk to the city go first; the ones that cannot, on this plane, are
        // tagged for a ship (`pickup.rs`) only when walkers are not enough.
        let (walkers, sailors): (Vec<hecs::Entity>, Vec<hecs::Entity>) = {
            let view = PlayerView::new(world, me);
            order
                .into_iter()
                .partition(|&u| reached.can_reach(&view, &[u], city))
        };
        let by_sea: Vec<hecs::Entity> = {
            let view = PlayerView::new(world, me);
            sailors
                .into_iter()
                .filter(|&u| {
                    view.is_walker(u)
                        && crate::view::is_coastal(&view, city)
                        && view.unit_position(u).is_some_and(|p| p.plane == city.plane)
                })
                .collect()
        };
        for (unit, sails) in walkers
            .into_iter()
            .map(|u| (u, false))
            .chain(by_sea.into_iter().map(|u| (u, true)))
        {
            if need <= 0.0 {
                break;
            }
            let cap = crate::garrison::city_cap(&PlayerView::new(world, me), city);
            let on_the_way = sent_to
                .iter()
                .find(|(p, _)| *p == city)
                .map_or(0, |&(_, n)| n);
            if world.units().units_at(city).len() + on_the_way >= cap {
                break;
            }
            let strength = unit_strength(&PlayerView::new(world, me), unit);
            if !sails {
                let go = Command::GoTo {
                    units: vec![unit],
                    to: city,
                };
                if ai.send(world, go, events).is_err() {
                    continue;
                }
            }
            let serial = PlayerView::new(world, me).serial_of(unit);
            let _ = ai
                .roles
                .claim(serial, Role::Reinforce, world.turn, Some(city));
            ai.roles.set_target(serial, Role::Reinforce, Some(city));
            if sails {
                ai.roles
                    .set_crossing(serial, Some(crate::roles::Crossing::Sea));
            }
            need -= strength;
            pool.retain(|&u| u != unit);
            match sent_to.iter_mut().find(|(p, _)| *p == city) {
                Some((_, n)) => *n += 1,
                None => sent_to.push((city, 1)),
            }
        }
    }
}

/// A soldier that stands in a city no enemy threatens, or whose city is no longer ours, has done
/// its task.
fn release(ai: &mut Ai, world: &World, threatened: &[(Position, f32)]) {
    let view = PlayerView::new(world, ai.player);
    let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    for u in view.my_units() {
        let serial = view.serial_of(u);
        let Some(to) = ai.roles.target_of(serial, Role::Reinforce) else {
            continue;
        };
        let arrived = view.unit_position(u) == Some(to);
        let danger = threatened.iter().any(|&(p, _)| p == to);
        if !cities.contains(&to) || (arrived && !danger) {
            ai.roles.release(serial, Role::Reinforce);
        }
    }
}

/// The free soldiers standing in a threatened city stay in it.
fn keep_home(ai: &mut Ai, world: &World, threatened: &[(Position, f32)]) {
    let free = crate::dispatch::soldiers(ai, world, |roles, s| roles.is_free(s));
    let view = PlayerView::new(world, ai.player);
    for u in free {
        let Some(at) = view.unit_position(u) else {
            continue;
        };
        if threatened.iter().any(|&(p, _)| p == at) {
            let serial = view.serial_of(u);
            let _ = ai
                .roles
                .claim(serial, Role::Reinforce, world.turn, Some(at));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0, P1};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A coast city with `n` spearmen and a second, empty city inland.
    fn two_cities(n: usize) -> Result<(World, Ai), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let events = w.apply(Command::FoundCity {
            owner: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Inland".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(!events.iter().any(|e| matches!(e, Event::Rejected(_))));
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* high_men_spearmen */
            n,
        );
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w);
        Ok((w, ai))
    }

    #[test]
    fn an_empty_city_is_sent_a_soldier_from_the_pool_and_only_one() -> TestResult {
        let (mut w, mut ai) = two_cities(4)?;
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.roles.targets(Role::Reinforce), vec![inland]);
        // The one on its way is counted as held: the next turn sends no more.
        crate::roles::register(&mut ai, &w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.roles.count(Role::Reinforce), 1);
        Ok(())
    }

    #[test]
    fn a_soldier_that_has_arrived_is_released() -> TestResult {
        let (mut w, mut ai) = two_cities(4)?;
        let inland = at(INLAND_CITY.0, INLAND_CITY.1);
        let unit = spawn(&mut w, P0, inland, 104, 1)[0];
        let serial = PlayerView::new(&w, P0).serial_of(unit);
        let _ = ai.roles.claim(serial, Role::Reinforce, 1, Some(inland));
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(!ai.roles.is(serial, Role::Reinforce));
        Ok(())
    }

    /// West's city holds one spearman; five of East's swordsmen stand two squares away and three
    /// of West's own wait three squares from them.
    fn besieged() -> (World, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(10, 6), 104 /* high_men_spearmen */, 1);
        spawn(&mut w, P1, at(8, 6), 105 /* high_men_swordsmen */, 5);
        let spare = spawn(&mut w, P0, at(6, 3), 105, 3);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        (w, spare)
    }

    /// The enemy near a city is worth more than it holds: the spare soldiers go into it.
    #[test]
    fn a_stronger_enemy_near_a_city_brings_the_spare_soldiers_into_it() {
        let (mut w, spare) = besieged();
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        for &u in &spare {
            assert!(view.unit_has_order(u));
            assert_eq!(
                ai.roles.target_of(view.serial_of(u), Role::Reinforce),
                Some(at(10, 6))
            );
        }
    }

    /// The free soldiers a threatened city holds beyond its post stay in it.
    #[test]
    fn the_free_soldiers_of_a_threatened_city_stay_in_it() {
        let (mut w, _) = besieged();
        let extra = spawn(&mut w, P0, at(10, 6), 105, 2);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let held = extra
            .iter()
            .filter(|&&u| {
                ai.roles.is(view.serial_of(u), Role::Reinforce)
                    || ai.roles.is(view.serial_of(u), Role::Garrison)
            })
            .count();
        assert_eq!(held, 2);
        assert!(!extra.iter().any(|&u| ai.roles.is_free(view.serial_of(u))));
    }

    /// The empty home city and two soldiers on the far island.
    fn overseas() -> Result<(World, Vec<hecs::Entity>), Box<dyn std::error::Error>> {
        use crate::testworld::ENEMY_CITY;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let far = {
            let view = PlayerView::new(&w, P0);
            crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
                .ok_or("a shore on the far island")?
                .0
        };
        let soldiers = spawn(&mut w, P0, far, 104 /* spearmen */, 2);
        Ok((w, soldiers))
    }

    /// Soldiers on another island go to an under-defended city when none can walk there: one is
    /// tagged for a ship, as many as the city wants.
    #[test]
    fn soldiers_across_the_sea_are_tagged_for_a_ship_when_no_walker_is_there() -> TestResult {
        let (mut w, overseas) = overseas()?;
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let tagged: Vec<_> = overseas
            .iter()
            .filter(|&&u| {
                ai.roles.crossing_of(view.serial_of(u)) == Some(crate::roles::Crossing::Sea)
            })
            .collect();
        assert_eq!(tagged.len(), 1, "one soldier is what the city wants");
        assert_eq!(
            ai.roles
                .target_of(view.serial_of(*tagged[0]), Role::Reinforce),
            Some(city)
        );
        Ok(())
    }

    /// With a walker in reach, the walker goes and nobody is tagged.
    #[test]
    fn a_walker_in_reach_is_sent_before_anyone_is_tagged_for_a_ship() -> TestResult {
        let (mut w, overseas) = overseas()?;
        let walker = spawn(&mut w, P0, at(8, 6), 104, 1)[0];
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        assert_eq!(
            ai.roles.target_of(view.serial_of(walker), Role::Reinforce),
            Some(at(COAST_CITY.0, COAST_CITY.1))
        );
        assert_eq!(ai.roles.crossing_of(view.serial_of(walker)), None);
        assert!(overseas
            .iter()
            .all(|&u| ai.roles.crossing_of(view.serial_of(u)).is_none()));
        Ok(())
    }
}
