//! Engineers build roads (`Role::Road`): first the links between the wizard's own cities, then the
//! way toward the city its army is gathering against. The roads halve the walking of every army
//! and settler after them.
//!
//! Nothing is planned and kept but which cities have been looked at. An engineer standing in an
//! own city takes the nearest few cities, asks the game for the way it would walk there, and builds
//! a road along it when any square of it has none. When the city has no such link it is marked
//! looked at and the engineer walks to the nearest city not yet looked at. When every city has been
//! looked at the engineer works toward the campaign target, else the nearest tower, a stretch at a
//! time, from wherever it stands; within `ENEMY_NEAR` squares of a rival's city it works only with
//! an escort (`escort::guard`) and waits for one. With nothing left it gives the role back. The marks are cleared when the number of
//! cities changes. One engineer unit is built (`build_engineer`) while there is work and none exists.
//!
//! See `docs/handler-unit-registry.md`.

use super::roles::Role;
use super::Handler;
use super::Host;
use crate::map::Position;
use crate::player::view::PlayerView;
use crate::rules::Stat;
use crate::world::{Command, Event, World};

/// The nearest cities an engineer links a city to.
const NEIGHBOURS: usize = 3;
/// Squares apart the cities of a link are at most.
const LINK_RANGE: u32 = 12;

/// Squares of a road toward the campaign target built at a time.
const STRETCH: usize = 8;

/// The engineers: units that build roads.
fn is_engineer(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_stat(unit, Stat::Construction) > 0
}

/// One turn of the engineers: each that is idle gets a road to build, or a place to walk to.
pub fn run(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    let me = handler.player;
    let turn = game.round();
    let (count, engineers) = {
        let view = PlayerView::new(game, me);
        let mut engineers: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_engineer(&view, u))
            .collect();
        view.sort_by_serial(&mut engineers);
        (view.my_cities().len(), engineers)
    };
    if count != handler.road_cities {
        handler.road_cities = count;
        handler.road_checked.clear();
    }
    for engineer in engineers {
        let (serial, busy) = {
            let view = PlayerView::new(game, me);
            (
                view.serial_of(engineer),
                view.unit_builds_road(engineer) || view.unit_has_order(engineer),
            )
        };
        if busy {
            let _ = handler.roles.claim(serial, Role::Road, turn, None);
            continue;
        }
        // An engineer aboard a ship at sea has no road to build.
        let afloat = {
            let view = PlayerView::new(game, me);
            view.unit_position(engineer)
                .is_some_and(|p| !view.road_possible(p))
        };
        if afloat || handler.roles.claim(serial, Role::Road, turn, None).is_err() {
            continue;
        }
        match work(handler, game, engineer, events) {
            Some(to) => {
                let _ = handler.roles.claim(serial, Role::Road, turn, Some(to));
            }
            None => {
                handler.roles.release(serial, Role::Road);
            }
        }
    }
}

/// Give one idle engineer its work: the square it works toward, when it found some.
fn work(
    handler: &mut Handler,
    game: &mut dyn Host,
    engineer: hecs::Entity,
    events: &mut Vec<Event>,
) -> Option<Position> {
    let me = handler.player;
    let (here, cities) = {
        let view = PlayerView::new(game, me);
        let mut cities: Vec<(hecs::Entity, Position)> = view.my_cities();
        cities.sort_by_key(|&(c, _)| view.serial_of(c));
        (view.unit_position(engineer)?, cities)
    };
    // In a city: build the first link that lacks a road.
    if let Some(&(city, _)) = cities.iter().find(|&&(_, p)| p == here) {
        let mut near: Vec<(u32, Position)> = {
            let view = PlayerView::new(game, me);
            cities
                .iter()
                .filter(|&&(_, p)| p != here)
                .filter_map(|&(_, p)| view.distance(here, p).map(|d| (d, p)))
                .filter(|&(d, _)| d <= LINK_RANGE)
                .collect()
        };
        near.sort_by_key(|&(d, p)| (d, p.plane as u8, p.y, p.x));
        // A link another engineer is on its way to or building is left to it.
        let others: Vec<Position> = {
            let serial = PlayerView::new(game, me).serial_of(engineer);
            handler
                .roles
                .targeted(Role::Road)
                .into_iter()
                .filter(|&(s, _)| s != serial)
                .map(|(_, p)| p)
                .collect()
        };
        for (_, to) in near.into_iter().take(NEIGHBOURS) {
            if others.contains(&to) {
                continue;
            }
            let path = {
                let view = PlayerView::new(game, me);
                view.path(&[engineer], to)
            };
            let Some(path) = path else { continue };
            if !path
                .iter()
                .all(|&p| PlayerView::new(game, me).road_possible(p))
            {
                continue;
            }
            if path.iter().all(|&p| PlayerView::new(game, me).has_road(p)) {
                continue;
            }
            if build(handler, game, engineer, path, events) {
                return Some(to);
            }
        }
        let serial = PlayerView::new(game, me).serial_of(city);
        if !handler.road_checked.contains(&serial) {
            handler.road_checked.push(serial);
        }
    }
    // Every city looked at: toward the campaign target.
    if cities.iter().all(|&(c, _)| {
        handler
            .road_checked
            .contains(&PlayerView::new(game, me).serial_of(c))
    }) {
        return frontier(handler, game, engineer, events);
    }
    // Else to the nearest city not looked at, that has room for the engineer (a full city
    // refuses it, `StackFull`; the cities with no room wait). With none left, to the frontier.
    let Some(next) = nearest_unchecked(handler, game, engineer, here, &cities) else {
        return frontier(handler, game, engineer, events);
    };
    handler
        .send(
            game,
            Command::GoTo {
                units: vec![engineer],
                to: next,
            },
            events,
        )
        .ok()
        .map(|()| next)
}

/// The nearest of the `cities` the wizard has not looked at for roads, that the engineer can
/// reach and that has room for it.
fn nearest_unchecked(
    handler: &Handler,
    world: &World,
    engineer: hecs::Entity,
    here: Position,
    cities: &[(hecs::Entity, Position)],
) -> Option<Position> {
    let view = PlayerView::new(world, handler.player);
    cities
        .iter()
        .filter(|&&(c, p)| p != here && !handler.road_checked.contains(&view.serial_of(c)))
        .filter(|&&(_, p)| Handler::room_at(world, p, 1))
        .filter_map(|&(_, p)| view.distance(here, p).map(|d| (d, p)))
        .filter(|&(_, p)| view.can_reach(&[engineer], p))
        .min_by_key(|&(d, p)| (d, p.plane as u8, p.y, p.x))
        .map(|(_, p)| p)
}

/// Build a road along `path`: whether the game took the order.
fn build(
    handler: &Handler,
    game: &mut dyn Host,
    engineer: hecs::Entity,
    path: Vec<Position>,
    events: &mut Vec<Event>,
) -> bool {
    handler
        .send(
            game,
            Command::BuildRoad {
                units: vec![engineer],
                path,
            },
            events,
        )
        .is_ok()
}

/// Squares from a rival's city within which an engineer works with an escort.
const ENEMY_NEAR: u32 = 8;

/// The way a road is built toward: the campaign target, else the nearest tower, whichever has a
/// way to it that lacks a road. The squares of that way, the last (the city or tower itself) left
/// out, and the first without a road.
fn frontier_way(
    handler: &Handler,
    world: &World,
    engineer: hecs::Entity,
) -> Option<(Vec<Position>, usize)> {
    let view = PlayerView::new(world, handler.player);
    let here = view.unit_position(engineer)?;
    let mut towers: Vec<Position> = view
        .towers()
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| p.plane == here.plane)
        .collect();
    towers.sort_by_key(|&p| (view.distance(here, p).unwrap_or(u32::MAX), p.y, p.x));
    handler
        .campaign
        .into_iter()
        .chain(towers)
        .find_map(|target| {
            let mut path = view.path(&[engineer], target)?;
            path.pop();
            if !path.iter().all(|&p| view.road_possible(p)) {
                return None;
            }
            let first = path.iter().position(|&p| !view.has_road(p))?;
            Some((path, first))
        })
}

/// A stretch of road toward the campaign target or a tower: built from where the engineer
/// stands when the first square has none, else the engineer walks to the last square that has
/// one. Near a rival's cities it works only with an escort.
fn frontier(
    handler: &mut Handler,
    game: &mut dyn Host,
    engineer: hecs::Entity,
    events: &mut Vec<Event>,
) -> Option<Position> {
    let me = handler.player;
    let (path, first) = frontier_way(handler, game, engineer)?;
    // The stretch another engineer is on is left to it.
    {
        let serial = PlayerView::new(game, me).serial_of(engineer);
        if handler
            .roles
            .targeted(Role::Road)
            .into_iter()
            .any(|(s, p)| s != serial && path.contains(&p))
        {
            return None;
        }
    }
    if first == 0 {
        let stretch: Vec<Position> = path.into_iter().take(STRETCH).collect();
        let end = *stretch.last()?;
        let near_enemy = {
            let view = PlayerView::new(game, me);
            view.known_cities().into_iter().any(|(_, owner, city)| {
                owner != me
                    && view.is_rival_wizard(owner)
                    && stretch
                        .iter()
                        .any(|&p| view.distance(p, city).is_some_and(|d| d <= ENEMY_NEAR))
            })
        };
        if near_enemy && !super::escort::guard(handler, game, engineer, events) {
            // It waits where it stands for the escort to come.
            return PlayerView::new(game, me).unit_position(engineer);
        }
        return build(handler, game, engineer, stretch, events).then_some(end);
    }
    let to = path[first - 1];
    handler
        .send(
            game,
            Command::GoTo {
                units: vec![engineer],
                to,
            },
            events,
        )
        .ok()
        .map(|()| to)
}

#[cfg(test)]
mod tests {
    use super::super::testworld::Game;
    use super::super::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Two cities of player 0 and engineers (high men) in the first.
    fn world() -> Result<(Game, hecs::Entity), Box<dyn std::error::Error>> {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let race = w.content().rules.races.id_numbered(8).ok_or("high men")?;
        let events = w.command(Command::FoundCity {
            owner: P0,
            pos: at(INLAND_CITY.0, INLAND_CITY.1),
            name: "Second".into(),
            race: Some(race),
            population: 4,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        let engineer = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 110, 1)[0];
        w.balance_farmers(P0);
        Ok((w, engineer))
    }

    /// An engineer in a city with another own city near, and no road between them, builds one.
    #[test]
    fn an_engineer_builds_the_road_to_the_next_city() -> TestResult {
        let (mut w, engineer) = world()?;
        let mut handler = super::super::testworld::handler(P0);
        run(&mut handler, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let serial = view.serial_of(engineer);
        assert!(view.unit_builds_road(engineer));
        assert_eq!(handler.roles.role_of(serial), Some(Role::Road));
        assert_eq!(
            handler.roles.target_of(serial, Role::Road),
            Some(at(INLAND_CITY.0, INLAND_CITY.1))
        );
        Ok(())
    }

    /// A city with no room for one more unit is not the engineer's next stop (`StackFull`).
    #[test]
    fn an_engineer_does_not_walk_to_a_full_city() -> TestResult {
        let (mut w, engineer) = world()?;
        let handler = super::super::testworld::handler(P0);
        let (first, second) = (
            at(COAST_CITY.0, COAST_CITY.1),
            at(INLAND_CITY.0, INLAND_CITY.1),
        );
        let cities: Vec<_> = PlayerView::new(&w, P0).my_cities();
        assert_eq!(
            nearest_unchecked(&handler, &w, engineer, first, &cities),
            Some(second)
        );
        let room = w
            .max_tile()
            .saturating_sub(w.units().units_at(second).len());
        spawn(&mut w, P0, second, 110, room);
        assert_eq!(
            nearest_unchecked(&handler, &w, engineer, first, &cities),
            None
        );
        Ok(())
    }

    /// Engineers are not soldiers: no escort, march or garrison count takes them.
    #[test]
    fn an_engineer_is_no_fighter() -> TestResult {
        let (w, engineer) = world()?;
        let view = PlayerView::new(&w, P0);
        assert!(!super::super::view::is_fighter(&view, engineer));
        Ok(())
    }

    /// With every link done and no campaign the engineer gives the role back, and the cities
    /// are marked as looked at.
    #[test]
    fn an_engineer_with_nothing_to_build_gives_the_role_back() -> TestResult {
        let (mut w, engineer) = world()?;
        let mut handler = super::super::testworld::handler(P0);
        run(&mut handler, &mut w, &mut Vec::new());
        // Let the work finish.
        for _ in 0..30 {
            if !PlayerView::new(&w, P0).unit_builds_road(engineer) {
                break;
            }
            w.play_round();
        }
        // It looks at the other city, finds the link done and has nothing left.
        let serial = PlayerView::new(&w, P0).serial_of(engineer);
        for _ in 0..10 {
            run(&mut handler, &mut w, &mut Vec::new());
            if !handler.roles.has_role(serial) {
                break;
            }
            w.play_round();
        }
        let view = PlayerView::new(&w, P0);
        assert!(!handler.roles.has_role(serial));
        assert!(!view.unit_builds_road(engineer));
        // The road between the cities is there.
        let roads = (4..=12)
            .flat_map(|x| (4..=8).map(move |y| (x, y)))
            .filter(|&(x, y)| view.has_road(at(x, y)))
            .count();
        assert!(roads >= 5, "{roads} squares of road");
        Ok(())
    }

    /// Two engineers in a city take two different links.
    #[test]
    fn two_engineers_take_different_links() -> TestResult {
        let (mut w, first) = world()?;
        let race = w.content().rules.races.id_numbered(8).ok_or("high men")?;
        for (x, y) in [(6, 10), (10, 10)] {
            let events = w.command(Command::FoundCity {
                owner: P0,
                pos: at(x, y),
                name: format!("C{x}{y}"),
                race: Some(race),
                population: 4,
            });
            assert!(
                !events.iter().any(|e| matches!(e, Event::Rejected(_))),
                "{events:?}"
            );
        }
        w.balance_farmers(P0);
        let second = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 110, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        run(&mut handler, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let (a, b) = (view.serial_of(first), view.serial_of(second));
        let (ta, tb) = (
            handler.roles.target_of(a, Role::Road),
            handler.roles.target_of(b, Role::Road),
        );
        assert!(ta.is_some() && tb.is_some() && ta != tb, "{ta:?} {tb:?}");
        Ok(())
    }

    /// With every city looked at and no campaign, the engineer builds toward the nearest tower.
    #[test]
    fn an_engineer_builds_toward_a_tower_when_the_cities_are_linked() -> TestResult {
        let (mut w, engineer) = world()?;
        let tower = at(8, 3);
        super::super::testworld::spawn_tower(&mut w, tower, true)?;
        w.explore(P0, tower, f4x_def::half_int::HalfInt::from_int(3));
        let mut handler = super::super::testworld::handler(P0);
        {
            let view = PlayerView::new(&w, P0);
            handler.road_cities = view.my_cities().len();
            handler.road_checked = view
                .my_cities()
                .iter()
                .map(|&(c, _)| view.serial_of(c))
                .collect();
        }
        run(&mut handler, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let serial = view.serial_of(engineer);
        assert_eq!(handler.roles.role_of(serial), Some(Role::Road));
        assert!(view.unit_builds_road(engineer) || view.unit_has_order(engineer));
        Ok(())
    }

    #[test]
    fn a_way_over_water_is_no_way_for_a_road() {
        let w = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&w, P0);
        let (width, height) = view.map_size().expect("a map");
        let squares: Vec<Position> = (0..height)
            .flat_map(|y| (0..width).map(move |x| at(x, y)))
            .collect();
        assert!(squares.iter().any(|&p| view.road_possible(p)), "land");
        assert!(squares.iter().any(|&p| !view.road_possible(p)), "water");
        assert!(view.road_possible(at(COAST_CITY.0, COAST_CITY.1)));
    }
}
