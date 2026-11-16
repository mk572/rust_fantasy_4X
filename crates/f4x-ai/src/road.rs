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
//! See `docs/ai-unit-registry.md`.

use crate::roles::Role;
use crate::Ai;
use f4x_core::city::production::ProductionItem;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;

/// The nearest cities an engineer links a city to.
const NEIGHBOURS: usize = 3;
/// Squares apart the cities of a link are at most.
const LINK_RANGE: u32 = 12;
/// Cities from which the roads are long enough for a second engineer.
const LONG_ROADS: usize = 5;

/// Squares of a road toward the campaign target built at a time.
const STRETCH: usize = 8;

/// The engineers: units that build roads.
fn is_engineer(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_stat(unit, Stat::Construction) > 0
}

/// One turn of the engineers: each that is idle gets a road to build, or a place to walk to.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let turn = game.turn;
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
    if count != ai.road_cities {
        ai.road_cities = count;
        ai.road_checked.clear();
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
            let _ = ai.roles.claim(serial, Role::Road, turn, None);
            continue;
        }
        if ai.roles.claim(serial, Role::Road, turn, None).is_err() {
            continue;
        }
        match work(ai, game, engineer, events) {
            Some(to) => {
                let _ = ai.roles.claim(serial, Role::Road, turn, Some(to));
            }
            None => {
                ai.roles.release(serial, Role::Road);
            }
        }
    }
}

/// Give one idle engineer its work: the square it works toward, when it found some.
fn work(
    ai: &mut Ai,
    game: &mut Game,
    engineer: hecs::Entity,
    events: &mut Vec<Event>,
) -> Option<Position> {
    let me = ai.player;
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
            ai.roles
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
            if path.iter().all(|&p| PlayerView::new(game, me).has_road(p)) {
                continue;
            }
            if build(ai, game, engineer, path, events) {
                return Some(to);
            }
        }
        let serial = PlayerView::new(game, me).serial_of(city);
        if !ai.road_checked.contains(&serial) {
            ai.road_checked.push(serial);
        }
    }
    // Every city looked at: toward the campaign target.
    if cities.iter().all(|&(c, _)| {
        ai.road_checked
            .contains(&PlayerView::new(game, me).serial_of(c))
    }) {
        return frontier(ai, game, engineer, events);
    }
    // Else to the nearest city not looked at, that has room for the engineer (a full city
    // refuses it, `StackFull`; the cities with no room wait). With none left, to the frontier.
    let Some(next) = nearest_unchecked(ai, game, engineer, here, &cities) else {
        return frontier(ai, game, engineer, events);
    };
    ai.send(
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
    ai: &Ai,
    game: &Game,
    engineer: hecs::Entity,
    here: Position,
    cities: &[(hecs::Entity, Position)],
) -> Option<Position> {
    let view = PlayerView::new(game, ai.player);
    cities
        .iter()
        .filter(|&&(c, p)| p != here && !ai.road_checked.contains(&view.serial_of(c)))
        .filter(|&&(_, p)| Ai::room_at(game, p, 1))
        .filter_map(|&(_, p)| view.distance(here, p).map(|d| (d, p)))
        .filter(|&(_, p)| view.can_reach(&[engineer], p))
        .min_by_key(|&(d, p)| (d, p.plane as u8, p.y, p.x))
        .map(|(_, p)| p)
}

/// Build a road along `path`: whether the game took the order.
fn build(
    ai: &Ai,
    game: &mut Game,
    engineer: hecs::Entity,
    path: Vec<Position>,
    events: &mut Vec<Event>,
) -> bool {
    ai.send(
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
fn frontier_way(ai: &Ai, game: &Game, engineer: hecs::Entity) -> Option<(Vec<Position>, usize)> {
    let view = PlayerView::new(game, ai.player);
    let here = view.unit_position(engineer)?;
    let mut towers: Vec<Position> = view
        .towers()
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| p.plane == here.plane)
        .collect();
    towers.sort_by_key(|&p| (view.distance(here, p).unwrap_or(u32::MAX), p.y, p.x));
    ai.campaign.into_iter().chain(towers).find_map(|target| {
        let mut path = view.path(&[engineer], target)?;
        path.pop();
        let first = path.iter().position(|&p| !view.has_road(p))?;
        Some((path, first))
    })
}

/// A stretch of road toward the campaign target or a tower: built from where the engineer
/// stands when the first square has none, else the engineer walks to the last square that has
/// one. Near a rival's cities it works only with an escort.
fn frontier(
    ai: &mut Ai,
    game: &mut Game,
    engineer: hecs::Entity,
    events: &mut Vec<Event>,
) -> Option<Position> {
    let me = ai.player;
    let (path, first) = frontier_way(ai, game, engineer)?;
    // The stretch another engineer is on is left to it.
    {
        let serial = PlayerView::new(game, me).serial_of(engineer);
        if ai
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
        if near_enemy && !crate::escort::guard(ai, game, engineer, events) {
            // It waits where it stands for the escort to come.
            return PlayerView::new(game, me).unit_position(engineer);
        }
        return build(ai, game, engineer, stretch, events).then_some(end);
    }
    let to = path[first - 1];
    ai.send(
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

/// How many engineers the wizard wants: one, and a second when the roads are long (many cities
/// still to link, or a campaign and towers to reach).
fn engineers_wanted(cities: usize, checked: usize, far_work: bool) -> usize {
    if cities >= LONG_ROADS && (checked + LONG_ROADS <= cities || far_work) {
        2
    } else {
        1
    }
}

/// The engineers' production: while there is road to build and no engineer, the city of most
/// people that can make them builds one.
pub fn build_engineer(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let city = {
        let view = PlayerView::new(game, me);
        let cities = view.my_cities();
        let work = cities.len() >= 2 && ai.road_checked.len() < cities.len()
            || ai.campaign.is_some()
            || !PlayerView::new(game, me).towers().is_empty();
        let engineers = view
            .my_units()
            .iter()
            .filter(|&&u| is_engineer(&view, u))
            .count();
        let wanted = engineers_wanted(
            cities.len(),
            ai.road_checked.len(),
            ai.campaign.is_some() && !view.towers().is_empty(),
        );
        if !work || engineers >= wanted {
            return;
        }
        let mut best: Option<(u32, hecs::Entity, ProductionItem)> = None;
        for (city, pos) in cities {
            let current = view.city_production(city).item;
            let Some(item) = engineer_item(&view, city) else {
                continue;
            };
            // A building under way is not thrown away for the hall.
            if (matches!(current, Some(ProductionItem::Building(_))) && current != Some(item))
                || ai.threatened.contains(&pos)
            {
                continue;
            }
            let size = view.city_economy(city).map_or(0, |e| e.figures);
            // A city already at it goes first.
            let score = size + if current == Some(item) { 1000 } else { 0 };
            if best.is_none_or(|(s, _, _)| score > s) {
                best = Some((score, city, item));
            }
        }
        best
    };
    if let Some((_, city, item)) = city {
        if game.economy().city_production(city).item != Some(item) {
            ai.order(game, Command::SetProduction { city, item }, events);
        }
    }
}

/// What the city builds toward engineers: the cheapest engineers of its race when it can build
/// them, else the cheapest building of the chain they need (the builders' hall).
fn engineer_item(view: &PlayerView, city: hecs::Entity) -> Option<ProductionItem> {
    let rules = &view.content().rules;
    let race = rules.races.get(view.city_race(city)?);
    let unit = race
        .units
        .iter()
        .copied()
        .filter(|&u| {
            let d = rules.units.get(u);
            d.stats
                .get(&Stat::Construction)
                .is_some_and(|v| v.as_f32() > 0.0)
                && !d.abilities.contains(&f4x_def::rules::Ability::new(12))
        })
        .min_by_key(|&u| rules.units.get(u).cost)?;
    if view.can_build(city, ProductionItem::Unit(unit)).is_ok() {
        return Some(ProductionItem::Unit(unit));
    }
    crate::forge::chain(view, unit)
        .into_iter()
        .map(ProductionItem::Building)
        .filter(|&i| view.can_build(city, i).is_ok())
        .min_by_key(|&i| match i {
            ProductionItem::Building(b) => (rules.buildings.get(b).cost, b),
            _ => (0, f4x_def::rules::BuildingId::new(0)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Two cities of player 0 and engineers (high men) in the first.
    fn world() -> Result<(Game, hecs::Entity), Box<dyn std::error::Error>> {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let race = g.content().rules.races.id_numbered(8).ok_or("high men")?;
        let events = g.apply(Command::FoundCity {
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
        let engineer = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 110, 1)[0];
        g.balance_farmers(P0);
        Ok((g, engineer))
    }

    /// An engineer in a city with another own city near, and no road between them, builds one.
    #[test]
    fn an_engineer_builds_the_road_to_the_next_city() -> TestResult {
        let (mut g, engineer) = world()?;
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let serial = view.serial_of(engineer);
        assert!(view.unit_builds_road(engineer));
        assert_eq!(ai.roles.role_of(serial), Some(Role::Road));
        assert_eq!(
            ai.roles.target_of(serial, Role::Road),
            Some(at(INLAND_CITY.0, INLAND_CITY.1))
        );
        Ok(())
    }

    /// A city with no room for one more unit is not the engineer's next stop (`StackFull`).
    #[test]
    fn an_engineer_does_not_walk_to_a_full_city() -> TestResult {
        let (mut g, engineer) = world()?;
        let ai = Ai::new(P0);
        let (first, second) = (
            at(COAST_CITY.0, COAST_CITY.1),
            at(INLAND_CITY.0, INLAND_CITY.1),
        );
        let cities: Vec<_> = PlayerView::new(&g, P0).my_cities();
        assert_eq!(
            nearest_unchecked(&ai, &g, engineer, first, &cities),
            Some(second)
        );
        let room = g.max_tile() - g.units().units_at(second).len();
        spawn(&mut g, P0, second, 110, room);
        assert_eq!(nearest_unchecked(&ai, &g, engineer, first, &cities), None);
        Ok(())
    }

    /// Engineers are not soldiers: no escort, march or garrison count takes them.
    #[test]
    fn an_engineer_is_no_fighter() -> TestResult {
        let (g, engineer) = world()?;
        let view = PlayerView::new(&g, P0);
        assert!(!crate::view::is_fighter(&view, engineer));
        Ok(())
    }

    /// With every link done and no campaign the engineer gives the role back, and the cities
    /// are marked as looked at.
    #[test]
    fn an_engineer_with_nothing_to_build_gives_the_role_back() -> TestResult {
        let (mut g, engineer) = world()?;
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut Vec::new());
        // Let the work finish.
        for _ in 0..30 {
            if !PlayerView::new(&g, P0).unit_builds_road(engineer) {
                break;
            }
            g.apply(Command::EndTurn);
        }
        // It looks at the other city, finds the link done and has nothing left.
        let serial = PlayerView::new(&g, P0).serial_of(engineer);
        for _ in 0..10 {
            run(&mut ai, &mut g, &mut Vec::new());
            if !ai.roles.has_role(serial) {
                break;
            }
            g.apply(Command::EndTurn);
        }
        let view = PlayerView::new(&g, P0);
        assert!(!ai.roles.has_role(serial));
        assert!(!view.unit_builds_road(engineer));
        // The road between the cities is there.
        let roads = (4..=12)
            .flat_map(|x| (4..=8).map(move |y| (x, y)))
            .filter(|&(x, y)| view.has_road(at(x, y)))
            .count();
        assert!(roads >= 5, "{roads} squares of road");
        Ok(())
    }

    /// A wizard with two cities and no engineers makes its biggest city build them.
    #[test]
    fn a_wizard_with_roads_to_build_and_no_engineer_builds_one() -> TestResult {
        let (mut g, engineer) = world()?;
        g.cheat_remove_unit(engineer);
        let mut ai = Ai::new(P0);
        build_engineer(&mut ai, &mut g, &mut Vec::new());
        let city = g
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("city")?;
        let item = g.economy().city_production(city).item;
        // The city has no builders' hall yet: it builds that toward the engineers.
        assert!(
            matches!(item, Some(ProductionItem::Building(_))),
            "{item:?}"
        );
        Ok(())
    }

    /// One engineer is enough for a few cities; with many still to link a second is wanted.
    #[test]
    fn long_roads_want_a_second_engineer() {
        assert_eq!(engineers_wanted(2, 0, false), 1);
        assert_eq!(engineers_wanted(6, 5, false), 1, "nearly all linked");
        assert_eq!(engineers_wanted(6, 0, false), 2);
        assert_eq!(engineers_wanted(6, 6, true), 2, "a far campaign");
        assert_eq!(engineers_wanted(3, 0, true), 1, "too few cities");
    }

    /// Two engineers in a city take two different links.
    #[test]
    fn two_engineers_take_different_links() -> TestResult {
        let (mut g, first) = world()?;
        let race = g.content().rules.races.id_numbered(8).ok_or("high men")?;
        for (x, y) in [(6, 10), (10, 10)] {
            let events = g.apply(Command::FoundCity {
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
        g.balance_farmers(P0);
        let second = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 110, 1)[0];
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let (a, b) = (view.serial_of(first), view.serial_of(second));
        let (ta, tb) = (
            ai.roles.target_of(a, Role::Road),
            ai.roles.target_of(b, Role::Road),
        );
        assert!(ta.is_some() && tb.is_some() && ta != tb, "{ta:?} {tb:?}");
        Ok(())
    }

    /// With every city looked at and no campaign, the engineer builds toward the nearest tower.
    #[test]
    fn an_engineer_builds_toward_a_tower_when_the_cities_are_linked() -> TestResult {
        let (mut g, engineer) = world()?;
        let tower = at(8, 3);
        crate::testworld::spawn_tower(&mut g, tower, true)?;
        g.explore(P0, tower, f4x_def::half_int::HalfInt::from_int(3));
        let mut ai = Ai::new(P0);
        {
            let view = PlayerView::new(&g, P0);
            ai.road_cities = view.my_cities().len();
            ai.road_checked = view
                .my_cities()
                .iter()
                .map(|&(c, _)| view.serial_of(c))
                .collect();
        }
        run(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        let serial = view.serial_of(engineer);
        assert_eq!(ai.roles.role_of(serial), Some(Role::Road));
        assert!(view.unit_builds_road(engineer) || view.unit_has_order(engineer));
        Ok(())
    }
}
