//! Scouts: one or more units whose job is to find the rest of the world (`docs/handler-design.md`
//! section 5.1).
//!
//! A scout walks to the reachable square from which it will see the most unexplored ground,
//! nearer squares first, and takes a new target when it arrives or stops. Scouts keep apart:
//! two never head for the same patch. When nothing reachable has unexplored ground near it the
//! scouts rejoin the army.

use crate::map::{Plane, Position};
use crate::player::view::PlayerView;
use crate::world::serial::Serial;
use crate::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::Stat;

use super::view::{distance, is_fighter, is_ship};
use super::Handler;
use super::Host;

/// How many squares of walking a square of new ground is worth.
const WALK_COST: f32 = 0.35;
/// Turns a scout may stand still on an order before its target is given up.
const STUCK_TURNS: u32 = 3;
/// Scouting targets this close to another scout's are left to it.
const KEEP_APART: u32 = 5;

/// Added to the value of a scouting square near a town to look at (squares of new ground are worth
/// one each).
const RECON_BONUS: f32 = 20.0;
/// How near a scouting square must be to such a town to get the bonus.
const RECON_NEAR: u32 = 3;
/// How many fighters a sea scout carries to put ashore.
const PARTY: usize = 2;

/// A plane explored less than this is still blind to the wizard.
const BLIND_SHARE: f32 = 0.6;

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
    unexplored_near(view, across(tower), HalfInt::from_raw(3)) > 0
}

/// Past the build-up, a plane the wizard has hardly seen and has no unit on is worth a crossing
/// at any open tower, whatever the ground at the tower: the rivals that live over there are
/// found only by looking (task 502, a rival on the other plane is never met otherwise).
fn blind_across(handler: &Handler, view: &PlayerView, tower: Position, round: u32) -> bool {
    let far = across(tower).plane;
    !handler.judge.building_up(view, round)
        && !view
            .my_units()
            .into_iter()
            .any(|u| view.unit_position(u).is_some_and(|p| p.plane == far))
        && view.explored_share(far) < BLIND_SHARE
}

/// Make sure the wizard has enough scouts and send them out.
pub fn scout(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    let me = handler.player;
    planar_scout(handler, game, events);
    if handler.frontier_closed {
        return;
    }
    let (scouts, wanted) = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        handler.roles.prune(&alive);
        let mut scouts: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| handler.roles.is_scouting(view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut scouts);
        (scouts, handler.judge.scouts_wanted(&view, game.round()))
    };
    if scouts.len() < wanted {
        if let Some(unit) = recruit(handler, game, None) {
            let serial = PlayerView::new(game, me).serial_of(unit);
            let _ = handler
                .roles
                .claim(serial, super::roles::Role::Scout, game.round(), None);
        }
    }
    let scouts: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut list: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| handler.roles.is_scouting(view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut list);
        list
    };
    let ship_out = {
        let view = PlayerView::new(game, me);
        scouts.iter().any(|&u| is_ship(&view, u))
    };
    // A rider whose ship no longer scouts (sunk, or given another task) waits for nothing: it is
    // free, and does not count as a scout at work.
    let scouts: Vec<hecs::Entity> = if ship_out {
        scouts
    } else {
        handler.roles.release_role(super::roles::Role::SeaParty);
        let view = PlayerView::new(game, me);
        scouts
            .into_iter()
            .filter(|&u| handler.roles.is_scouting(view.serial_of(u)))
            .collect()
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
        // A fighter riding on a sea scout waits to be put ashore.
        if handler.roles.is(serial, super::roles::Role::SeaParty)
            || handler.roles.is(serial, super::roles::Role::PlanarScout)
        {
            found_any = true;
            continue;
        }
        // A scout that has not moved for a few turns although it has an order is stuck: give
        // its target up (a guarded or blocked patch) and choose another.
        let stuck = handler.roles.track(serial, pos, !idle) >= STUCK_TURNS;
        if stuck {
            if let Some(t) = handler.roles.target_of(serial, super::roles::Role::Scout) {
                handler.bad_scout_targets.push(t);
                handler
                    .roles
                    .set_target(serial, super::roles::Role::Scout, None);
            }
            handler.roles.clear_track(serial);
            handler.order(game, Command::ClearOrders { units: vec![unit] }, events);
        } else if !idle {
            found_any = true;
            continue;
        }
        let is_sea_scout = is_ship(&PlayerView::new(game, me), unit);
        if is_sea_scout && put_ashore(handler, game, pos, events) {
            found_any = true;
            continue;
        }
        let claimed: Vec<Position> = handler
            .roles
            .targeted(super::roles::Role::Scout)
            .into_iter()
            .filter(|(s, _)| *s != serial)
            .map(|(_, p)| p)
            .chain(handler.bad_scout_targets.iter().copied())
            .collect();
        let target = {
            let view = PlayerView::new(game, me);
            best_target_with(&view, unit, pos, &claimed, &handler.recon)
        };
        match target {
            Some(to) => {
                found_any = true;
                handler
                    .roles
                    .set_target(serial, super::roles::Role::Scout, Some(to));
                let mut units = vec![unit];
                if is_sea_scout {
                    units.extend(board(handler, game, unit, pos, game.round()));
                }
                handler.order(game, Command::GoTo { units, to }, events);
            }
            None => {
                // Nothing left to explore that this unit can reach: its scouting is over and it
                // awaits orders (the registry says so, `docs/handler-unit-registry.md`).
                handler.roles.release(serial, super::roles::Role::Scout);
            }
        }
    }
    if handler.sea_scouting && !ship_out {
        // The ship is gone (sunk, or called back): the search by sea is over until another is
        // built. The flag stood for hundreds of turns while land scouts went on (task 567).
        handler.sea_scouting = false;
    }
    if !found_any {
        // Nothing reachable shows anything new: the rest of the map needs ships or a gate.
        handler.frontier_closed = true;
        if handler.sea_scouting {
            // A ship lost on the way is built again; only a ship that found nothing ends it.
            handler.sea_done = ship_out;
            handler.sea_scouting = false;
        }
        // The scouting is over; the scout bound for a tower goes on.
        handler.roles.release_role(super::roles::Role::Scout);
        handler.roles.release_role(super::roles::Role::SeaParty);
    }
}

/// The sea scout in a city takes spare fighters aboard, up to [`PARTY`] in all, to put ashore on
/// land nobody holds. The party sails with the ship (the order names them) and is left alone by
/// the rest of the scouting meanwhile. Returns the fighters taken aboard now, with the ones
/// already aboard.
fn board(
    handler: &mut Handler,
    world: &World,
    ship: hecs::Entity,
    at: Position,
    round: u32,
) -> Vec<hecs::Entity> {
    let game_turn = round;
    let view = PlayerView::new(world, handler.player);
    let mut party: Vec<hecs::Entity> = view
        .my_units()
        .into_iter()
        .filter(|&u| {
            handler
                .roles
                .is(view.serial_of(u), super::roles::Role::SeaParty)
                && view.unit_position(u) == Some(at)
        })
        .collect();
    if view.city_at(at).is_none() {
        return party;
    }
    let room = (view.unit_stat(ship, Stat::Transport).max(0) as usize).min(PARTY);
    let mut spare: Vec<hecs::Entity> = view
        .garrison(at)
        .into_iter()
        .filter(|&u| is_fighter(&view, u) && handler.roles.is_free(view.serial_of(u)))
        .collect();
    view.sort_by_serial(&mut spare);
    // The city's garrison holds its role and is not in the pool.
    let free = spare.len();
    let taken: Vec<hecs::Entity> = spare
        .into_iter()
        .take(room.saturating_sub(party.len()).min(free))
        .collect();
    let serials: Vec<Serial> = taken.iter().map(|&u| view.serial_of(u)).collect();
    party.extend(taken);
    handler
        .roles
        .claim_all(serials, super::roles::Role::SeaParty, game_turn);
    party
}

/// Whether land nobody of the wizard's stands on, which still has unexplored ground at its edge,
/// is the land mass of the square: it is found by walking the explored land from it.
fn land_unclaimed(view: &PlayerView, start: Position) -> bool {
    use crate::map::terrain::TerrainClass;
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
fn put_ashore(
    handler: &mut Handler,
    game: &mut dyn Host,
    ship_at: Position,
    events: &mut Vec<Event>,
) -> bool {
    use crate::map::terrain::TerrainClass;
    let me = handler.player;
    let (rider, shore) = {
        let view = PlayerView::new(game, me);
        let Some(rider) = view.my_units().into_iter().find(|&u| {
            handler
                .roles
                .is(view.serial_of(u), super::roles::Role::SeaParty)
                && view.unit_position(u) == Some(ship_at)
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
    if !handler
        .send(
            game,
            Command::GoTo {
                units: vec![rider],
                to: shore,
            },
            events,
        )
        .is_ok()
    {
        return false;
    }
    // Ashore, the rider is an ordinary scout.
    handler.roles.release(serial, super::roles::Role::SeaParty);
    let _ = handler
        .roles
        .claim(serial, super::roles::Role::Scout, game.round(), None);
    true
}

/// A scout walks to a tower nobody guards any more and changes plane there, when the ground
/// across it is unexplored; the search for the rest of the world then goes on over there.
fn planar_scout(handler: &mut Handler, game: &mut dyn Host, events: &mut Vec<Event>) {
    let me = handler.player;
    let (tower, unit) = {
        let view = PlayerView::new(game, me);
        let alive: Vec<Serial> = view.my_units().iter().map(|&u| view.serial_of(u)).collect();
        handler.roles.prune(&alive);
        let tower = view
            .towers()
            .into_iter()
            .filter(|&(p, intact)| {
                !intact
                    && !handler.bad_scout_targets.contains(&p)
                    // An enemy unit stands on it (task 567): the scout would wait next to it for
                    // ever, as an order onto an occupied square does not take it there.
                    && view.hostile_at(p).is_none()
                    && (worth_crossing(&view, p) || blind_across(handler, &view, p, game.round()))
            })
            .map(|(p, _)| p)
            .min_by_key(|&p| (p.plane as u8, p.y, p.x));
        let Some(tower) = tower else {
            // No tower is a way across now (all taken by enemy units, or all explored): a scout
            // sent to one is let go, to be recruited again when one opens.
            let gone = handler.roles.planar();
            handler.planar_tower = None;
            if let Some(serial) = gone {
                handler
                    .roles
                    .release(serial, super::roles::Role::PlanarScout);
                let unit = view
                    .my_units()
                    .into_iter()
                    .find(|&u| view.serial_of(u) == serial);
                drop(view);
                if let Some(unit) = unit {
                    handler.order(game, Command::ClearOrders { units: vec![unit] }, events);
                }
            }
            return;
        };
        let unit = handler.roles.planar().and_then(|s| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        });
        (tower, unit)
    };
    let unit = match unit {
        Some(u) => u,
        None => {
            let Some(u) = recruit(handler, game, Some(tower)) else {
                return;
            };
            let serial = PlayerView::new(game, me).serial_of(u);
            let _ = handler.roles.claim(
                serial,
                super::roles::Role::PlanarScout,
                game.round(),
                Some(tower),
            );
            handler.planar_tower = Some(tower);
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
        if shifts
            && handler
                .send(game, Command::ShiftPlane { units: vec![unit] }, events)
                .is_ok()
        {
            // Across, the scout is an ordinary one again.
            let serial = PlayerView::new(game, me).serial_of(unit);
            handler
                .roles
                .release(serial, super::roles::Role::PlanarScout);
            let _ = handler
                .roles
                .claim(serial, super::roles::Role::Scout, game.round(), None);
            handler.frontier_closed = false;
            handler.bad_scout_targets.clear();
        }
    } else if idle {
        // A unit that has since ended up where it cannot walk to the tower (another plane, an
        // island) is let go, so that another is recruited; ordered each turn, the walk would
        // be refused each turn.
        if !PlayerView::new(game, me).can_reach(&[unit], tower) {
            let serial = PlayerView::new(game, me).serial_of(unit);
            handler
                .roles
                .release(serial, super::roles::Role::PlanarScout);
            return;
        }
        handler.order(
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
fn recruit(handler: &Handler, world: &World, reach: Option<Position>) -> Option<hecs::Entity> {
    let view = PlayerView::new(world, handler.player);
    let mut best: Option<(i32, std::cmp::Reverse<Serial>, hecs::Entity)> = None;
    let mut reached = super::view::ReachCache::default();
    for u in view.my_units() {
        // A spirit scouts too (it flies, and a city does not need it); a node it could meld
        // with takes it from the scouting (`meld.rs`).
        let spirit = super::meld::is_melder(&view, u);
        if !(is_fighter(&view, u) || spirit) || !handler.roles.is_free(view.serial_of(u)) {
            continue;
        }
        if view.unit_position(u).is_none() {
            continue;
        }
        if reach.is_some_and(|to| !reached.can_reach(&view, &[u], to)) {
            continue;
        }
        let key = (
            view.unit_stat(u, Stat::Moves) * 2
                + i32::from(view.unit_has_flag(u, crate::rules::unit_flag::UnitFlag::Flies)) * 2,
            std::cmp::Reverse(view.serial_of(u)),
            u,
        );
        if best.as_ref().is_none_or(|b| (key.0, &key.1) > (b.0, &b.1)) {
            best = Some(key);
        }
    }
    best.map(|b| b.2)
}

#[cfg(test)]
/// The reachable square worth most to this scout: new ground within sight of it, less the
/// walk, skipping the patches other scouts are heading for.
fn best_target(
    view: &PlayerView,
    unit: hecs::Entity,
    from: Position,
    claimed: &[Position],
) -> Option<Position> {
    best_target_with(view, unit, from, claimed, &[])
}

/// [`best_target`] with the towns the war plan wants looked at (`war_plan.rs`): new ground near
/// one of them is worth more than other new ground.
pub(crate) fn best_target_with(
    view: &PlayerView,
    unit: hecs::Entity,
    from: Position,
    claimed: &[Position],
    recon: &[Position],
) -> Option<Position> {
    let reachable = view.reachable(&[unit]);
    // A unit shows the squares within its sight of where it stands, and no more.
    let look = view.unit_sight(unit);
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
        let mut value = fresh as f32 - walk * WALK_COST * 4.0;
        if recon
            .iter()
            .any(|&r| distance(view, r, p).is_some_and(|d| d <= RECON_NEAR))
        {
            value += RECON_BONUS;
        }
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

/// How many squares within the sight `look` of `at` the wizard has not explored.
fn unexplored_near(view: &PlayerView, at: Position, look: HalfInt) -> u32 {
    let mut n = 0;
    let reach = crate::map::fog::squares_seen(look) as i32;
    for dy in -reach..=reach {
        for dx in (-reach..=reach)
            .filter(|&dx| crate::map::fog::in_sight(dx, dy, look, view.diagonal_extra_cost()))
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
    use super::super::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn city() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    #[test]
    fn unexplored_ground_is_counted_within_the_look_and_vanishes_when_seen() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let far = at(4, 4);
        let view = PlayerView::new(&w, P0);
        assert_eq!(
            unexplored_near(&view, far, HalfInt::from_raw(5)),
            25,
            "all 25 squares of a look of 2.5 squares unseen"
        );
        // The city's own surroundings are known.
        assert_eq!(unexplored_near(&view, city(), HalfInt::from_raw(1)), 0);
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        assert_eq!(unexplored_near(&view, far, HalfInt::from_raw(5)), 0);
    }

    #[test]
    fn the_best_target_is_new_ground_the_unit_can_walk_to() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let walker = spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let view = PlayerView::new(&w, P0);
        let target =
            best_target(&view, walker, city(), &[]).ok_or("there is something to explore")?;
        assert_ne!(target, city());
        assert!(view.reachable(&[walker]).contains(&target));
        assert!(unexplored_near(&view, target, HalfInt::from_raw(3)) > 0);
        // The same choice every time.
        assert_eq!(best_target(&view, walker, city(), &[]), Some(target));
        Ok(())
    }

    #[test]
    fn a_target_near_another_scouts_is_left_to_it() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let walker = spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        let view = PlayerView::new(&w, P0);
        let first = best_target(&view, walker, city(), &[]).ok_or("a first target")?;
        let second = best_target(&view, walker, city(), &[first]).ok_or("a second target")?;
        let apart = distance(&view, first, second).ok_or("the targets are on one plane")?;
        assert!(apart >= KEEP_APART, "{apart}");
        Ok(())
    }

    #[test]
    fn nothing_is_a_target_once_the_whole_map_is_explored() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let walker = spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1)[0];
        w.reveal_map(P0);
        let view = PlayerView::new(&w, P0);
        assert_eq!(best_target(&view, walker, city(), &[]), None);
    }

    #[test]
    fn the_fastest_spare_fighter_becomes_the_scout() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let rider = spawn(&mut w, P0, city(), 107 /* high_men_cavalry */, 1)[0];
        let handler = super::super::testworld::handler(P0);
        assert_eq!(recruit(&handler, &w, None), Some(rider));
    }

    #[test]
    fn a_city_keeps_its_last_fighter() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1);
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        assert_eq!(recruit(&handler, &w, None), None);
        // A lone fighter outside any city is free to go.
        let outside = spawn(&mut w, P0, at(5, 5), 104 /* high_men_spearmen */, 1)[0];
        super::super::roles::register(&mut handler, &w, w.round());
        assert_eq!(recruit(&handler, &w, None), Some(outside));
    }

    #[test]
    fn a_scout_is_recruited_and_sent_to_new_ground() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(handler.roles.scouting().len(), 1);
        assert_eq!(handler.scout_targets().len(), 1);
        assert!(!handler.frontier_closed);
    }

    #[test]
    fn a_spirit_with_no_node_to_meld_with_is_a_scout() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let spirit = spawn(&mut w, P0, city(), 154 /* magic_spirit */, 1)[0];
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert!(handler.roles.is(serial, super::super::roles::Role::Scout));
        assert_eq!(handler.scout_targets().len(), 1);
    }

    #[test]
    fn with_no_spare_fighter_nobody_scouts_and_the_search_stays_open() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1);
        let mut handler = super::super::testworld::handler(P0);
        super::super::roles::register(&mut handler, &w, w.round());
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(handler.roles.scouting().is_empty());
        assert!(!handler.frontier_closed);
    }

    #[test]
    fn when_nothing_new_can_be_reached_scouting_by_land_is_over() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        w.reveal_map(P0);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(handler.frontier_closed);
        assert!(handler.roles.scouting().is_empty());
        assert!(handler.scout_targets().is_empty());
    }

    #[test]
    fn a_ship_scout_that_finds_nothing_ends_the_search_by_sea_but_a_lost_ship_does_not() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let ship = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            36, /* galley */
            1,
        )[0];
        let walker = spawn(&mut w, P0, at(5, 5), 104 /* high_men_spearmen */, 1)[0];
        for (scout_unit, done) in [(ship, true), (walker, false)] {
            let mut handler = super::super::testworld::handler(P0);
            handler.sea_scouting = true;
            let _ = handler.roles.claim(
                PlayerView::new(&w, P0).serial_of(scout_unit),
                super::super::roles::Role::Scout,
                1,
                None,
            );
            scout(&mut handler, &mut w, &mut Vec::new());
            assert!(handler.frontier_closed);
            assert!(!handler.sea_scouting);
            assert_eq!(handler.sea_done, done);
        }
    }

    #[test]
    fn the_search_by_sea_ends_when_the_ship_is_gone_though_land_scouts_go_on() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        // The ship that was out is lost: no ship is among the scouts.
        handler.sea_scouting = true;
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(
            !handler.roles.scouting().is_empty(),
            "a scout is recruited on land"
        );
        assert!(!handler.frontier_closed, "land scouting goes on");
        assert!(!handler.sea_scouting, "the flag does not outlive the ship");
        assert!(!handler.sea_done, "a lost ship is built again");
    }

    #[test]
    fn a_sea_scout_in_port_takes_spare_fighters_aboard_and_leaves_the_city_one() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ship = spawn(&mut w, P0, city(), 36 /* galley */, 1)[0];
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 3);
        let mut handler = super::super::testworld::handler(P0);
        let _ = handler.roles.claim(
            PlayerView::new(&w, P0).serial_of(ship),
            super::super::roles::Role::Scout,
            1,
            None,
        );
        handler.sea_scouting = true;
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(
            handler.roles.count(super::super::roles::Role::SeaParty),
            PARTY
        );
        assert!(handler
            .roles
            .members(super::super::roles::Role::SeaParty)
            .iter()
            .all(|s| handler.roles.is_scouting(*s)));
    }

    #[test]
    fn a_sea_scout_puts_a_fighter_ashore_on_land_nobody_holds() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // Off the east island's west shore, which the wizard has just seen.
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let view = PlayerView::new(&w, P0);
        let mut handler = super::super::testworld::handler(P0);
        let _ = handler.roles.claim(
            view.serial_of(boat),
            super::super::roles::Role::Scout,
            1,
            None,
        );
        let _ = handler.roles.claim(
            view.serial_of(walker),
            super::super::roles::Role::SeaParty,
            1,
            None,
        );
        handler.sea_scouting = true;
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(
            !handler.roles.any(super::super::roles::Role::SeaParty),
            "the rider went ashore"
        );
        let landed = w.units().unit_position(walker);
        assert!(landed.is_some_and(|p| p.x == 28), "{landed:?}");
    }

    #[test]
    fn a_rider_whose_ship_no_longer_scouts_is_set_free() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        let view = PlayerView::new(&w, P0);
        let (ship, rider) = (view.serial_of(boat), view.serial_of(walker));
        let mut handler = super::super::testworld::handler(P0);
        // The ship has no role; the rider still waits to be put ashore.
        let _ = handler
            .roles
            .claim(rider, super::super::roles::Role::SeaParty, 1, None);
        assert!(!handler.roles.is_scouting(ship));
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(
            !handler.roles.is(rider, super::super::roles::Role::SeaParty),
            "no ship scouts, so nobody waits for one"
        );
    }

    #[test]
    fn a_sea_scout_does_not_land_on_its_own_island() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let sea = at(11, 8);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let view = PlayerView::new(&w, P0);
        let mut handler = super::super::testworld::handler(P0);
        let _ = handler.roles.claim(
            view.serial_of(boat),
            super::super::roles::Role::Scout,
            1,
            None,
        );
        let _ = handler.roles.claim(
            view.serial_of(walker),
            super::super::roles::Role::SeaParty,
            1,
            None,
        );
        handler.sea_scouting = true;
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(handler.roles.count(super::super::roles::Role::SeaParty), 1);
    }

    #[test]
    fn a_scout_walks_to_a_tower_nobody_guards_and_changes_plane_there() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        w.explore(P0, tower, HalfInt::from_raw(3));
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        let serial = handler
            .roles
            .planar()
            .ok_or("a scout is sent to the tower")?;
        let unit = PlayerView::new(&w, P0)
            .my_units()
            .into_iter()
            .find(|&u| PlayerView::new(&w, P0).serial_of(u) == serial)
            .ok_or("the scout lives")?;
        assert!(PlayerView::new(&w, P0).unit_has_order(unit));
        // Standing on the tower, it goes across, and the search is open again over there.
        handler.frontier_closed = true;
        let spare = spawn(&mut w, P0, tower, 104 /* high_men_spearmen */, 1)[0];
        // The one on its way is replaced by the one standing on the tower.
        handler
            .roles
            .release_role(super::super::roles::Role::PlanarScout);
        let _ = handler.roles.claim(
            PlayerView::new(&w, P0).serial_of(spare),
            super::super::roles::Role::PlanarScout,
            1,
            None,
        );
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(
            w.units().unit_position(spare).map(|p| p.plane),
            Some(Plane::Myrror)
        );
        assert!(!handler.frontier_closed);
        assert_eq!(handler.roles.planar(), None);
        Ok(())
    }

    #[test]
    fn a_tower_with_an_enemy_unit_on_it_is_no_way_across_and_a_scout_bound_for_it_is_let_go(
    ) -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        w.explore(P0, tower, HalfInt::from_raw(3));
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(handler.roles.planar().is_some(), "the open tower is chosen");
        // An enemy unit moves onto the tower while the scout is on its way.
        spawn(
            &mut w,
            super::super::testworld::P1,
            tower,
            104, /* high_men_spearmen */
            1,
        );
        assert!(
            PlayerView::new(&w, P0).hostile_at(tower).is_some(),
            "the unit on the tower is hostile to the scout"
        );
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(handler.roles.planar(), None);
        assert_eq!(handler.planar_tower, None);
        assert!(handler.roles.planar().is_none());
        // And it is not chosen again while the enemy stands there.
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(handler.roles.planar(), None);
        Ok(())
    }

    #[test]
    fn a_guarded_tower_is_not_a_way_across() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(8, 6), true)?;
        w.explore(P0, at(8, 6), HalfInt::from_raw(3));
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        assert_eq!(handler.roles.planar(), None);
        Ok(())
    }

    #[test]
    fn a_closed_frontier_is_left_alone() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        handler.frontier_closed = true;
        scout(&mut handler, &mut w, &mut Vec::new());
        assert!(handler.roles.scouting().is_empty());
    }

    #[test]
    fn a_scout_that_stands_still_on_its_order_gives_the_target_up_and_chooses_another() -> TestResult
    {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut handler = super::super::testworld::handler(P0);
        scout(&mut handler, &mut w, &mut Vec::new());
        let (serial, first) = *handler
            .roles
            .targeted(super::super::roles::Role::Scout)
            .first()
            .ok_or("the scout has a target")?;
        // No turn passes, so the scout has no moves left: each look finds it where it was.
        for _ in 0..=STUCK_TURNS {
            scout(&mut handler, &mut w, &mut Vec::new());
        }
        assert!(
            handler.bad_scout_targets.contains(&first),
            "{:?}",
            handler.bad_scout_targets
        );
        assert_ne!(
            handler
                .roles
                .target_of(serial, super::super::roles::Role::Scout),
            Some(first)
        );
        Ok(())
    }

    #[test]
    fn a_plane_hardly_seen_is_worth_a_crossing_after_the_build_up_and_not_before() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(4, 4), false)?;
        let tower = at(4, 4);
        // Early on the wizard builds up instead.
        assert!(!blind_across(
            &super::super::testworld::handler(P0),
            &PlayerView::new(&w, P0),
            tower,
            w.round()
        ));
        // Late, with the far plane unseen and no unit there, it is worth looking.
        w.set_round(400);
        assert!(blind_across(
            &super::super::testworld::handler(P0),
            &PlayerView::new(&w, P0),
            tower,
            w.round()
        ));
        // Once the far plane is seen, or a unit of the wizard stands on it, it is not.
        let far = Position {
            plane: crate::map::Plane::Myrror,
            ..tower
        };
        let scout = spawn(&mut w, P0, far, 113 /* high_men_paladins */, 1);
        assert!(!blind_across(
            &super::super::testworld::handler(P0),
            &PlayerView::new(&w, P0),
            tower,
            w.round()
        ));
        w.cheat_remove_unit(scout[0]);
        assert!(blind_across(
            &super::super::testworld::handler(P0),
            &PlayerView::new(&w, P0),
            tower,
            w.round()
        ));
        w.reveal_map(P0);
        assert!(!blind_across(
            &super::super::testworld::handler(P0),
            &PlayerView::new(&w, P0),
            tower,
            w.round()
        ));
        Ok(())
    }
}
