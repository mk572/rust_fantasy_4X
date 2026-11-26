//! Settlers: choose a place for the next city, walk there and found it.

use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event};

use crate::city_site::Free;
use crate::Ai;
use f4x_master::Game;

/// A settler has the settle role only while a site is assigned to it. Where the site stops being
/// one (a city stands on it, it cannot be founded, no way leads to it: `dispatch.rs`
/// `mission_valid`), the role and the site go together and the settler awaits orders again. The
/// site check (`city_site.rs`, the book is rebuilt every few turns) gives a site to each settler
/// that has none, and at once to one that has just come into being.
pub fn settlers(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    use crate::roles::Role;
    let me = ai.player;
    let (mine, plan) = {
        let view = PlayerView::new(game, me);
        let mut mine: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| view.unit_is_settler(u))
            .collect();
        view.sort_by_serial(&mut mine);
        // The site check is due when the book is rebuilt; a new settler does not wait for it.
        let due = {
            ai.sites.refreshed(&view, game.round());
            ai.sites.rebuilt_on(game.round())
        };
        let free: Vec<Free> = mine
            .iter()
            .filter(|&&u| {
                let serial = view.serial_of(u);
                !ai.roles.has_role(serial)
                    && (due
                        || ai
                            .roles
                            .get(serial)
                            .is_some_and(|a| a.since == game.round()))
            })
            .filter_map(|&u| {
                Some(Free {
                    unit: u,
                    at: view.unit_position(u)?,
                    reach: view.reachable(&[u]),
                })
            })
            .collect();
        let list = ai.sites.refreshed(&view, game.round()).to_vec();
        let taken = ai.roles.targets(Role::Settle);
        (
            mine,
            crate::city_site::assign(&view, &list, &free, &taken, &[]),
        )
    };
    // The sites go to the free settlers; the role comes with the site.
    for &(unit, site, ship) in &plan {
        let serial = PlayerView::new(game, me).serial_of(unit);
        if ai
            .roles
            .claim(serial, Role::Settle, game.round(), Some(site))
            .is_err()
        {
            continue;
        }
        // A site over the sea: the settler waits for a ship like any unit tagged so (the ship is
        // built by the general rule, `economy.rs`).
        if ship {
            ai.roles
                .set_crossing(serial, Some(crate::roles::Crossing::Sea));
        }
    }

    for unit in mine {
        let (pos, serial) = {
            let view = PlayerView::new(game, me);
            let Some(pos) = view.unit_position(unit) else {
                continue;
            };
            (pos, view.serial_of(unit))
        };
        let Some(target) = ai.roles.target_of(serial, Role::Settle) else {
            // Awaiting orders: no site, no role.
            if !ai.roles.has_role(serial) {
                ai.settler_fates
                    .note(crate::settler_fate::SettlerFate::NoSite);
            }
            continue;
        };
        // Waiting for the transport that `pickup.rs` sends.
        if ai.roles.crossing_of(serial).is_some() {
            continue;
        }
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
                ai.roles.release(serial, Role::Settle);
                continue;
            }
            // Blocked on arrival: the site is no place for a city after all.
            ai.settler_fates
                .note(crate::settler_fate::SettlerFate::SiteUnusable);
            give_up(ai, game, unit, serial, events);
            continue;
        }
        let walking = PlayerView::new(game, me).unit_has_order(unit);
        if walking && ai.roles.track(serial, pos, true) < 1 {
            continue; // still walking
        }
        if walking || !crate::escort::walk(ai, game, unit, target, events) {
            // It did not move all turn, or no walk can be ordered: the way is blocked.
            ai.settler_fates
                .note(crate::settler_fate::SettlerFate::WayBlocked);
            give_up(ai, game, unit, serial, events);
        }
    }
}

/// The settler's site is no longer one it can reach: the role goes with the site, and the settler
/// awaits orders until the next site check.
fn give_up(
    ai: &mut Ai,
    game: &mut Game,
    unit: hecs::Entity,
    serial: crate::Serial,
    events: &mut Vec<Event>,
) {
    ai.roles.release(serial, crate::roles::Role::Settle);
    ai.roles.clear_track(serial);
    ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
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
    use crate::view::distance;
    use f4x_core::map::Position;
    use f4x_core::world::World;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_wizard_on_a_roomy_island_has_room_for_another_city() {
        let (w, _) = island_with_settlers(0);
        assert!(crate::city_site::has_sites(&PlayerView::new(&w, P0)));
    }

    /// The west island, explored by its wizard, with `count` settlers standing in the capital.
    fn island_with_settlers(count: usize) -> (Game, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.explore(P0, at(7, 6), HalfInt::from_int(4));
        let units = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            count,
        );
        (w, units)
    }

    fn cities_of(w: &World, player: crate::PlayerId) -> Vec<Position> {
        PlayerView::new(w, player)
            .my_cities()
            .into_iter()
            .map(|(_, pos)| pos)
            .collect()
    }

    fn end_turn(w: &mut Game) {
        // The settlers eat: put the citizens on the farms, or they starve at the turn.
        w.balance_farmers(P0);
        w.play_round();
    }

    #[test]
    fn a_settler_walks_to_a_site_and_founds_a_city_there() -> TestResult {
        let (mut w, units) = island_with_settlers(1);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        for _ in 0..40 {
            settlers(&mut ai, &mut w, &mut events);
            if cities_of(&w, P0).len() == 2 {
                break;
            }
            end_turn(&mut w);
        }
        let cities = cities_of(&w, P0);
        assert_eq!(cities.len(), 2, "the settler founded a city: {events:?}");
        let founded = *cities
            .iter()
            .find(|&&c| c != capital)
            .ok_or("a city besides the capital")?;
        assert_eq!(founded.plane, capital.plane);
        assert!(
            PlayerView::new(&w, P0)
                .distance(capital, founded)
                .is_some_and(|d| d >= 2),
            "{founded:?}"
        );
        assert!(
            w.units().unit_owner(units[0]).is_none(),
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
        let (mut w, _) = island_with_settlers(2);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut w, &mut Vec::new());
        let sites: Vec<Position> = ai.roles.targets(crate::roles::Role::Settle);
        assert_eq!(sites.len(), 2, "each settler has a site");
        let view = PlayerView::new(&w, P0);
        let apart = distance(&view, sites[0], sites[1]).ok_or("the sites are on one plane")?;
        assert!(
            apart >= view.min_city_distance(),
            "{sites:?} are {apart} apart"
        );
        Ok(())
    }

    #[test]
    fn a_settler_that_did_not_move_all_turn_loses_the_site_and_the_role() -> TestResult {
        use crate::roles::Role;
        let (mut w, units) = island_with_settlers(1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        settlers(&mut ai, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(units[0]);
        ai.roles
            .target_of(serial, Role::Settle)
            .ok_or("the settler chose a site")?;
        // A whole turn went by without the settler leaving its square: the last look equals now.
        let here = PlayerView::new(&w, P0)
            .unit_position(units[0])
            .ok_or("the settler is on the map")?;
        ai.roles.track(serial, here, false);
        settlers(&mut ai, &mut w, &mut Vec::new());
        assert!(
            ai.settler_failures()
                .contains(&("way to the site blocked", 1)),
            "{:?}",
            ai.settler_failures()
        );
        assert_eq!(ai.roles.role_of(serial), Some(Role::AwaitingOrders));
        assert!(ai.roles.targets(Role::Settle).is_empty());
        Ok(())
    }
    #[test]
    fn a_settler_that_finds_no_site_awaits_orders_with_no_role() -> TestResult {
        // Nothing but the squares round the capital is explored, and they are too near it.
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            111, /* high_men_settlers */
            1,
        );
        let serial = PlayerView::new(&w, P0).serial_of(units[0]);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &w, w.turn);
        settlers(&mut ai, &mut w, &mut Vec::new());
        assert!(!ai.roles.has_role(serial), "no site, no settle role");
        assert!(ai.roles.targets(crate::roles::Role::Settle).is_empty());
        assert_eq!(
            ai.roles.role_of(serial),
            Some(crate::roles::Role::AwaitingOrders)
        );
        Ok(())
    }

    #[test]
    fn the_new_city_is_named_after_its_race_when_the_game_has_no_name_list() -> TestResult {
        let (w, units) = island_with_settlers(1);
        let view = PlayerView::new(&w, P0);
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
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        build(
            &mut w,
            port,
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        w.explore(P0, port, HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            w.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        w.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut w);
        spawn(&mut w, P0, port, 36 /* galley */, 1);
        spawn(&mut w, P0, port, 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        let before = PlayerView::new(&w, P0).my_cities().len();
        for _ in 0..40 {
            let mut events = Vec::new();
            crate::roles::register(&mut ai, &w, w.turn);
            crate::dispatch::revalidate(&mut ai, &mut w, &mut events);
            settlers(&mut ai, &mut w, &mut events);
            crate::pickup::run(&mut ai, &mut w, &mut events);
            w.play_round();
        }
        let cities = PlayerView::new(&w, P0).my_cities();
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
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        w.explore(P0, port, HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            w.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        w.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut w);
        let settler = spawn(&mut w, P0, port, 111 /* high_men_settlers */, 1)[0];
        let serial = PlayerView::new(&w, P0).serial_of(settler);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut w, &mut Vec::new());
        let site = ai
            .roles
            .target_of(serial, Role::Settle)
            .ok_or("the settler has a site")?;
        assert!(site.x >= 28, "over the sea: {site:?}");
        assert_eq!(ai.roles.crossing_of(serial), Some(Crossing::Sea));
        Ok(())
    }
}
