//! Settlers: choose a place for the next city, walk there and found it.

use crate::player::view::PlayerView;
use crate::world::{Command, Event};

use super::Handler;
use super::Host;

/// A settler has the settle role only while a site is assigned to it. Where the site stops being
/// one (a city stands on it, it cannot be founded, no way leads to it: `dispatch.rs`
/// `mission_valid`), the role and the site go together and the settler awaits orders again. The
/// site check (`city_site.rs`, the book is rebuilt every few turns) gives a site to each settler
/// that has none, and at once to one that has just come into being.
pub fn settlers(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    use super::roles::Role;
    let me = handler.player;
    let mine = {
        let view = PlayerView::new(game, me);
        let mut mine: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| view.unit_is_settler(u))
            .collect();
        view.sort_by_serial(&mut mine);
        mine
    };

    for unit in mine {
        let (pos, serial) = {
            let view = PlayerView::new(game, me);
            let Some(pos) = view.unit_position(unit) else {
                continue;
            };
            (pos, view.serial_of(unit))
        };
        let Some(target) = handler.roles.target_of(serial, Role::Settle) else {
            // Awaiting orders: no site, no role.
            if !handler.roles.has_role(serial) {
                handler.judge.settler_lost(super::SettlerFate::NoSite);
            }
            continue;
        };
        // Waiting for the transport that `pickup.rs` sends.
        if handler.roles.crossing_of(serial).is_some() {
            continue;
        }
        if target == pos {
            let (can_found, name) = {
                let view = PlayerView::new(game, me);
                (view.can_found_city(pos), city_name(&view, unit))
            };
            if can_found
                && handler
                    .send(game, Command::SettleOutpost { unit, name }, events)
                    .is_ok()
            {
                handler.roles.release(serial, Role::Settle);
                continue;
            }
            // Blocked on arrival: the site is no place for a city after all.
            handler.judge.settler_lost(super::SettlerFate::SiteUnusable);
            give_up(handler, game, unit, serial, events);
            continue;
        }
        let walking = PlayerView::new(game, me).unit_has_order(unit);
        if walking && handler.roles.track(serial, pos, true) < 1 {
            continue; // still walking
        }
        if walking || !super::escort::walk(handler, game, unit, target, events) {
            // It did not move all turn, or no walk can be ordered: the way is blocked.
            handler.judge.settler_lost(super::SettlerFate::WayBlocked);
            give_up(handler, game, unit, serial, events);
        }
    }
}

/// The settler's site is no longer one it can reach: the role goes with the site, and the settler
/// awaits orders until the next site check.
fn give_up(
    handler: &mut Handler,
    game: &mut dyn Host,
    unit: hecs::Entity,
    serial: crate::world::serial::Serial,
    events: &mut Vec<Event>,
) {
    handler.roles.release(serial, super::roles::Role::Settle);
    handler.roles.clear_track(serial);
    handler.order(game, Command::ClearOrders { units: vec![unit] }, events);
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
    use super::super::testworld::Game;
    use super::super::testworld::{at, game, spawn, COAST_CITY, P0};
    use super::*;
    use crate::map::Position;
    use crate::world::World;
    use f4x_def::half_int::HalfInt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

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

    fn cities_of(w: &World, player: crate::player::PlayerId) -> Vec<Position> {
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
        let mut handler = super::super::testworld::handler(P0);
        let site = super::super::testworld::site_at_home(&w);
        super::super::testworld::give_site(&mut handler, &w, units[0], site);
        let mut events = Vec::new();
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        for _ in 0..40 {
            settlers(&mut handler, &mut w, &mut events);
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
            handler
                .roles
                .targets(super::super::roles::Role::Settle)
                .is_empty(),
            "the target is forgotten"
        );
        Ok(())
    }

    #[test]
    fn a_settler_that_did_not_move_all_turn_loses_the_site_and_the_role() -> TestResult {
        use super::super::roles::Role;
        let (mut w, units) = island_with_settlers(1);
        let plain = std::rc::Rc::new(super::super::testworld::Plain::default());
        let mut handler = super::super::Handler::judged(P0, plain.clone());
        let site = super::super::testworld::site_at_home(&w);
        super::super::testworld::give_site(&mut handler, &w, units[0], site);
        settlers(&mut handler, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(units[0]);
        handler
            .roles
            .target_of(serial, Role::Settle)
            .ok_or("the settler chose a site")?;
        // A whole turn went by without the settler leaving its square: the last look equals now.
        let here = PlayerView::new(&w, P0)
            .unit_position(units[0])
            .ok_or("the settler is on the map")?;
        handler.roles.track(serial, here, false);
        settlers(&mut handler, &mut w, &mut Vec::new());
        assert!(
            plain
                .fates
                .borrow()
                .contains(&super::super::SettlerFate::WayBlocked),
            "{:?}",
            plain.fates.borrow()
        );
        assert_eq!(handler.roles.role_of(serial), Some(Role::AwaitingOrders));
        assert!(handler.roles.targets(Role::Settle).is_empty());
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
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        settlers(&mut handler, &mut w, &mut Vec::new());
        assert!(!handler.roles.has_role(serial), "no site, no settle role");
        assert!(handler
            .roles
            .targets(super::super::roles::Role::Settle)
            .is_empty());
        assert_eq!(
            handler.roles.role_of(serial),
            Some(super::super::roles::Role::AwaitingOrders)
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
        use super::super::testworld::{build, fill_west, ENEMY_CITY};
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
        let settler = spawn(&mut w, P0, port, 111 /* high_men_settlers */, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        let site = super::super::testworld::site_over_the_sea(&w);
        super::super::testworld::give_site(&mut handler, &w, settler, site);
        let before = PlayerView::new(&w, P0).my_cities().len();
        for _ in 0..40 {
            let mut events = Vec::new();
            super::super::roles::register(&mut handler, &w, w.round());
            super::super::dispatch::revalidate(&mut handler, &mut w, &mut events);
            settlers(&mut handler, &mut w, &mut events);
            super::super::pickup::run(&mut handler, &mut w, &mut events);
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
}
