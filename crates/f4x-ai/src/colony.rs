//! Settlers by sea (`docs/ai-design.md` 11.3): a wizard whose own land mass has no site left for a
//! city, and who holds fewer cities than it wants, picks explored land that none of its own stands
//! on and where a city can go. The port builds the ship and the settler (`economy.rs`); here the
//! site is chosen and given to a settler as its target. The settler is then like any other that
//! cannot walk to its site (task 647): it is tagged `Crossing::Sea`, a ship picks it up first of
//! all the units that wait (`pickup.rs`) and the settler founds the city like any other
//! (`expand.rs`).

use f4x_def::rules::Ability;
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
/// Turns before a search that found nothing is made again.
const RETRY_TURNS: u32 = 20;

/// Where a settler goes by sea: the port it sails from, the sea square where the ship waits and
/// the land square beside it where the settler steps ashore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    // The plan is over when the settler given the site is gone (it founded the city, or died), or
    // when the port was lost.
    {
        let view = PlayerView::new(game, me);
        if let Some(c) = ai.colony {
            let settler_gone = ai.colony_since > 0
                && !ai
                    .roles
                    .targets(crate::roles::Role::Settle)
                    .contains(&c.land);
            let port_lost = view.city_at(c.port).and_then(|e| view.city_owner(e)) != Some(me);
            if settler_gone || port_lost {
                ai.colony = None;
                ai.colony_since = 0;
            }
        }
    }
    if ai.colony.is_none() && game.turn >= ai.colony_retry {
        let view = PlayerView::new(game, me);
        let wanted = crate::objective::traits(&view).cities_wanted;
        let chosen = ((view.my_cities().len() < wanted || !crate::expand::room_for_city(&view))
            && crate::pickup::voyages(ai).is_empty())
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
    if ai.colony_since == 0 {
        give_site(ai, game, colony);
    }
}

/// A settler in the port that has no site on this island is given the colony's site: it needs a
/// ship to get there, and the pickup planner sends one.
fn give_site(ai: &mut Ai, game: &Game, colony: Colony) {
    let me = ai.player;
    let view = PlayerView::new(game, me);
    let mut settlers: Vec<hecs::Entity> = view
        .garrison(colony.port)
        .into_iter()
        .filter(|&u| view.unit_has_ability(u, Ability::new(12)) && !view.unit_has_order(u))
        .filter(|&u| {
            ai.roles
                .target_of(view.serial_of(u), crate::roles::Role::Settle)
                .is_none()
        })
        .collect();
    view.sort_by_serial(&mut settlers);
    let Some(&settler) = settlers.first() else {
        return;
    };
    let serial = view.serial_of(settler);
    let _ = ai.roles.claim(
        serial,
        crate::roles::Role::Settle,
        game.turn,
        Some(colony.land),
    );
    ai.roles
        .set_crossing(serial, Some(crate::roles::Crossing::Sea));
    ai.colony_since = game.turn;
}

/// The sea scout is called off: the ship has a settler to carry.
pub(crate) fn release_scouts(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let ships: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        view.my_units()
            .into_iter()
            .filter(|&u| is_ship(&view, u) && ai.roles.is_scouting(view.serial_of(u)))
            .collect()
    };
    if ships.is_empty() {
        return;
    }
    let serials: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        ships.iter().map(|&s| view.serial_of(s)).collect()
    };
    ai.roles.release_many(&serials, crate::roles::Role::Scout);
    ai.roles.release_role(crate::roles::Role::SeaParty);
    ai.sea_scouting = false;
    ai.order(game, Command::ClearOrders { units: ships }, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, build, game, spawn, COAST_CITY, ENEMY_CITY, P0};
    use f4x_def::half_int::HalfInt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn port() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    /// The west island filled with cities of the wizard, as close as the rules let them stand.
    fn fill_west(g: &mut Game) {
        let race = g.content().rules.races.id_numbered(8);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, port(), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        let view = PlayerView::new(&g, P0);
        let cities = cities_seen(&view);
        // The west island has room for more cities than the capital.
        assert!(!closed(&view, &cities, port()));
    }

    #[test]
    fn the_nearest_free_island_is_found_from_a_full_one() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, port(), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_int(3));
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        build(
            &mut g,
            port(),
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        g.explore(P0, port(), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut g);
        // An expansionist wants ten cities.
        g.wizard_mut(P0)
            .ok_or("the west wizard exists")?
            .put(Some(f4x_core::wizard::objective::Objective::Expansionist));
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        spawn(&mut g, P0, port(), 111 /* high_men_settlers */, 1);
        let mut ai = Ai::new(P0);
        let before = PlayerView::new(&g, P0).my_cities().len();
        for _ in 0..40 {
            let mut events = Vec::new();
            crate::roles::register(&mut ai, &g);
            crate::dispatch::revalidate(&mut ai, &mut g, &mut events);
            crate::expand::settlers(&mut ai, &mut g, &mut events);
            run(&mut ai, &mut g, &mut events);
            crate::pickup::run(&mut ai, &mut g, &mut events);
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

    /// The settler in the port of a full island is given the colony's site as its target and
    /// tagged for a ship; the settler that waits has no special role.
    #[test]
    fn a_settler_in_the_port_is_given_the_colony_site_and_waits_for_a_ship() -> TestResult {
        use crate::roles::{Crossing, Role};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.explore(P0, port(), HalfInt::from_int(12));
        for x in [16, 22, 26, 30] {
            g.explore(P0, at(x, 6), HalfInt::from_int(3));
        }
        g.explore(P0, at(ENEMY_CITY.0 + 4, ENEMY_CITY.1), HalfInt::from_raw(7));
        fill_west(&mut g);
        g.wizard_mut(P0)
            .ok_or("the west wizard exists")?
            .put(Some(f4x_core::wizard::objective::Objective::Expansionist));
        let settler = spawn(&mut g, P0, port(), 111 /* high_men_settlers */, 1)[0];
        let serial = PlayerView::new(&g, P0).serial_of(settler);
        let mut ai = Ai::new(P0);
        crate::expand::settlers(&mut ai, &mut g, &mut Vec::new());
        run(&mut ai, &mut g, &mut Vec::new());
        let colony = ai.colony.ok_or("a site is chosen")?;
        assert_eq!(ai.roles.role_of(serial), Some(Role::Settle));
        assert_eq!(ai.roles.target_of(serial, Role::Settle), Some(colony.land));
        assert_eq!(ai.roles.crossing_of(serial), Some(Crossing::Sea));
        Ok(())
    }

    /// A settler gets the ship before soldiers that want it too.
    #[test]
    fn a_settler_gets_the_only_free_ship_before_soldiers() -> TestResult {
        use crate::roles::{Crossing, Role};
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.reveal_map(P0);
        let land = {
            let view = PlayerView::new(&g, P0);
            crate::naval::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
                .ok_or("a shore on the far island")?
                .0
        };
        // The settler's site is on the far island, on a tile that sorts after the port, so that the
        // soldiers' group would come first were it not for the priority.
        let site = {
            let view = PlayerView::new(&g, P0);
            (0..8i32)
                .flat_map(|dy| (-6..=6i32).map(move |dx| (dx, dy)))
                .filter_map(|(dx, dy)| view.shift(land, dx, dy))
                .find(|&p| {
                    p.y > port().y
                        && view.terrain_class(p) != Some(TerrainClass::Ocean)
                        && crate::view::is_coastal(&view, p)
                })
                .ok_or("a coastal tile on the far island south of the port")?
        };
        let soldiers = spawn(&mut g, P0, land, 104 /* spearmen */, 2);
        let settler = spawn(&mut g, P0, port(), 111 /* high_men_settlers */, 1)[0];
        spawn(&mut g, P0, port(), 36 /* galley */, 1);
        let mut ai = Ai::new(P0);
        let serials: Vec<(hecs::Entity, f4x_core::game::serial::Serial, Position)> = {
            let view = PlayerView::new(&g, P0);
            soldiers
                .iter()
                .map(|&u| (u, view.serial_of(u), port()))
                .chain([(settler, view.serial_of(settler), site)])
                .collect()
        };
        for &(_, s, to) in &serials {
            let _ = ai.roles.claim(s, Role::Settle, g.turn, Some(to));
            ai.roles.set_crossing(s, Some(Crossing::Sea));
        }
        crate::pickup::run(&mut ai, &mut g, &mut Vec::new());
        let boards = |u: hecs::Entity| {
            serials
                .iter()
                .find(|&&(e, _, _)| e == u)
                .and_then(|&(_, s, _)| ai.roles.boarding_of(s))
        };
        assert!(boards(settler).is_some(), "the settler has the ship");
        assert!(soldiers.iter().all(|&u| boards(u).is_none()));
        Ok(())
    }
}
