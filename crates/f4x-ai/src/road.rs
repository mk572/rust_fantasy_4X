//! Engineers build roads (`Role::Road`): first the links between the wizard's own cities, then the
//! way toward the city its army is gathering against. The roads halve the walking of every army
//! and settler after them.
//!
//! Nothing is planned and kept but which cities have been looked at. An engineer standing in an
//! own city takes the nearest few cities, asks the game for the way it would walk there, and builds
//! a road along it when any square of it has none. When the city has no such link it is marked
//! looked at and the engineer walks to the nearest city not yet looked at. When every city has been
//! looked at the engineer works toward the campaign target, else the nearest tower, a stretch at a
//! time, from wherever it stands; within `engineer_escort_range` squares of a rival's city it works only with
//! an escort (`escort::guard`) and waits for one. With nothing left it gives the role back. The marks are cleared when the number of
//! cities changes. One engineer unit is built (`build_engineer`) while there is work and none exists.
//!
//! See `docs/ai-unit-registry.md`.

use f4x_def::rules::ai_strategy::EngineerLevers;

use crate::lever::Levered;
use crate::roles::Role;
use crate::Ai;
use f4x_core::city::production::ProductionItem;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;
use f4x_core::world::{Command, Event, World};
use f4x_master::Game;

/// The engineers: units that build roads.
fn is_engineer(view: &PlayerView, unit: hecs::Entity) -> bool {
    view.unit_stat(unit, Stat::Construction) > 0
}

/// Whether this wizard can have roads built at all: an engineer is alive, or an own city can
/// build engineers (or the hall that makes them). Some races have none (`engineer_item`), and a
/// wizard of one has them only through cities of another race.
pub(crate) fn engineers_possible(view: &PlayerView) -> bool {
    view.my_units().into_iter().any(|u| is_engineer(view, u))
        || view
            .my_cities()
            .into_iter()
            .any(|(c, _)| engineer_item(view, c).is_some())
}

/// One turn of the engineers: each that is idle gets a road to build, or a place to walk to.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
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
        // An engineer aboard a ship at sea has no road to build.
        let afloat = {
            let view = PlayerView::new(game, me);
            view.unit_position(engineer)
                .is_some_and(|p| !view.road_possible(p))
        };
        if afloat || ai.roles.claim(serial, Role::Road, turn, None).is_err() {
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
                .filter(|&(d, _)| d <= view.levers().engineers.link_range)
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
        for (_, to) in near.into_iter().take(game.levers().engineers.linked_cities) {
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
    world: &World,
    engineer: hecs::Entity,
    here: Position,
    cities: &[(hecs::Entity, Position)],
) -> Option<Position> {
    let view = PlayerView::new(world, ai.player);
    cities
        .iter()
        .filter(|&&(c, p)| p != here && !ai.road_checked.contains(&view.serial_of(c)))
        .filter(|&&(_, p)| Ai::room_at(world, p, 1))
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

/// The way a road is built toward: the campaign target, else the nearest tower, whichever has a
/// way to it that lacks a road. The squares of that way, the last (the city or tower itself) left
/// out, and the first without a road.
fn frontier_way(ai: &Ai, world: &World, engineer: hecs::Entity) -> Option<(Vec<Position>, usize)> {
    let view = PlayerView::new(world, ai.player);
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
        let stretch: Vec<Position> = path
            .into_iter()
            .take(game.levers().engineers.road_stretch)
            .collect();
        let end = *stretch.last()?;
        let near_enemy = {
            let view = PlayerView::new(game, me);
            view.known_cities().into_iter().any(|(_, owner, city)| {
                owner != me
                    && view.is_rival_wizard(owner)
                    && stretch.iter().any(|&p| {
                        let range = view.levers().engineers.engineer_escort_range;
                        view.distance(p, city).is_some_and(|d| d <= range)
                    })
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

/// How many engineers the wizard wants while there is road to build: one for each
/// `cities_per_engineer` cities, one at least.
fn engineers_wanted(levers: &EngineerLevers, cities: usize) -> usize {
    cities.div_ceil(levers.cities_per_engineer).max(1)
}

/// The engineers' production: while there is road to build and fewer engineers than
/// [`engineers_wanted`], the city of most people that can make them builds one.
pub fn build_engineer(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let city = {
        let view = PlayerView::new(game, me);
        if !engineers_possible(&view) {
            return;
        }
        let cities = view.my_cities();
        let work = cities.len() >= 2 && ai.road_checked.len() < cities.len()
            || ai.campaign.is_some()
            || !PlayerView::new(game, me).towers().is_empty();
        let engineers = view
            .my_units()
            .iter()
            .filter(|&&u| is_engineer(&view, u))
            .count();
        let wanted = engineers_wanted(&view.levers().engineers, cities.len());
        if !work || engineers >= wanted {
            return;
        }
        let spare = crate::builder::spare_food(&view);
        let mut best: Option<(u32, hecs::Entity, ProductionItem)> = None;
        for (city, pos) in cities {
            let current = view.city_production(city).item;
            let Some(item) = engineer_item(&view, city) else {
                continue;
            };
            // An engineer the empire cannot feed is let go at the turn change, and the next one
            // is started at once.
            if let ProductionItem::Unit(u) = item {
                if view.content().rules.units.get(u).upkeep.food > spare {
                    continue;
                }
            }
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
                && !d.is_settler()
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

    fn engineers() -> EngineerLevers {
        crate::testworld::levers().engineers
    }
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use f4x_master::Game;

    /// A race with engineers can have roads built; one without can not, until an engineer exists.
    #[test]
    fn roads_are_possible_only_with_a_race_that_has_engineers_or_an_engineer() {
        let possible = |race: u16| {
            let w = game(COAST_CITY, race);
            engineers_possible(&PlayerView::new(&w, P0))
        };
        assert!(possible(8), "high men build engineers");
        assert!(!possible(7), "high elves have none");
        let mut w = game(COAST_CITY, 7);
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            110, /* high_men_engineers */
            1,
        );
        assert!(
            engineers_possible(&PlayerView::new(&w, P0)),
            "a captured one"
        );
    }

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
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
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
        let (mut w, engineer) = world()?;
        let ai = Ai::new(P0);
        let (first, second) = (
            at(COAST_CITY.0, COAST_CITY.1),
            at(INLAND_CITY.0, INLAND_CITY.1),
        );
        let cities: Vec<_> = PlayerView::new(&w, P0).my_cities();
        assert_eq!(
            nearest_unchecked(&ai, &w, engineer, first, &cities),
            Some(second)
        );
        let room = w
            .max_tile()
            .saturating_sub(w.units().units_at(second).len());
        spawn(&mut w, P0, second, 110, room);
        assert_eq!(nearest_unchecked(&ai, &w, engineer, first, &cities), None);
        Ok(())
    }

    /// Engineers are not soldiers: no escort, march or garrison count takes them.
    #[test]
    fn an_engineer_is_no_fighter() -> TestResult {
        let (w, engineer) = world()?;
        let view = PlayerView::new(&w, P0);
        assert!(!crate::view::is_fighter(&view, engineer));
        Ok(())
    }

    /// With every link done and no campaign the engineer gives the role back, and the cities
    /// are marked as looked at.
    #[test]
    fn an_engineer_with_nothing_to_build_gives_the_role_back() -> TestResult {
        let (mut w, engineer) = world()?;
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
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
            run(&mut ai, &mut w, &mut Vec::new());
            if !ai.roles.has_role(serial) {
                break;
            }
            w.play_round();
        }
        let view = PlayerView::new(&w, P0);
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
        let (mut w, engineer) = world()?;
        w.cheat_remove_unit(engineer);
        let mut ai = Ai::new(P0);
        build_engineer(&mut ai, &mut w, &mut Vec::new());
        let city = w
            .economy()
            .city_at(at(COAST_CITY.0, COAST_CITY.1))
            .ok_or("city")?;
        let item = w.economy().city_production(city).item;
        // The city has no builders' hall yet: it builds that toward the engineers.
        assert!(
            matches!(item, Some(ProductionItem::Building(_))),
            "{item:?}"
        );
        Ok(())
    }

    /// The wizard wants one engineer for each `cities_per_engineer` cities, and one at least.
    #[test]
    fn a_wizard_wants_one_engineer_for_every_few_cities() {
        assert_eq!(engineers_wanted(&engineers(), 1), 1);
        assert_eq!(
            engineers_wanted(&engineers(), engineers().cities_per_engineer),
            1
        );
        assert_eq!(
            engineers_wanted(&engineers(), engineers().cities_per_engineer + 1),
            2
        );
        assert_eq!(
            engineers_wanted(&engineers(), 3 * engineers().cities_per_engineer),
            3
        );
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
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
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
        let (mut w, engineer) = world()?;
        let tower = at(8, 3);
        crate::testworld::spawn_tower(&mut w, tower, true)?;
        w.explore(P0, tower, f4x_def::half_int::HalfInt::from_int(3));
        let mut ai = Ai::new(P0);
        {
            let view = PlayerView::new(&w, P0);
            ai.road_cities = view.my_cities().len();
            ai.road_checked = view
                .my_cities()
                .iter()
                .map(|&(c, _)| view.serial_of(c))
                .collect();
        }
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        let serial = view.serial_of(engineer);
        assert_eq!(ai.roles.role_of(serial), Some(Role::Road));
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
