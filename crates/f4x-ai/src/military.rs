//! The army: keep cities garrisoned, hit enemies that are weaker, gather a force and march
//! it on the nearest enemy city, and send spare units to explore.

use std::collections::HashMap;

use f4x_core::city::{PlayerId, Position};
use f4x_core::player_view::PlayerView;
use f4x_core::{Command, Event, Game};

use crate::view::{distance, is_fighter, nearest_unexplored, stack_strength, unit_strength};
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

fn neighbours(view: &PlayerView, at: Position) -> Vec<Position> {
    NEIGHBOURS
        .iter()
        .filter_map(|&(dx, dy)| view.shift(at, dx, dy))
        .collect()
}

/// My fighting units, grouped by square, in a stable order.
fn stacks(view: &PlayerView) -> Vec<(Position, Vec<hecs::Entity>)> {
    let mut by_pos: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
    for u in view.my_units() {
        if let (true, Some(p)) = (is_fighter(view, u), view.unit_position(u)) {
            by_pos.entry(p).or_default().push(u);
        }
    }
    let mut list: Vec<_> = by_pos.into_iter().collect();
    for (_, units) in &mut list {
        view.sort_by_serial(units);
    }
    list.sort_by_key(|(_, units)| view.serial_of(units[0]));
    list
}

/// What a stack decides about one square next to it.
enum Sortie {
    /// Nothing hostile there.
    Nothing,
    /// An enemy city with no one in it: walk in.
    WalkIn,
    /// Defended: fight if this force is enough.
    Fight(Vec<hecs::Entity>, f32),
}

pub fn army(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let own_cities: Vec<Position> = PlayerView::new(game, me)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .collect();

    // 1. Hit what stands next to us, when we are clearly the stronger.
    let mut spent: Vec<hecs::Entity> = Vec::new();
    let stack_list = stacks(&PlayerView::new(game, me));
    for (pos, units) in stack_list {
        let at_city = own_cities.contains(&pos);
        let ratio = if at_city { SORTIE_RATIO } else { ATTACK_RATIO };
        let (mine, targets) = {
            let view = PlayerView::new(game, me);
            (stack_strength(&view, &units), neighbours(&view, pos))
        };
        for target in targets {
            let sortie = {
                let view = PlayerView::new(game, me);
                if view.game_over() {
                    return;
                }
                match view.hostile_at(target) {
                    None => Sortie::Nothing,
                    Some(defenders) if defenders.is_empty() => Sortie::WalkIn,
                    Some(defenders) => {
                        // A square we see is a square we know the defence of.
                        let theirs = stack_strength(&view, &defenders)
                            * if view.city_at(target).is_some() {
                                1.3
                            } else {
                                1.0
                            };
                        Sortie::Fight(defenders, theirs)
                    }
                }
            };
            match sortie {
                Sortie::Nothing => continue,
                Sortie::WalkIn => {
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
                }
                Sortie::Fight(defenders, theirs) => {
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
        }
    }

    // 2. Everyone who is neither at home guarding nor fighting is free to campaign.
    let (free, capital) = {
        let view = PlayerView::new(game, me);
        let mut free: Vec<hecs::Entity> = Vec::new();
        for (pos, units) in stacks(&view) {
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
                    .filter(|&u| view.unit_position(u).is_some()),
            );
        }
        let capital = view
            .player()
            .and_then(|p| p.fortress)
            .or_else(|| own_cities.first().copied());
        (free, capital)
    };
    let Some(capital) = capital else {
        return;
    };

    // 3. Choose (or keep) a city to attack.
    {
        let view = PlayerView::new(game, me);
        if ai.campaign.is_some_and(|c| {
            view.city_at(c)
                .and_then(|e| view.city_owner(e))
                .is_none_or(|o| o == me)
        }) {
            ai.campaign = None;
        }
        if ai.campaign.is_none() {
            ai.campaign = nearest_enemy_city(&view, capital);
        }
    }
    let Some(target) = ai.campaign else {
        explore(ai, game, &free, events);
        return;
    };

    // Gather at the own city nearest the target; march when strong enough.
    let (rally, expected, at_rally, elsewhere, force) = {
        let view = PlayerView::new(game, me);
        let rally = own_cities
            .iter()
            .copied()
            .filter(|&c| distance(&view, c, target).is_some())
            .min_by_key(|&c| distance(&view, c, target))
            .unwrap_or(capital);
        let expected = if view.sees(target) {
            let defenders = view.hostile_at(target).unwrap_or_default();
            stack_strength(&view, &defenders) * 1.3
        } else {
            GUESS_DEFENDERS * average_strength(&view)
        };
        let (at_rally, elsewhere): (Vec<_>, Vec<_>) = free
            .iter()
            .copied()
            .partition(|&u| view.unit_position(u) == Some(rally));
        let force = stack_strength(&view, &at_rally);
        (rally, expected, at_rally, elsewhere, force)
    };
    for u in &elsewhere {
        let comes_home = {
            let view = PlayerView::new(game, me);
            !view.unit_has_order(*u)
                && view
                    .unit_position(*u)
                    .is_some_and(|p| p.plane == rally.plane)
        };
        if comes_home {
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

fn average_strength(view: &PlayerView) -> f32 {
    let units: Vec<_> = view
        .my_units()
        .into_iter()
        .filter(|&u| is_fighter(view, u))
        .collect();
    if units.is_empty() {
        return 20.0;
    }
    units.iter().map(|&u| unit_strength(view, u)).sum::<f32>() / units.len() as f32
}

/// The nearest city of another wizard on a square this wizard has explored.
fn nearest_enemy_city(view: &PlayerView, from: Position) -> Option<Position> {
    view.known_cities()
        .into_iter()
        .filter_map(|(_, _, p)| distance(view, from, p).map(|d| (d, p)))
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
    let spots = {
        let view = PlayerView::new(game, ai.player);
        let mut spots = neighbours(&view, target);
        spots.sort_by_key(|&s| distance(&view, s, view.unit_position(units[0]).unwrap_or(target)));
        spots
    };
    for spot in spots {
        {
            let view = PlayerView::new(game, ai.player);
            if view.hostile_at(spot).is_some() || !view.can_reach(units, spot) {
                continue;
            }
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
    let me: PlayerId = ai.player;
    for &unit in free.iter().take(2) {
        let to = {
            let view = PlayerView::new(game, me);
            if view.unit_has_order(unit) {
                continue;
            }
            nearest_unexplored(&view, unit)
        };
        if let Some(to) = to {
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
