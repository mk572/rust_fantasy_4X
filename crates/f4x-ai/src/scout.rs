//! Scouts: one or more units whose job is to find the rest of the world (`docs/ai-design.md`
//! section 5.1). Without them the wizards never meet, so there are no wars.
//!
//! A scout walks to the reachable square from which it will see the most unexplored ground,
//! nearer squares first, and takes a new target when it arrives or stops. Scouts keep apart:
//! two never head for the same patch. When nothing reachable has unexplored ground near it the
//! scouts rejoin the army.

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;

use crate::view::{distance, is_fighter};
use crate::Ai;

/// How many squares of walking a square of new ground is worth.
const WALK_COST: f32 = 0.35;
/// Turns a scout may stand still on an order before its target is given up.
const STUCK_TURNS: u32 = 3;
/// Scouting targets this close to another scout's are left to it.
const KEEP_APART: u32 = 5;

/// How many scouts the wizard wants.
fn wanted(cities: usize) -> usize {
    match cities {
        0 => 0,
        1..=3 => 1,
        _ => 2,
    }
}

/// Make sure the wizard has enough scouts and send them out.
pub fn scout(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    if ai.frontier_closed {
        return;
    }
    let (scouts, cities) = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.scouts.retain(|s| alive.contains(s));
        ai.scout_targets.retain(|s, _| alive.contains(s));
        let mut scouts: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| ai.scouts.contains(&view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut scouts);
        (scouts, view.my_cities().len())
    };
    if scouts.len() < wanted(cities) {
        if let Some(unit) = recruit(ai, game) {
            ai.scouts.push(PlayerView::new(game, me).serial_of(unit));
        }
    }
    let scouts: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut list: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| ai.scouts.contains(&view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut list);
        list
    };
    let mut found_any = scouts.is_empty();
    for unit in scouts {
        let (serial, pos, idle) = {
            let view = PlayerView::new(game, me);
            let Some(pos) = view.unit_position(unit) else {
                continue;
            };
            (view.serial_of(unit), pos, !view.unit_has_order(unit))
        };
        // A scout that has not moved for a few turns although it has an order is stuck: give
        // its target up (a guarded or blocked patch) and choose another.
        let stuck = {
            let entry = ai.scout_last.entry(serial).or_insert((pos, 0));
            if entry.0 == pos && !idle {
                entry.1 += 1;
            } else {
                *entry = (pos, 0);
            }
            entry.1 >= STUCK_TURNS
        };
        if stuck {
            if let Some(t) = ai.scout_targets.remove(&serial) {
                ai.bad_scout_targets.push(t);
            }
            ai.scout_last.remove(&serial);
            ai.send(game, Command::ClearOrders { units: vec![unit] }, events);
        } else if !idle {
            found_any = true;
            continue;
        }
        let claimed: Vec<Position> = ai
            .scout_targets
            .iter()
            .filter(|(s, _)| **s != serial)
            .map(|(_, p)| *p)
            .chain(ai.bad_scout_targets.iter().copied())
            .collect();
        let target = {
            let view = PlayerView::new(game, me);
            best_target(&view, unit, pos, &claimed)
        };
        match target {
            Some(to) => {
                found_any = true;
                ai.scout_targets.insert(serial, to);
                ai.send(
                    game,
                    Command::GoTo {
                        units: vec![unit],
                        to,
                    },
                    events,
                );
            }
            None => {
                ai.scout_targets.remove(&serial);
            }
        }
    }
    if !found_any {
        // Nothing reachable shows anything new: the rest of the map needs ships or a gate.
        ai.frontier_closed = true;
        if ai.sea_scouting {
            ai.sea_done = true;
            ai.sea_scouting = false;
        }
        ai.scouts.clear();
        ai.scout_targets.clear();
    }
}

/// Take the unit that can walk furthest in a turn among those a city can spare, from the
/// fighters that are not settlers.
fn recruit(ai: &Ai, game: &Game) -> Option<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    let mut best: Option<(i32, std::cmp::Reverse<Serial>, hecs::Entity)> = None;
    for u in view.my_units() {
        if !is_fighter(&view, u) || ai.scouts.contains(&view.serial_of(u)) {
            continue;
        }
        let Some(at) = view.unit_position(u) else {
            continue;
        };
        // A city keeps one fighter at least: only take from a stack of two or more, or from
        // a unit that stands outside a city.
        let in_city = view.city_at(at).is_some();
        if in_city && view.garrison(at).len() < 2 {
            continue;
        }
        let key = (
            view.unit_stat(u, Stat::Moves) * 2 + i32::from(view.unit_has_ability(u, "flying")) * 2,
            std::cmp::Reverse(view.serial_of(u)),
            u,
        );
        if best.as_ref().is_none_or(|b| (key.0, &key.1) > (b.0, &b.1)) {
            best = Some(key);
        }
    }
    best.map(|b| b.2)
}

/// The reachable square worth most to this scout: new ground within sight of it, less the
/// walk, skipping the patches other scouts are heading for.
fn best_target(
    view: &PlayerView,
    unit: hecs::Entity,
    from: Position,
    claimed: &[Position],
) -> Option<Position> {
    let reachable = view.reachable(&[unit]);
    // A unit shows the squares within its sight of where it stands, and no more.
    let look = view.unit_stat(unit, Stat::Sight).max(3);
    let mut best: Option<(f32, Position)> = None;
    for &p in &reachable {
        if p == from
            || claimed
                .iter()
                .any(|&c| distance(view, c, p).is_some_and(|d| d < KEEP_APART))
        {
            continue;
        }
        let fresh = unexplored_near(view, p, look);
        if fresh == 0 {
            continue;
        }
        let walk = distance(view, from, p).unwrap_or(99) as f32;
        let value = fresh as f32 - walk * WALK_COST * 4.0;
        // A strict comparison with the position as a tie-break keeps the choice deterministic
        // whatever order the set is walked in.
        let better = match best {
            None => true,
            Some((v, b)) => value > v || (value == v && (p.y, p.x) < (b.y, b.x)),
        };
        if better {
            best = Some((value, p));
        }
    }
    best.map(|(_, p)| p)
}

/// How many squares within the sight `look` (half-squares) of `at` the wizard has not explored.
fn unexplored_near(view: &PlayerView, at: Position, look: i32) -> u32 {
    let mut n = 0;
    let reach = f4x_core::map::fog::squares_seen(look as u32) as i32;
    for dy in -reach..=reach {
        for dx in (-reach..=reach)
            .filter(|&dx| f4x_core::map::fog::in_sight(dx, dy, look, view.diagonal_extra_cost()))
        {
            if let Some(p) = view.shift(at, dx, dy) {
                if !view.explored(p) {
                    n += 1;
                }
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    fn city() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    #[test]
    fn the_number_of_scouts_grows_with_the_cities() {
        assert_eq!(wanted(0), 0);
        assert_eq!(wanted(1), 1);
        assert_eq!(wanted(3), 1);
        assert_eq!(wanted(4), 2);
        assert_eq!(wanted(12), 2);
    }

    #[test]
    fn unexplored_ground_is_counted_within_the_look_and_vanishes_when_seen() {
        let mut g = game(COAST_CITY, "high_men");
        let far = at(4, 4);
        let view = PlayerView::new(&g, P0);
        assert_eq!(
            unexplored_near(&view, far, 5),
            25,
            "all 25 squares of a look of 5 half-squares unseen"
        );
        // The city's own surroundings are known.
        assert_eq!(unexplored_near(&view, city(), 1), 0);
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        assert_eq!(unexplored_near(&view, far, 5), 0);
    }

    #[test]
    fn the_best_target_is_new_ground_the_unit_can_walk_to() {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let target = best_target(&view, walker, city(), &[]).expect("something to explore");
        assert_ne!(target, city());
        assert!(view.reachable(&[walker]).contains(&target));
        assert!(unexplored_near(&view, target, 3) > 0);
        // The same choice every time.
        assert_eq!(best_target(&view, walker, city(), &[]), Some(target));
    }

    #[test]
    fn a_target_near_another_scouts_is_left_to_it() {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let first = best_target(&view, walker, city(), &[]).unwrap();
        let second = best_target(&view, walker, city(), &[first]);
        if let Some(second) = second {
            assert!(distance(&view, first, second).unwrap() >= KEEP_APART);
        }
    }

    #[test]
    fn nothing_is_a_target_once_the_whole_map_is_explored() {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        g.reveal_map(P0);
        let view = PlayerView::new(&g, P0);
        assert_eq!(best_target(&view, walker, city(), &[]), None);
    }

    #[test]
    fn the_fastest_spare_fighter_becomes_the_scout() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let rider = spawn(&mut g, P0, city(), "high_men_cavalry", 1)[0];
        let ai = Ai::new(P0);
        assert_eq!(recruit(&ai, &g), Some(rider));
    }

    #[test]
    fn a_city_keeps_its_last_fighter() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 1);
        assert_eq!(recruit(&Ai::new(P0), &g), None);
        // A lone fighter outside any city is free to go.
        let outside = spawn(&mut g, P0, at(5, 5), "high_men_spearmen", 1)[0];
        assert_eq!(recruit(&Ai::new(P0), &g), Some(outside));
    }

    #[test]
    fn a_scout_is_recruited_and_sent_to_new_ground() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.scouts.len(), 1);
        assert_eq!(ai.scout_targets.len(), 1);
        assert!(!ai.frontier_closed);
    }

    #[test]
    fn with_no_spare_fighter_nobody_scouts_and_the_search_stays_open() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 1);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.scouts.is_empty());
        assert!(!ai.frontier_closed);
    }

    #[test]
    fn when_nothing_new_can_be_reached_scouting_by_land_is_over() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.frontier_closed);
        assert!(ai.scouts.is_empty());
        assert!(ai.scout_targets.is_empty());
    }

    #[test]
    fn a_closed_frontier_is_left_alone() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        scout(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.scouts.is_empty());
    }
}
