//! Settlers: choose a place for the next city, walk there and found it.

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::Ability;

use crate::city_site::{Free, SPACING};
use crate::view::{distance, nearest_unexplored};
use crate::Ai;

/// How far from a city of the wizard `room_for_city` looks for a place for another.
const SEARCH_RADIUS: i32 = 9;

/// Is there ground near one of the wizard's cities where a city could still be founded? There is
/// no cost to growing in this game, so a wizard keeps settling until the land is full.
pub(crate) fn room_for_city(view: &PlayerView) -> bool {
    let cities: Vec<Position> = view
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .chain(view.known_cities().into_iter().map(|(_, _, p)| p))
        .collect();
    view.my_cities().into_iter().any(|(_, from)| {
        (-SEARCH_RADIUS..=SEARCH_RADIUS).any(|dy| {
            (-SEARCH_RADIUS..=SEARCH_RADIUS).any(|dx| {
                view.shift(from, dx, dy).is_some_and(|p| {
                    p.y >= 1
                        && view.can_found_city(p)
                        && !view.has_lair(p)
                        && !view.in_node(p)
                        && !cities
                            .iter()
                            .any(|&t| distance(view, t, p).is_some_and(|d| d < SPACING))
                })
            })
        })
    })
}

pub fn settlers(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let mine: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut mine: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| view.unit_has_ability(u, Ability::new(12)))
            .collect();
        view.sort_by_serial(&mut mine);
        // Forget the targets of settlers that are gone.
        let alive: Vec<_> = mine.iter().map(|&u| view.serial_of(u)).collect();
        ai.roles.prune(&{
            let mut keep = alive.clone();
            keep.extend(view.my_units().iter().map(|&u| view.serial_of(u)));
            keep
        });
        for &s in &alive {
            let _ = ai
                .roles
                .claim(s, crate::roles::Role::Settle, game.turn, None);
        }
        mine
    };

    // The free settlers get a site each, the best site to the nearest settler.
    let mut avoid_now: Vec<Position> = Vec::new();
    let plan: Vec<(hecs::Entity, Position, bool)> = {
        let view = PlayerView::new(game, me);
        let list = ai.sites.refreshed(&view).to_vec();
        let taken = ai.roles.targets(crate::roles::Role::Settle);
        let free: Vec<Free> = mine
            .iter()
            .filter(|&&u| {
                ai.roles
                    .target_of(view.serial_of(u), crate::roles::Role::Settle)
                    .is_none()
            })
            .filter_map(|&u| {
                Some(Free {
                    unit: u,
                    at: view.unit_position(u)?,
                    reach: view.reachable(&[u]),
                })
            })
            .collect();
        crate::city_site::assign(&view, &list, &free, &taken, &[])
    };

    for unit in mine {
        let (pos, serial) = {
            let view = PlayerView::new(game, me);
            let Some(pos) = view.unit_position(unit) else {
                continue;
            };
            (pos, view.serial_of(unit))
        };
        // No walk leads to the site: the settler waits where it stands for the transport, the
        // dispatcher (`revalidate`) has tagged it.
        if ai.roles.crossing_of(serial).is_some() {
            continue;
        }
        if let Some(target) = ai.roles.target_of(serial, crate::roles::Role::Settle) {
            if target == pos {
                let (can_found, name) = {
                    let view = PlayerView::new(game, me);
                    (view.can_found_city(pos), city_name(&view, unit))
                };
                if can_found
                    && ai
                        .send(game, Command::SettleOutpost { unit, name }, events)
                        .is_ok()
                {
                    ai.roles
                        .set_target(serial, crate::roles::Role::Settle, None);
                    continue;
                }
                // Blocked on arrival: the site is no place for a city after all.
                ai.settler_fates
                    .note(crate::settler_fate::SettlerFate::SiteUnusable);
                ai.roles
                    .set_target(serial, crate::roles::Role::Settle, None); // blocked: choose again below
            } else if PlayerView::new(game, me).unit_has_order(unit) {
                if ai.roles.track(serial, pos, true) >= 1 {
                    // It did not move all turn: the way is blocked. Give this site up.
                    ai.settler_fates
                        .note(crate::settler_fate::SettlerFate::WayBlocked);
                    avoid_now.push(target);
                    ai.roles
                        .set_target(serial, crate::roles::Role::Settle, None);
                    ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
                } else {
                    continue; // still walking
                }
            }
        }
        ai.roles.track(serial, pos, false);
        if ai
            .roles
            .target_of(serial, crate::roles::Role::Settle)
            .is_none()
        {
            let (site, scout) = {
                let view = PlayerView::new(game, me);
                let given = plan
                    .iter()
                    .find(|&&(u, _, _)| u == unit)
                    .map(|&(_, site, ship)| (site, ship))
                    .filter(|&(site, _)| !avoid_now.contains(&site) && view.can_found_city(site));
                // The planned site, or the best one left for this settler alone (a settler whose
                // site just failed, or one that came after the plan was made).
                let chosen = given.or_else(|| {
                    let list = ai.sites.refreshed(&view).to_vec();
                    let taken = ai.roles.targets(crate::roles::Role::Settle);
                    let free = [Free {
                        unit,
                        at: pos,
                        reach: view.reachable(&[unit]),
                    }];
                    crate::city_site::assign(&view, &list, &free, &taken, &avoid_now)
                        .first()
                        .map(|&(_, site, ship)| (site, ship))
                });
                match chosen {
                    Some(chosen) => (Some(chosen), None),
                    // No site on the explored map: walk to unexplored land to see more of it.
                    None => (None, nearest_unexplored(&view, unit)),
                }
            };
            match (site, scout) {
                (Some((site, ship)), _) => {
                    ai.roles
                        .set_target(serial, crate::roles::Role::Settle, Some(site));
                    // A site over the sea: the settler waits for a ship like any unit tagged so (the
                    // ship is built by the general rule, `economy.rs`).
                    if ship {
                        ai.roles
                            .set_crossing(serial, Some(crate::roles::Crossing::Sea));
                        continue;
                    }
                }
                (None, Some(to)) => {
                    ai.order(
                        game,
                        Command::GoTo {
                            units: vec![unit],
                            to,
                        },
                        events,
                    );
                    continue;
                }
                (None, None) => {
                    ai.settler_fates
                        .note(crate::settler_fate::SettlerFate::NoSite);
                    continue;
                }
            }
        }
        let Some(target) = ai.roles.target_of(serial, crate::roles::Role::Settle) else {
            continue;
        };
        if target == pos {
            // Found it right here (next turn if the settlers just arrived and cannot).
            let name = city_name(&PlayerView::new(game, me), unit);
            if ai
                .send(game, Command::SettleOutpost { unit, name }, events)
                .is_ok()
            {
                ai.roles
                    .set_target(serial, crate::roles::Role::Settle, None);
            }
        } else if !crate::escort::walk(ai, game, unit, target, events) {
            ai.settler_fates
                .note(crate::settler_fate::SettlerFate::WayBlocked);
            ai.roles
                .set_target(serial, crate::roles::Role::Settle, None);
        }
    }
}

/// A name for the next city: the race's list, by how many cities the wizard has.
fn city_name(view: &PlayerView, unit: hecs::Entity) -> String {
    let used = view.my_cities().len();
    view.unit_def(unit)
        .and_then(|d| d.race.as_ref())
        .copied()
        .map(|r| view.city_name_for(r, used))
        .unwrap_or_else(|| format!("Outpost {}", used + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};
    use f4x_def::half_int::HalfInt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_wizard_on_a_roomy_island_has_room_for_another_city() {
        let (g, _) = island_with_settlers(0);
        assert!(room_for_city(&PlayerView::new(&g, P0)));
    }

    /// The west island, explored by its wizard, with `count` settlers standing in the capital.
    fn island_with_settlers(count: usize) -> (Game, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, at(7, 6), HalfInt::from_int(4));
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            count,
        );
        (g, units)
    }

    fn cities_of(g: &Game, player: crate::PlayerId) -> Vec<Position> {
        PlayerView::new(g, player)
            .my_cities()
            .into_iter()
            .map(|(_, pos)| pos)
            .collect()
    }

    fn end_turn(g: &mut Game) {
        // The settlers eat: put the citizens on the farms, or they starve at the turn.
        g.balance_farmers(P0);
        g.apply(Command::EndTurn);
    }

    #[test]
    fn a_settler_walks_to_a_site_and_founds_a_city_there() -> TestResult {
        let (mut g, units) = island_with_settlers(1);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        for _ in 0..40 {
            settlers(&mut ai, &mut g, &mut events);
            if cities_of(&g, P0).len() == 2 {
                break;
            }
            end_turn(&mut g);
        }
        let cities = cities_of(&g, P0);
        assert_eq!(cities.len(), 2, "the settler founded a city: {events:?}");
        let founded = *cities
            .iter()
            .find(|&&c| c != capital)
            .ok_or("a city besides the capital")?;
        assert_eq!(founded.plane, capital.plane);
        assert!(
            PlayerView::new(&g, P0)
                .distance(capital, founded)
                .is_some_and(|d| d >= 2),
            "{founded:?}"
        );
        assert!(
            g.units().unit_owner(units[0]).is_none(),
            "the settlers are used up by founding"
        );
        assert!(
            ai.roles.targets(crate::roles::Role::Settle).is_empty(),
            "the target is forgotten"
        );
        Ok(())
    }

    #[test]
    fn two_settlers_choose_sites_that_keep_apart() -> TestResult {
        let (mut g, _) = island_with_settlers(2);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let sites: Vec<Position> = ai.roles.targets(crate::roles::Role::Settle);
        assert_eq!(sites.len(), 2, "each settler has a site");
        let view = PlayerView::new(&g, P0);
        let apart = distance(&view, sites[0], sites[1]).ok_or("the sites are on one plane")?;
        assert!(apart >= SPACING, "{sites:?} are {apart} apart");
        Ok(())
    }

    #[test]
    fn a_settler_that_did_not_move_all_turn_leaves_its_site_for_now_and_chooses_another(
    ) -> TestResult {
        let (mut g, units) = island_with_settlers(1);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let serial = PlayerView::new(&g, P0).serial_of(units[0]);
        let first = ai
            .roles
            .target_of(serial, crate::roles::Role::Settle)
            .ok_or("the settler chose a site")?;
        // A whole turn went by without the settler leaving its square: the last look equals now.
        let here = PlayerView::new(&g, P0)
            .unit_position(units[0])
            .ok_or("the settler is on the map")?;
        ai.roles.track(serial, here, false);
        settlers(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.bad_sites.is_empty(), "no blacklist: {:?}", ai.bad_sites);
        assert!(
            ai.settler_failures()
                .contains(&("way to the site blocked", 1)),
            "{:?}",
            ai.settler_failures()
        );
        let second = ai
            .roles
            .target_of(serial, crate::roles::Role::Settle)
            .ok_or("the settler chose another site")?;
        assert_ne!(second, first);
        Ok(())
    }

    #[test]
    fn a_settler_that_finds_no_site_scouts_instead() -> TestResult {
        // Nothing but the squares round the capital is explored, and they are too near it.
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        );
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        assert!(ai.roles.targets(crate::roles::Role::Settle).is_empty());
        assert!(view.unit_has_order(units[0]), "it walks to unexplored land");
        Ok(())
    }

    #[test]
    fn the_new_city_is_named_after_its_race_when_the_game_has_no_name_list() -> TestResult {
        let (g, units) = island_with_settlers(1);
        let view = PlayerView::new(&g, P0);
        let race = view
            .content()
            .rules
            .races
            .by_num(8)
            .ok_or("high_men is shipped")?;
        let used = view.my_cities().len();
        assert_eq!(
            city_name(&view, units[0]),
            format!("{}_city_{used:02}", race.id)
        );
        Ok(())
    }

    #[test]
    fn a_settler_on_a_full_island_is_sent_over_the_sea_and_founds_a_city_across() -> TestResult {
        use crate::testworld::{build, fill_west, ENEMY_CITY};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        build(
            &mut g,
            port,
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        g.explore(P0, port, HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut g);
        spawn(&mut g, P0, port, 36 /* galley */, 1);
        spawn(&mut g, P0, port, 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        let before = PlayerView::new(&g, P0).my_cities().len();
        for _ in 0..40 {
            let mut events = Vec::new();
            crate::roles::register(&mut ai, &g);
            crate::dispatch::revalidate(&mut ai, &mut g, &mut events);
            settlers(&mut ai, &mut g, &mut events);
            crate::pickup::run(&mut ai, &mut g, &mut events);
            g.apply(Command::EndTurn);
        }
        let cities = PlayerView::new(&g, P0).my_cities();
        assert!(cities.len() > before, "{cities:?}");
        assert!(
            cities.iter().any(|&(_, p)| p.x >= 28),
            "a city on the east island: {cities:?}"
        );
        Ok(())
    }

    #[test]
    fn a_settler_given_a_site_over_the_sea_is_tagged_for_a_ship_and_keeps_the_site() -> TestResult {
        use crate::roles::{Crossing, Role};
        use crate::testworld::{fill_west, ENEMY_CITY};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        g.explore(P0, port, HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut g);
        let settler = spawn(&mut g, P0, port, 111 /* high_men_settlers */, 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(settler);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let site = ai
            .roles
            .target_of(serial, Role::Settle)
            .ok_or("the settler has a site")?;
        assert!(site.x >= 28, "over the sea: {site:?}");
        assert_eq!(ai.roles.crossing_of(serial), Some(Crossing::Sea));
        assert!(ai.bad_sites.is_empty());
        Ok(())
    }
}
