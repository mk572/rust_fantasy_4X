//! Scouts: one or more units whose job is to find the rest of the world (`docs/ai-design.md`
//! section 5.1).
//!
//! A scout walks to the reachable square from which it will see the most unexplored ground,
//! nearer squares first, and takes a new target when it arrives or stops. Scouts keep apart:
//! two never head for the same patch. When nothing reachable has unexplored ground near it the
//! scouts rejoin the army.

use f4x_def::rules::ai_strategy::ScoutingLevers;

use crate::lever::Levered;
use f4x_core::map::{Plane, Position};
use f4x_core::player::view::PlayerView;
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::Stat;

use crate::view::{distance, is_fighter, is_ship};
use crate::Ai;
use f4x_master::Game;

/// How many scouts the wizard wants. While it builds up (`phase::Phase::Build`) it looks harder
/// for places to settle and for its neighbours: two scouts until it has four cities. No settler is
/// built before a place for a city is known, so the sooner the land is seen the sooner the second
/// city is founded.
pub(crate) fn wanted(levers: &ScoutingLevers, cities: usize, building: bool) -> usize {
    let base = match cities {
        0 => 0,
        1..=3 => 1 + usize::from(building),
        _ => 2,
    };
    base + levers.scouts as usize
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
    unexplored_near(view, across(tower), HalfInt::from_raw(3)) > 0
}

/// Past the build-up, a plane the wizard has hardly seen and has no unit on is worth a crossing
/// at any open tower, whatever the ground at the tower: the rivals that live over there are
/// found only by looking (task 502, a rival on the other plane is never met otherwise).
fn blind_across(view: &PlayerView, tower: Position, round: u32) -> bool {
    let far = across(tower).plane;
    crate::phase::phase(view, round) != crate::phase::Phase::Build
        && !view
            .my_units()
            .into_iter()
            .any(|u| view.unit_position(u).is_some_and(|p| p.plane == far))
        && view.explored_share(far) < view.levers().scouting.blind_share
}

/// Make sure the wizard has enough scouts and send them out.
pub fn scout(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    planar_scout(ai, game, events);
    if ai.frontier_closed {
        return;
    }
    ai.write(game).tidy_musters();
    let (scouts, cities, building) = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        let mut scouts: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| tasks.roles.is_scouting(view.serial_of(u)))
            .collect();
        view.sort_by_serial(&mut scouts);
        (
            scouts,
            view.my_cities().len(),
            crate::phase::phase(&view, game.round()) == crate::phase::Phase::Build,
        )
    };
    if scouts.len() < wanted(&game.levers().scouting, cities, building) {
        if let Some(unit) = recruit(ai, game, None) {
            let serial = PlayerView::new(game, me).serial_of(unit);
            let _ = ai
                .write(game)
                .claim(serial, crate::roles::Role::Scout, None);
        }
    }
    let scouts: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        let mut list: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| tasks.roles.is_scouting(view.serial_of(u)))
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
        ai.write(game).release_role(crate::roles::Role::SeaParty);
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        scouts
            .into_iter()
            .filter(|&u| tasks.roles.is_scouting(view.serial_of(u)))
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
        let riding = {
            let tasks = ai.tasks(game);
            tasks.roles.is(serial, crate::roles::Role::SeaParty)
                || tasks.roles.is(serial, crate::roles::Role::PlanarScout)
        };
        if riding {
            found_any = true;
            continue;
        }
        // A scout that has not moved for a few turns although it has an order is stuck: give
        // its target up (a guarded or blocked patch) and choose another.
        let stood = ai.write(game).track(serial, pos, !idle);
        let stuck = stood >= game.levers().scouting.stuck_turns;
        if stuck {
            let bound_for = ai
                .tasks(game)
                .roles
                .target_of(serial, crate::roles::Role::Scout);
            if let Some(t) = bound_for {
                ai.bad_scout_targets.push(t);
                ai.write(game)
                    .set_target(serial, crate::roles::Role::Scout, None);
            }
            ai.write(game).clear_track(serial);
            ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
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
            .tasks(game)
            .roles
            .targeted(crate::roles::Role::Scout)
            .into_iter()
            .filter(|(s, _)| *s != serial)
            .map(|(_, p)| p)
            .chain(ai.bad_scout_targets.iter().copied())
            .collect();
        let target = {
            let view = PlayerView::new(game, me);
            best_target_with(&view, unit, pos, &claimed, &ai.war_plan.recon)
        };
        match target {
            Some(to) => {
                found_any = true;
                ai.write(game)
                    .set_target(serial, crate::roles::Role::Scout, Some(to));
                let mut units = vec![unit];
                if is_sea_scout {
                    units.extend(board(ai, game, unit, pos));
                }
                ai.order(game, Command::GoTo { units, to }, events);
            }
            None => {
                // Nothing left to explore that this unit can reach: its scouting is over and it
                // awaits orders (the registry says so, `docs/ai-unit-registry.md`).
                ai.write(game).release(serial, crate::roles::Role::Scout);
            }
        }
    }
    if ai.sea_scouting && !ship_out {
        // The ship is gone (sunk, or called back): the search by sea is over until another is
        // built. The flag stood for hundreds of turns while land scouts went on (task 567).
        ai.sea_scouting = false;
    }
    if !found_any {
        // Nothing reachable shows anything new: the rest of the map needs ships or a gate.
        ai.frontier_closed = true;
        if ai.sea_scouting {
            // A ship lost on the way is built again; only a ship that found nothing ends it.
            ai.sea_done = ship_out;
            ai.sea_scouting = false;
        }
        // The scouting is over; the scout bound for a tower goes on.
        ai.write(game).release_role(crate::roles::Role::Scout);
        ai.write(game).release_role(crate::roles::Role::SeaParty);
    }
}

/// The sea scout in a city takes spare fighters aboard, up to `sea_party` in all, to put ashore on
/// land nobody holds. The party sails with the ship (the order names them) and is left alone by
/// the rest of the scouting meanwhile; each of them is recorded as carried by the ship. Returns the
/// fighters taken aboard now, with the ones already aboard.
fn board(ai: &mut Ai, game: &mut Game, ship: hecs::Entity, at: Position) -> Vec<hecs::Entity> {
    let (party, taken, carried, ship_serial, in_city) = {
        let view = PlayerView::new(game, ai.player);
        let tasks = view.tasks();
        let mut party: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| {
                tasks
                    .roles
                    .is(view.serial_of(u), crate::roles::Role::SeaParty)
                    && view.unit_position(u) == Some(at)
            })
            .collect();
        let carried: Vec<Serial> = party.iter().map(|&u| view.serial_of(u)).collect();
        let in_city = view.city_at(at).is_some();
        let mut taken = Vec::new();
        if in_city {
            let room = (view.unit_stat(ship, Stat::Transport).max(0) as usize)
                .min(view.levers().scouting.sea_party);
            let mut spare: Vec<hecs::Entity> = view
                .garrison(at)
                .into_iter()
                .filter(|&u| is_fighter(&view, u) && tasks.roles.is_free(view.serial_of(u)))
                .collect();
            view.sort_by_serial(&mut spare);
            // The city's garrison holds its role and is not in the pool.
            let free = spare.len();
            let more: Vec<hecs::Entity> = spare
                .into_iter()
                .take(room.saturating_sub(party.len()).min(free))
                .collect();
            taken = more.iter().map(|&u| view.serial_of(u)).collect();
            party.extend(more);
        }
        (party, taken, carried, view.serial_of(ship), in_city)
    };
    let mut aboard = carried;
    if in_city {
        aboard.extend(
            ai.write(game)
                .claim_all(taken, crate::roles::Role::SeaParty),
        );
    }
    for serial in aboard {
        ai.write(game).tie(serial, ship_serial);
    }
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
        let tasks = view.tasks();
        let Some(rider) = view.my_units().into_iter().find(|&u| {
            tasks
                .roles
                .is(view.serial_of(u), crate::roles::Role::SeaParty)
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
    if !ai
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
    ai.write(game).release(serial, crate::roles::Role::SeaParty);
    let _ = ai
        .write(game)
        .claim(serial, crate::roles::Role::Scout, None);
    true
}

/// A scout walks to a tower nobody guards any more and changes plane there, when the ground
/// across it is unexplored; the search for the rest of the world then goes on over there.
fn planar_scout(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    ai.write(game).tidy_musters();
    let tower = {
        let view = PlayerView::new(game, me);
        view.towers()
            .into_iter()
            .filter(|&(p, intact)| {
                !intact
                    && !ai.bad_scout_targets.contains(&p)
                    // An enemy unit stands on it (task 567): the scout would wait next to it for
                    // ever, as an order onto an occupied square does not take it there.
                    && view.hostile_at(p).is_none()
                    && (worth_crossing(&view, p) || blind_across(&view, p, game.round()))
            })
            .map(|(p, _)| p)
            .min_by_key(|&p| (p.plane as u8, p.y, p.x))
    };
    // The unit sent to cross, if one is.
    let (gone, unit) = {
        let view = PlayerView::new(game, me);
        let serial = view.tasks().roles.planar();
        let unit = serial.and_then(|s| {
            view.my_units()
                .into_iter()
                .find(|&u| view.serial_of(u) == s)
        });
        (serial, unit)
    };
    let Some(tower) = tower else {
        // No tower is a way across now (all taken by enemy units, or all explored): a scout
        // sent to one is let go, to be recruited again when one opens.
        ai.planar_tower = None;
        if let Some(serial) = gone {
            ai.write(game)
                .release(serial, crate::roles::Role::PlanarScout);
            if let Some(unit) = unit {
                ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
            }
        }
        return;
    };
    let unit = match unit {
        Some(u) => u,
        None => {
            let Some(u) = recruit(ai, game, Some(tower)) else {
                return;
            };
            let serial = PlayerView::new(game, me).serial_of(u);
            let _ = ai
                .write(game)
                .claim(serial, crate::roles::Role::PlanarScout, Some(tower));
            ai.planar_tower = Some(tower);
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
            && ai
                .send(game, Command::ShiftPlane { units: vec![unit] }, events)
                .is_ok()
        {
            // Across, the scout is an ordinary one again.
            let serial = PlayerView::new(game, me).serial_of(unit);
            ai.write(game)
                .release(serial, crate::roles::Role::PlanarScout);
            let _ = ai
                .write(game)
                .claim(serial, crate::roles::Role::Scout, None);
            ai.frontier_closed = false;
            ai.bad_scout_targets.clear();
        }
    } else if idle {
        // A unit that has since ended up where it cannot walk to the tower (another plane, an
        // island) is let go, so that another is recruited; ordered each turn, the walk would
        // be refused each turn.
        if !PlayerView::new(game, me).can_reach(&[unit], tower) {
            let serial = PlayerView::new(game, me).serial_of(unit);
            ai.write(game)
                .release(serial, crate::roles::Role::PlanarScout);
            return;
        }
        ai.order(
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
fn recruit(ai: &Ai, world: &World, reach: Option<Position>) -> Option<hecs::Entity> {
    let view = PlayerView::new(world, ai.player);
    let tasks = view.tasks();
    let mut best: Option<(i32, std::cmp::Reverse<Serial>, hecs::Entity)> = None;
    let mut reached = crate::view::ReachCache::default();
    for u in view.my_units() {
        // A spirit scouts too (it flies, and a city does not need it); a node it could meld
        // with takes it from the scouting (`meld.rs`).
        let spirit = crate::meld::is_melder(&view, u);
        if !(is_fighter(&view, u) || spirit) || !tasks.roles.is_free(view.serial_of(u)) {
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
                + i32::from(view.unit_has_flag(u, f4x_core::rules::unit_flag::UnitFlag::Flies)) * 2,
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
            || claimed.iter().any(|&c| {
                distance(view, c, p).is_some_and(|d| d < view.levers().scouting.keep_apart)
            })
        {
            continue;
        }
        let fresh = unexplored_near(view, p, look);
        if fresh == 0 {
            continue;
        }
        let walk = distance(view, from, p).unwrap_or(99) as f32;
        let mut value = fresh as f32 - walk * view.levers().scouting.walk_cost * 4.0;
        if recon
            .iter()
            .any(|&r| distance(view, r, p).is_some_and(|d| d <= view.levers().target.recon_near))
        {
            value += view.levers().target.recon_bonus;
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
    let reach = f4x_core::map::fog::squares_seen(look) as i32;
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
    use crate::testworld::levers;
    use crate::testworld::{at, game, spawn, spawn_tower, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn city() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    #[test]
    fn the_number_of_scouts_grows_with_the_cities() {
        assert_eq!(wanted(&levers().scouting, 0, true), 0);
        assert_eq!(wanted(&levers().scouting, 1, false), 1);
        assert_eq!(wanted(&levers().scouting, 3, false), 1);
        assert_eq!(wanted(&levers().scouting, 4, false), 2);
        assert_eq!(wanted(&levers().scouting, 12, false), 2);
    }

    #[test]
    fn a_wizard_that_builds_up_wants_a_second_scout() {
        assert_eq!(wanted(&levers().scouting, 1, true), 2);
        assert_eq!(wanted(&levers().scouting, 3, true), 2);
        assert_eq!(wanted(&levers().scouting, 2, false), 1);
    }

    /// The two units a wizard starts with are one garrison and one scout (a new game, no map seen).
    #[test]
    fn of_the_two_starting_units_one_scouts_from_the_first_turn() -> TestResult {
        use crate::roles::Role;
        let mut w = crate::testworld::fresh_game()?;
        let mut ai = Ai::new(crate::testworld::P0);
        ai.play_turn(&mut w);
        let view = PlayerView::new(&w, crate::testworld::P0);
        let roles: Vec<Option<Role>> = view
            .my_units()
            .iter()
            .map(|&u| ai.tasks(&w).roles.role_of(view.serial_of(u)))
            .collect();
        assert_eq!(roles.len(), 2, "a wizard starts with two units: {roles:?}");
        assert!(roles.contains(&Some(Role::Garrison)), "{roles:?}");
        assert!(roles.contains(&Some(Role::Scout)), "{roles:?}");
        Ok(())
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
        assert!(apart >= levers().scouting.keep_apart, "{apart}");
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
        let ai = Ai::new(P0);
        assert_eq!(recruit(&ai, &w, None), Some(rider));
    }

    #[test]
    fn a_city_keeps_its_last_fighter() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        assert_eq!(recruit(&ai, &w, None), None);
        // A lone fighter outside any city is free to go.
        let outside = spawn(&mut w, P0, at(5, 5), 104 /* high_men_spearmen */, 1)[0];
        crate::roles::register(&mut ai, &mut w);
        assert_eq!(recruit(&ai, &w, None), Some(outside));
    }

    #[test]
    fn a_scout_is_recruited_and_sent_to_new_ground() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.tasks(&w).roles.scouting().len(), 1);
        assert_eq!(ai.scout_targets(&w).len(), 1);
        assert!(!ai.frontier_closed);
    }

    #[test]
    fn a_spirit_with_no_node_to_meld_with_is_a_scout() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let spirit = spawn(&mut w, P0, city(), 154 /* magic_spirit */, 1)[0];
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        let serial = PlayerView::new(&w, P0).serial_of(spirit);
        assert!(ai.tasks(&w).roles.is(serial, crate::roles::Role::Scout));
        assert_eq!(ai.scout_targets(&w).len(), 1);
    }

    #[test]
    fn with_no_spare_fighter_nobody_scouts_and_the_search_stays_open() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 1);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.tasks(&w).roles.scouting().is_empty());
        assert!(!ai.frontier_closed);
    }

    #[test]
    fn when_nothing_new_can_be_reached_scouting_by_land_is_over() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.frontier_closed);
        assert!(ai.tasks(&w).roles.scouting().is_empty());
        assert!(ai.scout_targets(&w).is_empty());
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
            let mut ai = Ai::new(P0);
            ai.sea_scouting = true;
            let serial = PlayerView::new(&w, P0).serial_of(scout_unit);
            let _ = ai
                .write(&mut w)
                .claim(serial, crate::roles::Role::Scout, None);
            scout(&mut ai, &mut w, &mut Vec::new());
            assert!(ai.frontier_closed);
            assert!(!ai.sea_scouting);
            assert_eq!(ai.sea_done, done);
        }
    }

    #[test]
    fn the_search_by_sea_ends_when_the_ship_is_gone_though_land_scouts_go_on() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        // The ship that was out is lost: no ship is among the scouts.
        ai.sea_scouting = true;
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.scouting().is_empty(),
            "a scout is recruited on land"
        );
        assert!(!ai.frontier_closed, "land scouting goes on");
        assert!(!ai.sea_scouting, "the flag does not outlive the ship");
        assert!(!ai.sea_done, "a lost ship is built again");
    }

    #[test]
    fn a_sea_scout_in_port_takes_spare_fighters_aboard_and_leaves_the_city_one() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let ship = spawn(&mut w, P0, city(), 36 /* galley */, 1)[0];
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 3);
        let mut ai = Ai::new(P0);
        let serial = PlayerView::new(&w, P0).serial_of(ship);
        let _ = ai
            .write(&mut w)
            .claim(serial, crate::roles::Role::Scout, None);
        ai.sea_scouting = true;
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(
            ai.tasks(&w).roles.count(crate::roles::Role::SeaParty),
            levers().scouting.sea_party
        );
        let tasks = ai.tasks(&w);
        let carrier = PlayerView::new(&w, P0).serial_of(ship);
        assert_eq!(
            tasks.roles.tied_with(carrier),
            tasks.roles.members(crate::roles::Role::SeaParty),
            "the ship that carries each of the party is recorded"
        );
        assert!(tasks
            .roles
            .members(crate::roles::Role::SeaParty)
            .iter()
            .all(|s| tasks.roles.is_scouting(*s)));
    }

    #[test]
    fn a_sea_scout_puts_a_fighter_ashore_on_land_nobody_holds() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        // Off the east island's west shore, which the wizard has just seen.
        let sea = at(27, 9);
        let boat = spawn(&mut w, P0, sea, 36 /* galley */, 1)[0];
        let walker = spawn(&mut w, P0, sea, 104 /* high_men_spearmen */, 1)[0];
        w.explore(P0, sea, HalfInt::from_raw(3));
        let (ship, rider) = {
            let view = PlayerView::new(&w, P0);
            (view.serial_of(boat), view.serial_of(walker))
        };
        let mut ai = Ai::new(P0);
        let _ = ai
            .write(&mut w)
            .claim(ship, crate::roles::Role::Scout, None);
        let _ = ai
            .write(&mut w)
            .claim(rider, crate::roles::Role::SeaParty, None);
        ai.sea_scouting = true;
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::SeaParty),
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
        let mut ai = Ai::new(P0);
        // The ship has no role; the rider still waits to be put ashore.
        let _ = ai
            .write(&mut w)
            .claim(rider, crate::roles::Role::SeaParty, None);
        assert!(!ai.tasks(&w).roles.is_scouting(ship));
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(
            !ai.tasks(&w).roles.is(rider, crate::roles::Role::SeaParty),
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
        let (ship, rider) = {
            let view = PlayerView::new(&w, P0);
            (view.serial_of(boat), view.serial_of(walker))
        };
        let mut ai = Ai::new(P0);
        let _ = ai
            .write(&mut w)
            .claim(ship, crate::roles::Role::Scout, None);
        let _ = ai
            .write(&mut w)
            .claim(rider, crate::roles::Role::SeaParty, None);
        ai.sea_scouting = true;
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.tasks(&w).roles.count(crate::roles::Role::SeaParty), 1);
    }

    #[test]
    fn a_scout_walks_to_a_tower_nobody_guards_and_changes_plane_there() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        w.explore(P0, tower, HalfInt::from_raw(3));
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        let serial = ai
            .tasks(&w)
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
        ai.frontier_closed = true;
        let spare = spawn(&mut w, P0, tower, 104 /* high_men_spearmen */, 1)[0];
        // The one on its way is replaced by the one standing on the tower.
        ai.write(&mut w)
            .release_role(crate::roles::Role::PlanarScout);
        let serial = PlayerView::new(&w, P0).serial_of(spare);
        let _ = ai
            .write(&mut w)
            .claim(serial, crate::roles::Role::PlanarScout, None);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(
            w.units().unit_position(spare).map(|p| p.plane),
            Some(Plane::Myrror)
        );
        assert!(!ai.frontier_closed);
        assert_eq!(ai.tasks(&w).roles.planar(), None);
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
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(
            ai.tasks(&w).roles.planar().is_some(),
            "the open tower is chosen"
        );
        // An enemy unit moves onto the tower while the scout is on its way.
        spawn(
            &mut w,
            crate::testworld::P1,
            tower,
            104, /* high_men_spearmen */
            1,
        );
        assert!(
            PlayerView::new(&w, P0).hostile_at(tower).is_some(),
            "the unit on the tower is hostile to the scout"
        );
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.tasks(&w).roles.planar(), None);
        assert_eq!(ai.planar_tower, None);
        assert!(ai.tasks(&w).roles.planar().is_none());
        // And it is not chosen again while the enemy stands there.
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.tasks(&w).roles.planar(), None);
        Ok(())
    }

    #[test]
    fn a_guarded_tower_is_not_a_way_across() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn_tower(&mut w, at(8, 6), true)?;
        w.explore(P0, at(8, 6), HalfInt::from_raw(3));
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.tasks(&w).roles.planar(), None);
        Ok(())
    }

    #[test]
    fn a_closed_frontier_is_left_alone() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        ai.frontier_closed = true;
        scout(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.tasks(&w).roles.scouting().is_empty());
    }

    #[test]
    fn a_scout_that_stands_still_on_its_order_gives_the_target_up_and_chooses_another() -> TestResult
    {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, city(), 104 /* high_men_spearmen */, 2);
        let mut ai = Ai::new(P0);
        scout(&mut ai, &mut w, &mut Vec::new());
        let (serial, first) = *ai
            .tasks(&w)
            .roles
            .targeted(crate::roles::Role::Scout)
            .first()
            .ok_or("the scout has a target")?;
        // No turn passes, so the scout has no moves left: each look finds it where it was.
        for _ in 0..=levers().scouting.stuck_turns {
            scout(&mut ai, &mut w, &mut Vec::new());
        }
        assert!(
            ai.bad_scout_targets.contains(&first),
            "{:?}",
            ai.bad_scout_targets
        );
        assert_ne!(
            ai.tasks(&w)
                .roles
                .target_of(serial, crate::roles::Role::Scout),
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
        assert!(!blind_across(&PlayerView::new(&w, P0), tower, w.round()));
        // Late, with the far plane unseen and no unit there, it is worth looking.
        w.set_round(400);
        assert!(blind_across(&PlayerView::new(&w, P0), tower, w.round()));
        // Once the far plane is seen, or a unit of the wizard stands on it, it is not.
        let far = Position {
            plane: f4x_core::map::Plane::Myrror,
            ..tower
        };
        let scout = spawn(&mut w, P0, far, 113 /* high_men_paladins */, 1);
        assert!(!blind_across(&PlayerView::new(&w, P0), tower, w.round()));
        w.cheat_remove_unit(scout[0]);
        assert!(blind_across(&PlayerView::new(&w, P0), tower, w.round()));
        w.reveal_map(P0);
        assert!(!blind_across(&PlayerView::new(&w, P0), tower, w.round()));
        Ok(())
    }
}
