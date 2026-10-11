//! The army: keep cities garrisoned, hit enemies that are weaker, gather a force and march
//! it on the nearest enemy city, and send spare units to explore.

use std::collections::HashMap;

use f4x_core::city::Position;
use f4x_core::{Command, Event, Game};

use crate::view::{distance, is_fighter, stack_strength, unit_strength, Sight};
use crate::Ai;

/// Fighters each city keeps at home.
const GARRISON: usize = 2;
/// The AI attacks when its force is this much stronger than what it expects to meet.
const ATTACK_RATIO: f32 = 1.4;
/// An attack out of a city's garrison needs a much bigger edge.
const SORTIE_RATIO: f32 = 2.5;
/// Strength assumed for a defence the AI cannot see.
const GUESS_DEFENDERS: f32 = 3.0;

const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn neighbours(game: &Game, at: Position) -> Vec<Position> {
    let Some(map) = game.map.as_ref() else {
        return Vec::new();
    };
    let grid = map.plane(at.plane);
    NEIGHBOURS
        .iter()
        .filter_map(|&(dx, dy)| {
            let y = at.y as i32 + dy;
            (y >= 0 && y < grid.height() as i32).then(|| Position {
                plane: at.plane,
                x: grid.wrap_x(at.x as i32 + dx),
                y: y as u32,
            })
        })
        .collect()
}

/// My fighting units, grouped by square, in a stable order.
fn stacks(game: &Game, ai: &Ai) -> Vec<(Position, Vec<hecs::Entity>)> {
    let mut by_pos: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
    for u in game.player_units(ai.player) {
        if let (true, Some(p)) = (is_fighter(game, u), game.unit_position(u)) {
            by_pos.entry(p).or_default().push(u);
        }
    }
    let mut list: Vec<_> = by_pos.into_iter().collect();
    for (_, units) in &mut list {
        game.sort_by_serial(units);
    }
    list.sort_by_key(|(_, units)| game.serial_of(units[0]));
    list
}

pub fn army(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let sight = Sight::of(game, me);
    let own_cities: Vec<Position> = game
        .cities()
        .into_iter()
        .filter(|(_, c, _)| c.owner == me)
        .map(|(_, _, p)| p)
        .collect();

    // 1. Hit what stands next to us, when we are clearly the stronger.
    let mut spent: Vec<hecs::Entity> = Vec::new();
    for (pos, units) in stacks(game, ai) {
        let at_city = own_cities.contains(&pos);
        let ratio = if at_city { SORTIE_RATIO } else { ATTACK_RATIO };
        let mine = stack_strength(game, &units);
        for target in neighbours(game, pos) {
            if game.game_over.is_some() {
                return;
            }
            let Some(defenders) = game.hostile_at(target, me) else {
                continue;
            };
            if defenders.is_empty() {
                // An empty enemy city: walk in.
                if ai.send(
                    game,
                    Command::MoveStack {
                        units: units.clone(),
                        to: target,
                    },
                    events,
                ) {
                    spent.extend(&units);
                    break;
                }
                continue;
            }
            let seen = sight.sees(target);
            let theirs = if seen {
                stack_strength(game, &defenders)
                    * if game.city_at(target).is_some() {
                        1.3
                    } else {
                        1.0
                    }
            } else {
                GUESS_DEFENDERS * average_strength(game, me)
            };
            if mine >= theirs * ratio && (!at_city || units.len() > 1) {
                let attackers = if at_city {
                    // Leave the weakest at home.
                    units.iter().skip(1).copied().collect()
                } else {
                    units.clone()
                };
                ai.send(
                    game,
                    Command::AutoResolve {
                        attackers,
                        defenders,
                    },
                    events,
                );
                spent.extend(&units);
                break;
            }
        }
    }

    // 2. Everyone who is neither at home guarding nor fighting is free to campaign.
    let mut free: Vec<hecs::Entity> = Vec::new();
    for (pos, units) in stacks(game, ai) {
        let skip = if own_cities.contains(&pos) {
            GARRISON
        } else {
            0
        };
        free.extend(
            units
                .into_iter()
                .skip(skip)
                .filter(|u| !spent.contains(u))
                .filter(|&u| game.unit_position(u).is_some()),
        );
    }
    let capital = game
        .player(me)
        .and_then(|p| p.fortress)
        .or_else(|| own_cities.first().copied());
    let Some(capital) = capital else {
        return;
    };

    // 3. Choose (or keep) a city to attack.
    if ai.campaign.is_some_and(|c| {
        game.city_at(c)
            .and_then(|e| game.city_owner(e))
            .is_none_or(|o| o == me)
    }) {
        ai.campaign = None;
    }
    if ai.campaign.is_none() {
        ai.campaign = nearest_enemy_city(game, me, capital);
    }
    let Some(target) = ai.campaign else {
        explore(ai, game, &free, events);
        return;
    };

    // Gather at the own city nearest the target; march when strong enough.
    let rally = own_cities
        .iter()
        .copied()
        .filter(|&c| distance(game, c, target).is_some())
        .min_by_key(|&c| distance(game, c, target))
        .unwrap_or(capital);
    let expected = if sight.sees(target) {
        let defenders = game.hostile_at(target, me).unwrap_or_default();
        stack_strength(game, &defenders) * 1.3
    } else {
        GUESS_DEFENDERS * average_strength(game, me)
    };
    let (at_rally, elsewhere): (Vec<_>, Vec<_>) = free
        .iter()
        .copied()
        .partition(|&u| game.unit_position(u) == Some(rally));
    let force = stack_strength(game, &at_rally);
    for u in &elsewhere {
        if game.world.get::<&f4x_core::units::Order>(*u).is_err()
            && game
                .unit_position(*u)
                .is_some_and(|p| p.plane == rally.plane)
        {
            // Units on their way to the target (further from the rally than from it) go on;
            // the rest come home to gather.
            ai.send(
                game,
                Command::GoTo {
                    units: vec![*u],
                    to: rally,
                },
                events,
            );
        }
    }
    if force >= expected * ATTACK_RATIO + 1.0 && at_rally.len() >= 3 {
        march(ai, game, &at_rally, target, events);
    }
}

fn average_strength(game: &Game, me: f4x_core::city::PlayerId) -> f32 {
    let units: Vec<_> = game
        .player_units(me)
        .into_iter()
        .filter(|&u| is_fighter(game, u))
        .collect();
    if units.is_empty() {
        return 20.0;
    }
    units.iter().map(|&u| unit_strength(game, u)).sum::<f32>() / units.len() as f32
}

/// The nearest city of another wizard on a square this wizard has explored.
fn nearest_enemy_city(
    game: &Game,
    me: f4x_core::city::PlayerId,
    from: Position,
) -> Option<Position> {
    game.cities()
        .into_iter()
        .filter(|(_, c, p)| c.owner != me && game.fog.is_explored(me, p.plane, p.x, p.y))
        .filter_map(|(_, _, p)| distance(game, from, p).map(|d| (d, p)))
        .min_by_key(|&(d, p)| (d, p.y, p.x))
        .map(|(_, p)| p)
}

/// Send a stack towards a square next to the target; it attacks from there next turn.
fn march(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    target: Position,
    events: &mut Vec<Event>,
) {
    let mut spots = neighbours(game, target);
    spots.sort_by_key(|&s| distance(game, s, game.unit_position(units[0]).unwrap_or(target)));
    for spot in spots {
        if game.hostile_at(spot, ai.player).is_some() || game.find_path(units, spot).is_none() {
            continue;
        }
        if ai.send(
            game,
            Command::GoTo {
                units: units.to_vec(),
                to: spot,
            },
            events,
        ) {
            return;
        }
    }
}

/// Spare units walk to the nearest square nobody has seen.
fn explore(ai: &mut Ai, game: &mut Game, free: &[hecs::Entity], events: &mut Vec<Event>) {
    let me = ai.player;
    for &unit in free.iter().take(2) {
        if game.world.get::<&f4x_core::units::Order>(unit).is_ok() {
            continue;
        }
        let Some(from) = game.unit_position(unit) else {
            continue;
        };
        let Some(map) = game.map.as_ref() else {
            return;
        };
        let grid = map.plane(from.plane);
        let mut best: Option<(u32, Position)> = None;
        for radius in 3i32..=14 {
            for dy in (-radius..=radius).step_by(2) {
                for dx in (-radius..=radius).step_by(2) {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let y = from.y as i32 + dy;
                    if y < 0 || y >= grid.height() as i32 {
                        continue;
                    }
                    let p = Position {
                        plane: from.plane,
                        x: grid.wrap_x(from.x as i32 + dx),
                        y: y as u32,
                    };
                    if !game.fog.is_explored(me, p.plane, p.x, p.y)
                        && best.is_none_or(|(d, _)| radius as u32 <= d)
                        && game.find_path(&[unit], p).is_some()
                    {
                        best = Some((radius as u32, p));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        if let Some((_, to)) = best {
            ai.send(
                game,
                Command::GoTo {
                    units: vec![unit],
                    to,
                },
                events,
            );
        }
    }
}
