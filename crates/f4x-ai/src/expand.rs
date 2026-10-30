//! Settlers: choose a place for the next city, walk there and found it.

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{distance, nearest_unexplored};
use crate::Ai;

/// How far from the settler the AI looks for a site.
const SEARCH_RADIUS: i32 = 9;
/// Sites keep this far apart from each other and from cities.
const SPACING: u32 = 4;

pub fn settlers(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let mine: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut mine: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| view.unit_has_ability(u, "create_outpost"))
            .collect();
        view.sort_by_serial(&mut mine);
        // Forget the targets of settlers that are gone.
        let alive: Vec<_> = mine.iter().map(|&u| view.serial_of(u)).collect();
        ai.settler_targets.retain(|s, _| alive.contains(s));
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
        if let Some(&target) = ai.settler_targets.get(&serial) {
            if target == pos {
                let (can_found, name) = {
                    let view = PlayerView::new(game, me);
                    (view.can_found_city(pos), city_name(&view, unit))
                };
                if can_found && ai.send(game, Command::SettleOutpost { unit, name }, events) {
                    ai.settler_targets.remove(&serial);
                    continue;
                }
                ai.settler_targets.remove(&serial); // blocked: choose again below
            } else if PlayerView::new(game, me).unit_has_order(unit) {
                if ai.settler_last.get(&serial) == Some(&pos) {
                    // It did not move all turn: the way is blocked. Give this site up.
                    ai.bad_sites.push(target);
                    ai.settler_targets.remove(&serial);
                    ai.send(game, Command::ClearOrders { units: vec![unit] }, events);
                } else {
                    ai.settler_last.insert(serial, pos);
                    continue; // still walking
                }
            }
        }
        ai.settler_last.insert(serial, pos);
        if !ai.settler_targets.contains_key(&serial) {
            let others: Vec<Position> = ai.settler_targets.values().copied().collect();
            let mut avoid = others;
            avoid.extend(ai.bad_sites.iter().copied());
            let (site, scout) = {
                let view = PlayerView::new(game, me);
                match choose_site(&view, unit, pos, &avoid) {
                    Some(site) => (Some(site), None),
                    // No explored square will do: walk to unexplored land to see more of it.
                    None => (None, nearest_unexplored(&view, unit)),
                }
            };
            match (site, scout) {
                (Some(site), _) => {
                    ai.settler_targets.insert(serial, site);
                }
                (None, Some(to)) => {
                    ai.send(
                        game,
                        Command::GoTo {
                            units: vec![unit],
                            to,
                        },
                        events,
                    );
                    continue;
                }
                (None, None) => continue,
            }
        }
        let target = ai.settler_targets[&serial];
        if target == pos {
            // Found it right here (next turn if the settlers just arrived and cannot).
            let name = city_name(&PlayerView::new(game, me), unit);
            if ai.send(game, Command::SettleOutpost { unit, name }, events) {
                ai.settler_targets.remove(&serial);
            }
        } else if !ai.send(
            game,
            Command::GoTo {
                units: vec![unit],
                to: target,
            },
            events,
        ) {
            ai.settler_targets.remove(&serial);
        }
    }
}

/// A name for the next city: the race's list, by how many cities the wizard has.
fn city_name(view: &PlayerView, unit: hecs::Entity) -> String {
    let used = view.my_cities().len();
    view.unit_def(unit)
        .and_then(|d| d.race.as_deref())
        .and_then(|r| view.content().rules.races.by_name(r))
        .and_then(|r| r.city_names.get(used % r.city_names.len().max(1)).cloned())
        .unwrap_or_else(|| format!("Outpost {}", used + 1))
}

/// How good a city here would be: the land it can work (only what has been explored counts).
fn site_value(view: &PlayerView, at: Position) -> f32 {
    let mut value = 0.0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            if dx.abs() == 2 && dy.abs() == 2 {
                continue;
            }
            let Some(p) = view.shift(at, dx, dy) else {
                continue;
            };
            value += match view.terrain_class(p) {
                Some(TerrainClass::Grassland | TerrainClass::SorceryNode) => 3.0,
                Some(TerrainClass::Forest | TerrainClass::NatureNode | TerrainClass::Hills) => 2.0,
                Some(TerrainClass::Swamp | TerrainClass::Mountains | TerrainClass::ChaosNode) => {
                    1.0
                }
                Some(TerrainClass::Desert) | Some(TerrainClass::Tundra) => 0.5,
                Some(TerrainClass::Ocean) => 0.7,
                None => 0.0,
            };
            if view.has_river(p) {
                value += 0.5;
            }
        }
    }
    value
}

fn choose_site(
    view: &PlayerView,
    unit: hecs::Entity,
    from: Position,
    taken: &[Position],
) -> Option<Position> {
    let mut candidates: Vec<(f32, Position)> = Vec::new();
    for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
        // Not the first or last row: nothing can be founded there.
        let Some(row) = view.shift(from, 0, dy) else {
            continue;
        };
        if row.y < 1 || view.shift(from, 0, dy + 1).is_none() {
            continue;
        }
        for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
            let Some(p) = view.shift(from, dx, dy) else {
                continue;
            };
            if !view.can_found_city(p)
                || view.has_lair(p)
                || view.in_node(p)
                || taken
                    .iter()
                    .any(|&t| distance(view, t, p).is_some_and(|d| d < SPACING))
            {
                continue;
            }
            let walk = dx.abs().max(dy.abs()) as f32;
            candidates.push((site_value(view, p) - walk * 0.7, p));
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let reachable = view.reachable(&[unit]);
    candidates
        .into_iter()
        .take(10)
        .map(|(_, p)| p)
        .find(|&p| p == from || reachable.contains(&p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The west island, explored by its wizard, with `count` settlers standing in the capital.
    fn island_with_settlers(count: usize) -> (Game, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, "high_men");
        g.explore(P0, at(7, 6), 8);
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            "high_men_settlers",
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
        assert!(ai.settler_targets.is_empty(), "the target is forgotten");
        Ok(())
    }

    #[test]
    fn two_settlers_choose_sites_that_keep_apart() -> TestResult {
        let (mut g, _) = island_with_settlers(2);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let sites: Vec<Position> = ai.settler_targets.values().copied().collect();
        assert_eq!(sites.len(), 2, "each settler has a site");
        let view = PlayerView::new(&g, P0);
        let apart = distance(&view, sites[0], sites[1]).ok_or("the sites are on one plane")?;
        assert!(apart >= SPACING, "{sites:?} are {apart} apart");
        Ok(())
    }

    #[test]
    fn a_settler_that_did_not_move_all_turn_gives_its_site_up_and_chooses_another() -> TestResult {
        let (mut g, units) = island_with_settlers(1);
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let serial = PlayerView::new(&g, P0).serial_of(units[0]);
        let first = *ai
            .settler_targets
            .get(&serial)
            .ok_or("the settler chose a site")?;
        // A whole turn went by without the settler leaving its square: the last look equals now.
        let here = PlayerView::new(&g, P0)
            .unit_position(units[0])
            .ok_or("the settler is on the map")?;
        ai.settler_last.insert(serial, here);
        settlers(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.bad_sites.contains(&first), "{:?}", ai.bad_sites);
        let second = *ai
            .settler_targets
            .get(&serial)
            .ok_or("the settler chose another site")?;
        assert_ne!(second, first);
        Ok(())
    }

    #[test]
    fn a_settler_that_finds_no_site_scouts_instead() -> TestResult {
        // Nothing but the squares round the capital is explored, and they are too near it.
        let mut g = game(COAST_CITY, "high_men");
        let units = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            "high_men_settlers",
            1,
        );
        let mut ai = Ai::new(P0);
        settlers(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        assert!(ai.settler_targets.is_empty());
        assert!(view.unit_has_order(units[0]), "it walks to unexplored land");
        Ok(())
    }

    #[test]
    fn the_new_city_is_named_from_the_races_list() -> TestResult {
        let (g, units) = island_with_settlers(1);
        let view = PlayerView::new(&g, P0);
        let race = view
            .content()
            .rules
            .races
            .by_name("high_men")
            .ok_or("high_men is shipped")?;
        let used = view.my_cities().len();
        assert_eq!(
            city_name(&view, units[0]),
            race.city_names[used % race.city_names.len()]
        );
        Ok(())
    }
}
