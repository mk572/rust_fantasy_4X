//! Settlers by sea (`docs/ai-design.md` 11.3): a wizard whose own land mass has no site left for a
//! city, and who holds fewer cities than it wants, sends a settler by ship to explored land that
//! none of its own stands on and where a city can go. The port builds the ship and the settler
//! (`economy.rs`); here the plan is chosen and the voyage sailed. The settler then founds the city
//! like any other (`expand.rs`).

use std::collections::{HashSet, VecDeque};

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{distance, is_coastal, is_ship};
use crate::Ai;

/// The most squares of sea and of one land mass a search goes through.
const SEA_LIMIT: usize = 3000;
const LAND_LIMIT: usize = 1500;
/// Sites keep this far from the cities seen, and from a rival's by more.
const SPACING: u32 = 4;
const FROM_RIVAL: u32 = 5;
/// Turns a voyage may take before it is given up.
const SAIL_TURNS: u32 = 30;
/// Turns before a search that found nothing is made again.
const RETRY_TURNS: u32 = 20;

/// Where a settler goes by sea: the port it sails from, the sea square where the ship waits and
/// the land square beside it where the settler steps ashore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colony {
    pub port: Position,
    pub sea: Position,
    pub land: Position,
}

/// A city seen: where, and whether a rival wizard holds it.
type Seen = (Position, bool);

fn cities_seen(view: &PlayerView) -> Vec<Seen> {
    let mine = view.my_cities().into_iter().map(|(_, p)| (p, false));
    let others = view
        .known_cities()
        .into_iter()
        .map(|(_, owner, p)| (p, view.is_rival_wizard(owner)));
    mine.chain(others).collect()
}

/// Whether a city can go on the square, clear of every city seen.
fn is_site(view: &PlayerView, cities: &[Seen], at: Position) -> bool {
    view.can_found_city(at)
        && !view.has_lair(at)
        && !view.in_node(at)
        && cities.iter().all(|&(c, rival)| {
            let need = if rival { FROM_RIVAL } else { SPACING };
            distance(view, c, at).is_none_or(|d| d >= need)
        })
}

/// The explored land of the square's land mass, and whether it has an unexplored edge.
fn mass(view: &PlayerView, start: Position) -> (HashSet<Position>, bool) {
    let mut seen: HashSet<Position> = HashSet::from([start]);
    let mut queue: VecDeque<Position> = VecDeque::from([start]);
    let mut open_edge = false;
    while let Some(here) = queue.pop_front() {
        if seen.len() > LAND_LIMIT {
            break;
        }
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let Some(next) = view.shift(here, dx, dy) else {
                    continue;
                };
                if !view.explored(next) {
                    open_edge = true;
                } else if view.terrain_class(next) != Some(TerrainClass::Ocean) && seen.insert(next)
                {
                    queue.push_back(next);
                }
            }
        }
    }
    (seen, open_edge)
}

/// Whether the land mass of the port is used up: all of it explored and no site left on it.
fn closed(view: &PlayerView, cities: &[Seen], port: Position) -> bool {
    let (land, open_edge) = mass(view, port);
    !open_edge && !land.iter().any(|&p| is_site(view, cities, p))
}

/// The nearest sea square from the port beside land of another land mass that none of the wizard's
/// stands on and that has a site for a city: the sea square and the land square.
fn choose(
    view: &PlayerView,
    cities: &[Seen],
    port: Position,
    bad: &[Position],
) -> Option<(Position, Position)> {
    let mine: HashSet<Position> = view
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .chain(
            view.my_units()
                .into_iter()
                .filter(|&u| !is_ship(view, u))
                .filter_map(|u| view.unit_position(u)),
        )
        .collect();
    let (home, _) = mass(view, port);
    let mut checked: HashSet<Position> = home;
    let mut good: HashSet<Position> = HashSet::new();
    let mut seen: HashSet<Position> = HashSet::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if let Some(p) = view.shift(port, dx, dy) {
                if view.terrain_class(p) == Some(TerrainClass::Ocean) && seen.insert(p) {
                    queue.push_back(p);
                }
            }
        }
    }
    while let Some(sea) = queue.pop_front() {
        if seen.len() > SEA_LIMIT {
            break;
        }
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let Some(next) = view.shift(sea, dx, dy) else {
                    continue;
                };
                if !view.explored(next) {
                    continue;
                }
                if view.terrain_class(next) == Some(TerrainClass::Ocean) {
                    if view.hostile_at(next).is_none() && seen.insert(next) {
                        queue.push_back(next);
                    }
                    continue;
                }
                if bad.contains(&next) {
                    continue;
                }
                if !checked.contains(&next) {
                    let (land, _) = mass(view, next);
                    checked.extend(land.iter().copied());
                    if !land.iter().any(|p| mine.contains(p))
                        && land.iter().any(|&p| is_site(view, cities, p))
                    {
                        good.extend(land);
                    }
                }
                if !good.contains(&next)
                    || view.hostile_at(next).is_some()
                    || view.has_lair(next)
                    || view.city_at(next).is_some()
                {
                    continue;
                }
                return Some((sea, next));
            }
        }
    }
    None
}

/// The city that sails the settlers: the oldest real coastal one.
fn port_city(view: &PlayerView) -> Option<Position> {
    view.my_cities()
        .into_iter()
        .find(|&(c, p)| is_coastal(view, p) && view.city_economy(c).is_some_and(|e| e.figures > 0))
        .map(|(_, p)| p)
}

pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // Forget a voyage whose ship or settler is gone, or whose port was lost.
    {
        let view = PlayerView::new(game, me);
        let alive = |s: Serial| view.my_units().iter().any(|&u| view.serial_of(u) == s);
        if ai
            .colonists
            .is_some_and(|(ship, settler)| !alive(ship) || !alive(settler))
        {
            ai.colonists = None;
            ai.colony = None;
        }
        if ai
            .colony
            .is_some_and(|c| view.city_at(c.port).and_then(|e| view.city_owner(e)) != Some(me))
        {
            ai.colonists = None;
            ai.colony = None;
        }
    }
    if ai.colony.is_none() && game.turn >= ai.colony_retry {
        let view = PlayerView::new(game, me);
        let wanted = crate::objective::traits(&view).cities_wanted;
        let chosen = (view.my_cities().len() < wanted && ai.ferry.is_none())
            .then(|| port_city(&view))
            .flatten()
            .and_then(|port| {
                let cities = cities_seen(&view);
                closed(&view, &cities, port)
                    .then(|| choose(&view, &cities, port, &ai.colony_bad))
                    .flatten()
                    .map(|(sea, land)| Colony { port, sea, land })
            });
        ai.colony = chosen;
        if chosen.is_none() {
            ai.colony_retry = game.turn + RETRY_TURNS;
        }
    }
    let Some(colony) = ai.colony else {
        return;
    };
    release_scouts(ai, game, events);
    match ai.colonists {
        None => load(ai, game, colony, events),
        Some((ship, settler)) => sail(ai, game, colony, ship, settler, events),
    }
}

/// The sea scout is called off: the ship has a settler to carry.
fn release_scouts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let ships: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        view.my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u) && ai.scouts.contains(&view.serial_of(u)))
            .collect()
    };
    if ships.is_empty() {
        return;
    }
    let serials: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        ships.iter().map(|&s| view.serial_of(s)).collect()
    };
    ai.scouts.retain(|s| !serials.contains(s));
    ai.sea_party.clear();
    ai.sea_scouting = false;
    ai.send(game, Command::ClearOrders { units: ships }, events);
}

/// With the ship and a settler in the port, sail.
fn load(ai: &mut Ai, game: &mut Game, colony: Colony, events: &mut Vec<Event>) {
    let me = ai.player;
    let ferrying = ai.ferry.as_ref().map(|f| f.ship);
    let (ship, settler, strays) = {
        let view = PlayerView::new(game, me);
        let mut ships: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u))
            .filter(|&u| !ai.scouts.contains(&view.serial_of(u)))
            .filter(|&u| Some(view.serial_of(u)) != ferrying && !view.unit_has_order(u))
            .collect();
        view.sort_by_serial(&mut ships);
        let (here, strays): (Vec<_>, Vec<_>) = ships
            .into_iter()
            .partition(|&s| view.unit_position(s) == Some(colony.port));
        let mut settlers: Vec<hecs::Entity> = view
            .garrison(colony.port)
            .into_iter()
            .filter(|&u| view.unit_has_ability(u, "create_outpost") && !view.unit_has_order(u))
            .filter(|&u| !ai.settler_targets.contains_key(&view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut settlers);
        (here.first().copied(), settlers.first().copied(), strays)
    };
    // A ship left at sea by an earlier voyage comes home.
    if !strays.is_empty() {
        ai.send(
            game,
            Command::GoTo {
                units: strays,
                to: colony.port,
            },
            events,
        );
    }
    let (Some(ship), Some(settler)) = (ship, settler) else {
        return;
    };
    // Soldiers still aboard from an earlier voyage take up the ship's room: they step ashore
    // first, and the settler sails next turn.
    if crate::naval::unload(ai, game, ship, colony.port, &[], events) {
        return;
    }
    let (ship_serial, settler_serial) = {
        let view = PlayerView::new(game, me);
        (view.serial_of(ship), view.serial_of(settler))
    };
    if ai.send(
        game,
        Command::GoTo {
            units: vec![ship, settler],
            to: colony.sea,
        },
        events,
    ) {
        ai.colonists = Some((ship_serial, settler_serial));
        ai.colony_since = game.turn;
    } else {
        give_up(ai, colony);
    }
}

/// The ship sails; when it is at the sea square the settler steps ashore and the settlers' own
/// logic takes over.
fn sail(
    ai: &mut Ai,
    game: &mut Game,
    colony: Colony,
    ship: Serial,
    settler: Serial,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let (ship_e, settler_e, settler_at, idle) = {
        let view = PlayerView::new(game, me);
        let find = |s: Serial| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        };
        let (Some(ship_e), Some(settler_e)) = (find(ship), find(settler)) else {
            return;
        };
        (
            ship_e,
            settler_e,
            view.unit_position(settler_e),
            !view.unit_has_order(ship_e),
        )
    };
    if settler_at == Some(colony.sea) {
        if ai.send(
            game,
            Command::GoTo {
                units: vec![settler_e],
                to: colony.land,
            },
            events,
        ) {
            ai.colonists = None;
            ai.colony = None;
        } else {
            give_up(ai, colony);
        }
        return;
    }
    if game.turn.saturating_sub(ai.colony_since) > SAIL_TURNS {
        ai.send(
            game,
            Command::ClearOrders {
                units: vec![ship_e, settler_e],
            },
            events,
        );
        give_up(ai, colony);
        return;
    }
    if idle
        && !ai.send(
            game,
            Command::GoTo {
                units: vec![ship_e, settler_e],
                to: colony.sea,
            },
            events,
        )
    {
        give_up(ai, colony);
    }
}

/// A landing that did not work out is not tried again.
fn give_up(ai: &mut Ai, colony: Colony) {
    ai.colony_bad.push(colony.land);
    ai.colonists = None;
    ai.colony = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, build, game, spawn, COAST_CITY, ENEMY_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    /// The west island filled with cities of the wizard, as close as the rules let them stand.
    fn fill_west(g: &mut Game) {
        let race = g.content().rules.races.id_of("high_men");
        for x in 3..=10 {
            for y in 3..=10 {
                g.apply(f4x_core::game::Command::FoundCity {
                    owner: P0,
                    pos: at(x, y),
                    name: format!("C{x}{y}"),
                    race,
                    population: 5,
                });
            }
        }
    }

    #[test]
    fn a_site_on_the_wizards_own_island_means_no_voyage() {
        let mut g = game(COAST_CITY, "high_men");
        g.explore(P0, port(), 24);
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), 6);
        }
        let view = PlayerView::new(&g, P0);
        let cities = cities_seen(&view);
        // The west island has room for more cities than the capital.
        assert!(!closed(&view, &cities, port()));
    }

    #[test]
    fn the_nearest_free_island_is_found_from_a_full_one() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        g.explore(P0, port(), 24);
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), 6);
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), 6);
        // Fill the west island with cities, spaced as the AI spaces them.
        fill_west(&mut g);
        let view = PlayerView::new(&g, P0);
        let cities = cities_seen(&view);
        let (sea, land) =
            choose(&view, &cities, port(), &[]).ok_or("the east island has room for a city")?;
        assert!(land.x >= 28 && sea.x < 28 + 2, "{sea:?} {land:?}");
        Ok(())
    }

    #[test]
    fn a_settler_in_a_full_islands_port_sails_and_founds_a_city_across() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        build(
            &mut g,
            port(),
            &["ship_wrights_guild", "sawmill", "ship_yard"],
        );
        g.explore(P0, port(), 24);
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), 6);
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), 7);
        fill_west(&mut g);
        // An expansionist wants ten cities.
        g.wizard_mut(P0)
            .ok_or("the west wizard exists")?
            .put(Some(f4x_core::wizard::objective::Objective::Expansionist));
        spawn(&mut g, P0, port(), "galley", 1);
        spawn(&mut g, P0, port(), "high_men_settlers", 1);
        let mut ai = Ai::new(P0);
        let before = PlayerView::new(&g, P0).my_cities().len();
        for _ in 0..40 {
            let mut events = Vec::new();
            crate::expand::settlers(&mut ai, &mut g, &mut events);
            run(&mut ai, &mut g, &mut events);
            g.apply(f4x_core::game::Command::EndTurn);
        }
        let cities = PlayerView::new(&g, P0).my_cities();
        assert!(cities.len() > before, "{cities:?}");
        assert!(
            cities.iter().any(|&(_, p)| p.x >= 28),
            "a city on the east island: {cities:?}"
        );
        Ok(())
    }
}
