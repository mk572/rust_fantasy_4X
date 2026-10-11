//! Settlers: choose a place for the next city, walk there and found it.

use f4x_core::city::Position;
use f4x_core::content::TerrainClass;
use f4x_core::map::Plane;
use f4x_core::{Command, Event, Game};

use crate::view::distance;
use crate::Ai;

/// How far from the settler the AI looks for a site.
const SEARCH_RADIUS: i32 = 9;
/// Sites keep this far apart from each other and from cities.
const SPACING: u32 = 4;

pub fn settlers(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let mut mine: Vec<hecs::Entity> = game
        .player_units(me)
        .into_iter()
        .filter(|&u| game.unit_has_ability(u, "create_outpost"))
        .collect();
    game.sort_by_serial(&mut mine);
    // Forget the targets of settlers that are gone.
    let alive: Vec<_> = mine.iter().map(|&u| game.serial_of(u)).collect();
    ai.settler_targets.retain(|s, _| alive.contains(s));

    for unit in mine {
        let Some(pos) = game.unit_position(unit) else {
            continue;
        };
        let serial = game.serial_of(unit);
        if let Some(&target) = ai.settler_targets.get(&serial) {
            if target == pos {
                if game.can_found_city(pos).is_ok() {
                    let name = city_name(game, unit);
                    if ai.send(game, Command::SettleOutpost { unit, name }, events) {
                        ai.settler_targets.remove(&serial);
                        continue;
                    }
                }
                ai.settler_targets.remove(&serial); // blocked: choose again below
            } else if game.world.get::<&f4x_core::units::Order>(unit).is_ok() {
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
            match choose_site(game, unit, pos, &avoid) {
                Some(site) => {
                    ai.settler_targets.insert(serial, site);
                }
                None => continue,
            }
        }
        let target = ai.settler_targets[&serial];
        if target == pos {
            // Found it right here (next turn if the settlers just arrived and cannot).
            let name = city_name(game, unit);
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

fn city_name(game: &Game, unit: hecs::Entity) -> String {
    let used = game.cities().len();
    game.unit_def(unit)
        .and_then(|d| d.race.as_deref())
        .and_then(|r| game.content.rules.races.by_name(r))
        .and_then(|r| r.city_names.get(used % r.city_names.len().max(1)).cloned())
        .unwrap_or_else(|| format!("Outpost {}", used + 1))
}

/// How good a city here would be: the land it can work.
fn site_value(game: &Game, at: Position) -> f32 {
    let Some(map) = game.map.as_ref() else {
        return 0.0;
    };
    let grid = map.plane(at.plane);
    let mut value = 0.0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            if dx.abs() == 2 && dy.abs() == 2 {
                continue;
            }
            let y = at.y as i32 + dy;
            if y < 0 || y >= grid.height() as i32 {
                continue;
            }
            let p = Position {
                plane: at.plane,
                x: grid.wrap_x(at.x as i32 + dx),
                y: y as u32,
            };
            value += match game.terrain_class_at(p) {
                Some(TerrainClass::Grassland) => 3.0,
                Some(TerrainClass::Forest) | Some(TerrainClass::Hills) => 2.0,
                Some(TerrainClass::Swamp) | Some(TerrainClass::Mountains) => 1.0,
                Some(TerrainClass::Desert) | Some(TerrainClass::Tundra) => 0.5,
                Some(TerrainClass::Ocean) => 0.7,
                None => 0.0,
            };
            if grid.has_river(p.x as i32, p.y as i32) {
                value += 0.5;
            }
        }
    }
    value
}

fn choose_site(
    game: &Game,
    unit: hecs::Entity,
    from: Position,
    taken: &[Position],
) -> Option<Position> {
    let map = game.map.as_ref()?;
    let grid = map.plane(from.plane);
    let mut candidates: Vec<(f32, Position)> = Vec::new();
    for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
        let y = from.y as i32 + dy;
        if y < 1 || y >= grid.height() as i32 - 1 {
            continue;
        }
        for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
            let p = Position {
                plane: from.plane,
                x: grid.wrap_x(from.x as i32 + dx),
                y: y as u32,
            };
            if game.can_found_city(p).is_err()
                || game.lair_at(p).is_some()
                || game.map.as_ref().is_some_and(|m| m.node_covering(p.plane, p.x, p.y).is_some())
                || taken
                    .iter()
                    .any(|&t| distance(game, t, p).is_some_and(|d| d < SPACING))
            {
                continue;
            }
            let walk = dx.abs().max(dy.abs()) as f32;
            candidates.push((site_value(game, p) - walk * 0.7, p));
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let _ = Plane::Arcanus;
    candidates
        .into_iter()
        .take(10)
        .map(|(_, p)| p)
        .find(|&p| p == from || game.find_path(&[unit], p).is_some())
}
