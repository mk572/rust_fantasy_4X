//! Scouts: one or more units whose job is to find the rest of the world (`docs/ai-design.md`
//! section 5.1).
//!
//! A scout walks to the reachable square from which it will see the most unexplored ground,
//! nearer squares first, and takes a new target when it arrives or stops. Scouts keep apart:
//! two never head for the same patch. When nothing reachable has unexplored ground near it the
//! scouts rejoin the army.

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::rules::Stat;

use crate::view::{distance, is_fighter, is_ship};
use crate::Ai;

/// How many squares of walking a square of new ground is worth.
const WALK_COST: f32 = 0.35;
/// Turns a scout may stand still on an order before its target is given up.
const STUCK_TURNS: u32 = 3;
/// Scouting targets this close to another scout's are left to it.
const KEEP_APART: u32 = 5;

/// How many fighters a sea scout carries to put ashore.
const PARTY: usize = 2;

/// How many scouts the wizard wants.
fn wanted(cities: usize) -> usize {
    let base = match cities {
        0 => 0,
        1..=3 => 1,
        _ => 2,
    };
    base + crate::knob("SCOUTS", 0.0) as usize
}

/// The same square on the other plane.
fn across(at: Position) -> Position {
    let plane = match at.plane {
        Plane::Arcanus => Plane::Myrror,
        Plane::Myrror => Plane::Arcanus,
    };
    Position { plane, ..at }
}

/// Whether the ground across a tower is still unexplored, so that crossing it shows something.
pub(crate) fn worth_crossing(view: &PlayerView, tower: Position) -> bool {
    unexplored_near(view, across(tower), 3) > 0
}

/// Make sure the wizard has enough scouts and send them out.
pub fn scout(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    planar_scout(ai, game, events);
    if ai.frontier_closed {
        return;
    }
    let (scouts, cities) = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        ai.scouts.retain(|s| alive.contains(s));
        ai.scout_targets.retain(|s, _| alive.contains(s));
        ai.sea_party.retain(|s| alive.contains(s));
        let mut scouts: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| ai.scouts.contains(&view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut scouts);
        (scouts, view.my_cities().len())
    };
    if scouts.len() < wanted(cities) {
        if let Some(unit) = recruit(ai, game, None) {
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
    let ship_out = {
        let view = PlayerView::new(game, me);
        scouts.iter().any(|&u| is_ship(&view, u))
    };
    for unit in scouts {
        let (serial, pos, idle) = {
            let view = PlayerView::new(game, me);
            let Some(pos) = view.unit_position(unit) else {
                continue;
            };
            (view.serial_of(unit), pos, !view.unit_has_order(unit))
        };
        // A fighter riding on a sea scout waits to be put ashore.
        if ai.sea_party.contains(&serial) || ai.planar == Some(serial) {
            found_any = true;
            continue;
        }
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
        let is_sea_scout = is_ship(&PlayerView::new(game, me), unit);
        if is_sea_scout && put_ashore(ai, game, pos, events) {
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
                let mut units = vec![unit];
                if is_sea_scout {
                    units.extend(board(ai, game, unit, pos));
                }
                ai.send(game, Command::GoTo { units, to }, events);
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
            // A ship lost on the way is built again; only a ship that found nothing ends it.
            ai.sea_done = ship_out;
            ai.sea_scouting = false;
        }
        let planar = ai.planar;
        ai.scouts.retain(|s| Some(*s) == planar);
        ai.sea_party.clear();
        ai.scout_targets.clear();
    }
}

/// The sea scout in a city takes spare fighters aboard, up to [`PARTY`] in all, to put ashore on
/// land nobody holds. The party sails with the ship (the order names them) and is left alone by
/// the rest of the scouting meanwhile. Returns the fighters taken aboard now, with the ones
/// already aboard.
fn board(ai: &mut Ai, game: &Game, ship: hecs::Entity, at: Position) -> Vec<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    let mut party: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| ai.sea_party.contains(&view.serial_of(u)) && view.unit_position(u) == Some(at))
        .collect();
    if view.city_at(at).is_none() {
        return party;
    }
    let room = (view.unit_stat(ship, Stat::Transport).max(0) as usize).min(PARTY);
    let mut spare: Vec<hecs::Entity> = view
        .garrison(at)
        .into_iter()
        .filter(|&u| {
            is_fighter(&view, u)
                && !ai.scouts.contains(&view.serial_of(u))
                && !ai.raiders.contains(&view.serial_of(u))
        })
        .collect();
    view.sort_by_serial(&mut spare);
    // The city keeps a fighter.
    let free = spare.len().saturating_sub(1);
    let taken: Vec<hecs::Entity> = spare
        .into_iter()
        .take(room.saturating_sub(party.len()).min(free))
        .collect();
    let serials: Vec<Serial> = taken.iter().map(|&u| view.serial_of(u)).collect();
    party.extend(taken);
    ai.scouts.extend(serials.iter().copied());
    ai.sea_party.extend(serials);
    party
}

/// Whether land nobody of the wizard's stands on, which still has unexplored ground at its edge,
/// is the land mass of the square: it is found by walking the explored land from it.
fn land_unclaimed(view: &PlayerView, start: Position) -> bool {
    use f4x_core::map::terrain::TerrainClass;
    use std::collections::{HashSet, VecDeque};
    let mine: HashSet<Position> = view
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .chain(
            view.my_units()
                .into_iter()
                .filter_map(|u| view.unit_position(u)),
        )
        .collect();
    let mut seen: HashSet<Position> = HashSet::from([start]);
    let mut queue: VecDeque<Position> = VecDeque::from([start]);
    let mut open_edge = false;
    while let Some(here) = queue.pop_front() {
        if mine.contains(&here) {
            return false;
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
    open_edge
}

/// A sea scout beside land nobody holds puts one of its party ashore to explore it. Returns
/// whether it did.
fn put_ashore(ai: &mut Ai, game: &mut Game, ship_at: Position, events: &mut Vec<Event>) -> bool {
    use f4x_core::map::terrain::TerrainClass;
    let me = ai.player;
    let (rider, shore) = {
        let view = PlayerView::new(game, me);
        let Some(rider) = view.my_units().into_iter().find(|&u| {
            ai.sea_party.contains(&view.serial_of(u)) && view.unit_position(u) == Some(ship_at)
        }) else {
            return false;
        };
        let shore = (-1i32..=1)
            .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
            .filter_map(|(dx, dy)| view.shift(ship_at, dx, dy))
            .filter(|&p| {
                view.terrain_class(p)
                    .is_some_and(|t| t != TerrainClass::Ocean)
                    && view.hostile_at(p).is_none()
                    && !view.has_lair(p)
                    && view.city_at(p).is_none()
                    && land_unclaimed(&view, p)
            })
            .find(|&p| view.can_reach(&[rider], p));
        (rider, shore)
    };
    let Some(shore) = shore else {
        return false;
    };
    let serial = PlayerView::new(game, me).serial_of(rider);
    if !ai.send(
        game,
        Command::GoTo {
            units: vec![rider],
            to: shore,
        },
        events,
    ) {
        return false;
    }
    ai.sea_party.retain(|s| *s != serial);
    true
}

/// A scout walks to a tower nobody guards any more and changes plane there, when the ground
/// across it is unexplored; the search for the rest of the world then goes on over there.
fn planar_scout(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let (tower, unit) = {
        let view = PlayerView::new(game, me);
        ai.planar = ai
            .planar
            .filter(|s| view.my_units().iter().any(|&u| view.serial_of(u) == *s));
        let tower = view
            .towers()
            .into_iter()
            .filter(|&(p, intact)| !intact && worth_crossing(&view, p))
            .map(|(p, _)| p)
            .min_by_key(|&p| (p.plane as u8, p.y, p.x));
        let Some(tower) = tower else {
            return;
        };
        let unit = ai.planar.and_then(|s| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        });
        (tower, unit)
    };
    let unit = match unit {
        Some(u) => u,
        None => {
            let Some(u) = recruit(ai, game, Some(tower)) else {
                return;
            };
            let serial = PlayerView::new(game, me).serial_of(u);
            ai.planar = Some(serial);
            if !ai.scouts.contains(&serial) {
                ai.scouts.push(serial);
            }
            u
        }
    };
    let (at, idle, shifts) = {
        let view = PlayerView::new(game, me);
        (
            view.unit_position(unit),
            !view.unit_has_order(unit),
            view.can_shift_plane(&[unit]),
        )
    };
    if at == Some(tower) {
        if shifts && ai.send(game, Command::ShiftPlane { units: vec![unit] }, events) {
            ai.planar = None;
            ai.frontier_closed = false;
            ai.bad_scout_targets.clear();
        }
    } else if idle {
        ai.send(
            game,
            Command::GoTo {
                units: vec![unit],
                to: tower,
            },
            events,
        );
    }
}

/// Take the unit that can walk furthest in a turn among those a city can spare, from the
/// fighters that are not settlers.
fn recruit(ai: &Ai, game: &Game, reach: Option<Position>) -> Option<hecs::Entity> {
    let view = PlayerView::new(game, ai.player);
    let mut best: Option<(i32, std::cmp::Reverse<Serial>, hecs::Entity)> = None;
    for u in view.my_units() {
        if !is_fighter(&view, u)
            || ai.scouts.contains(&view.serial_of(u))
            || ai.raiders.contains(&view.serial_of(u))
        {
            continue;
        }
        let Some(at) = view.unit_position(u) else {
            continue;
        };
        if reach.is_some_and(|to| !view.can_reach(&[u], to)) {
            continue;
        }
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
pub(crate) fn best_target(
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
    use crate::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

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
    fn the_best_target_is_new_ground_the_unit_can_walk_to() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let target =
            best_target(&view, walker, city(), &[]).ok_or("there is something to explore")?;
        assert_ne!(target, city());
        assert!(view.reachable(&[walker]).contains(&target));
        assert!(unexplored_near(&view, target, 3) > 0);
        // The same choice every time.
        assert_eq!(best_target(&view, walker, city(), &[]), Some(target));
        Ok(())
    }

    #[test]
    fn a_target_near_another_scouts_is_left_to_it() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let walker = spawn(&mut g, P0, city(), "high_men_spearmen", 1)[0];
        let view = PlayerView::new(&g, P0);
        let first = best_target(&view, walker, city(), &[]).ok_or("a first target")?;
        let second = best_target(&view, walker, city(), &[first]).ok_or("a second target")?;
        let apart = distance(&view, first, second).ok_or("the targets are on one plane")?;
        assert!(apart >= KEEP_APART, "{apart}");
        Ok(())
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
        assert_eq!(recruit(&ai, &g, None), Some(rider));
    }

    #[test]
    fn a_city_keeps_its_last_fighter() {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 1);
        assert_eq!(recruit(&Ai::new(P0), &g, None), None);
        // A lone fighter outside any city is free to go.
        let outside = spawn(&mut g, P0, at(5, 5), "high_men_spearmen", 1)[0];
        assert_eq!(recruit(&Ai::new(P0), &g, None), Some(outside));
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
    fn a_ship_scout_that_finds_nothing_ends_the_search_by_sea_but_a_lost_ship_does_not() {
        let mut g = game(COAST_CITY, "high_men");
        g.reveal_map(P0);
        let ship = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), "galley", 1)[0];
        let walker = spawn(&mut g, P0, at(5, 5), "high_men_spearmen", 1)[0];
        for (scout_unit, done) in [(ship, true), (walker, false)] {
            let mut ai = Ai::new(P0);
            ai.sea_scouting = true;
            ai.scouts
                .push(PlayerView::new(&g, P0).serial_of(scout_unit));
            scout(&mut ai, &mut g, &mut Vec::new());
            assert!(ai.frontier_closed);
            assert!(!ai.sea_scouting);
            assert_eq!(ai.sea_done, done);
        }
    }

    #[test]
    fn a_sea_scout_in_port_takes_spare_fighters_aboard_and_leaves_the_city_one() {
        let mut g = game(COAST_CITY, "high_men");
        let ship = spawn(&mut g, P0, city(), "galley", 1)[0];
        spawn(&mut g, P0, city(), "high_men_spearmen", 3);
        let mut ai = Ai::new(P0);
        ai.scouts.push(PlayerView::new(&g, P0).serial_of(ship));
        ai.sea_scouting = true;
        scout(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.sea_party.len(), PARTY);
        assert!(ai.sea_party.iter().all(|s| ai.scouts.contains(s)));
    }

    #[test]
    fn a_sea_scout_puts_a_fighter_ashore_on_land_nobody_holds() {
        let mut g = game(COAST_CITY, "high_men");
        // Off the east island's west shore, which the wizard has just seen.
        let sea = at(27, 9);
        let boat = spawn(&mut g, P0, sea, "galley", 1)[0];
        let walker = spawn(&mut g, P0, sea, "high_men_spearmen", 1)[0];
        g.explore(P0, sea, 3);
        let view = PlayerView::new(&g, P0);
        let mut ai = Ai::new(P0);
        ai.sea_party = vec![view.serial_of(walker)];
        ai.scouts = vec![view.serial_of(boat), view.serial_of(walker)];
        ai.sea_scouting = true;
        scout(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.sea_party.is_empty(), "the rider went ashore");
        let landed = g.units().unit_position(walker);
        assert!(landed.is_some_and(|p| p.x == 28), "{landed:?}");
    }

    #[test]
    fn a_sea_scout_does_not_land_on_its_own_island() {
        let mut g = game(COAST_CITY, "high_men");
        let sea = at(11, 8);
        let boat = spawn(&mut g, P0, sea, "galley", 1)[0];
        let walker = spawn(&mut g, P0, sea, "high_men_spearmen", 1)[0];
        g.explore(P0, sea, 3);
        let view = PlayerView::new(&g, P0);
        let mut ai = Ai::new(P0);
        ai.sea_party = vec![view.serial_of(walker)];
        ai.scouts = vec![view.serial_of(boat), view.serial_of(walker)];
        ai.sea_scouting = true;
        scout(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.sea_party.len(), 1);
    }

    #[test]
    fn a_scout_walks_to_a_tower_nobody_guards_and_changes_plane_there() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        let tower = at(8, 6);
        spawn_tower(&mut g, tower, false)?;
        g.explore(P0, tower, 3);
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        let serial = ai.planar.ok_or("a scout is sent to the tower")?;
        let unit = PlayerView::new(&g, P0)
            .my_units()
            .into_iter()
            .find(|&u| PlayerView::new(&g, P0).serial_of(u) == serial)
            .ok_or("the scout lives")?;
        assert!(PlayerView::new(&g, P0).unit_has_order(unit));
        // Standing on the tower, it goes across, and the search is open again over there.
        ai.frontier_closed = true;
        let spare = spawn(&mut g, P0, tower, "high_men_spearmen", 1)[0];
        ai.planar = Some(PlayerView::new(&g, P0).serial_of(spare));
        scout(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(
            g.units().unit_position(spare).map(|p| p.plane),
            Some(Plane::Myrror)
        );
        assert!(!ai.frontier_closed);
        assert_eq!(ai.planar, None);
        Ok(())
    }

    #[test]
    fn a_guarded_tower_is_not_a_way_across() -> TestResult {
        let mut g = game(COAST_CITY, "high_men");
        spawn_tower(&mut g, at(8, 6), true)?;
        g.explore(P0, at(8, 6), 3);
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.planar, None);
        Ok(())
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

    #[test]
    fn a_scout_that_stands_still_on_its_order_gives_the_target_up_and_chooses_another() -> TestResult
    {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, city(), "high_men_spearmen", 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut g, &mut Vec::new());
        let (&serial, &first) = ai
            .scout_targets
            .iter()
            .next()
            .ok_or("the scout has a target")?;
        // No turn passes, so the scout has no moves left: each look finds it where it was.
        for _ in 0..=STUCK_TURNS {
            scout(&mut ai, &mut g, &mut Vec::new());
        }
        assert!(
            ai.bad_scout_targets.contains(&first),
            "{:?}",
            ai.bad_scout_targets
        );
        assert_ne!(ai.scout_targets.get(&serial), Some(&first));
        Ok(())
    }
}
