//! Settlers: choose a place for the next city, walk there and found it.

use f4x_core::city::Position;
use f4x_core::content::TerrainClass;
use f4x_core::player_view::PlayerView;
use f4x_core::{Command, Event, Game};

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
                Some(TerrainClass::Grassland) => 3.0,
                Some(TerrainClass::Forest) | Some(TerrainClass::Hills) => 2.0,
                Some(TerrainClass::Swamp) | Some(TerrainClass::Mountains) => 1.0,
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
