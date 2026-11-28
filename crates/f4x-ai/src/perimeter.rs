//! The perimeter of the cities (`Role::Perimeter`): what stands in the zone of an own city, the 21
//! squares its people work plus the towers and nodes within four squares of it, is dealt with by
//! the soldiers the city can spare (the surplus over `garrison_wanted`) and the free soldiers near.
//! It replaces the call-in of troops by a threatened city.
//!
//! One turn, for each stack of enemy units, raiders or monsters in a zone:
//!
//! - when the soldiers within reach are strong enough (`attack_force` times its strength) they
//!   gather on a square beside it; there the attack is `attack.rs`'s. They do not follow it out
//!   of the zone. Their role is `Perimeter`, `to` = the enemy's square;
//! - when it is the stronger nothing is done here: the city is sent soldiers by `reinforce.rs`.
//!
//! Units of a player not at war count only when the AI is open to war on it (`attack.rs`).
//!
//! A holder whose enemy is gone gives the role back. `Ai::threatened` (the cities the economy
//! builds soldiers for) is set here from the same look. See `docs/ai-unit-registry.md`.

use crate::lever::Levered;
use crate::roles::Role;
use crate::view::{is_fighter, stack_strength};
use crate::Ai;
use f4x_core::city::economy::area_offsets;
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::Event;
use f4x_master::Game;

/// A stack of the enemy in the zone of one of our cities.
struct Threat {
    square: Position,
    units: Vec<hecs::Entity>,
}

/// The squares round a city that its people work (`area_offsets`), the city itself left out.
pub fn zone(view: &PlayerView, city: Position) -> Vec<Position> {
    let mut squares = Vec::new();
    for (dx, dy) in area_offsets().filter(|&offset| offset != (0, 0)) {
        if let Some(p) = view.shift(city, dx, dy) {
            squares.push(p);
        }
    }
    let near_sites = view
        .towers()
        .into_iter()
        .map(|(p, _)| p)
        .chain(view.nodes().into_iter().map(|n| n.pos));
    for p in near_sites {
        if p.plane == city.plane
            && view
                .distance(city, p)
                .is_some_and(|d| d <= view.levers().attack.perimeter_site_range as u32)
            && !squares.contains(&p)
        {
            squares.push(p);
        }
    }
    squares
}

/// The stacks to be dealt with in the zones of the own cities (`attack::threat_at`).
fn threats(ai: &Ai, game: &Game) -> Vec<Threat> {
    let view = PlayerView::new(game, ai.player);
    let mut found: Vec<Threat> = Vec::new();
    let mut cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
    cities.sort_by_key(|p| (p.plane as u8, p.y, p.x));
    for city in cities {
        for square in zone(&view, city) {
            if found.iter().any(|t| t.square == square) {
                continue;
            }
            let Some(units) = crate::attack::threat_at(ai, game, &view, square) else {
                continue;
            };
            found.push(Threat { square, units });
        }
    }
    found
}

/// One turn of the perimeter: the soldiers that are `free` (the army's spare ones) and the ones that
/// already hold the role deal with the threats. Returns the free soldiers left.
pub fn run(
    ai: &mut Ai,
    game: &mut Game,
    mut free: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) -> Vec<hecs::Entity> {
    let me = ai.player;
    // The cities the economy builds soldiers for: the enemy near is more than the garrison holds.
    ai.threatened = crate::military::threats(&PlayerView::new(game, me))
        .iter()
        .map(|&(pos, _)| pos)
        .collect();
    let threats = threats(ai, game);
    // A holder whose enemy is gone (or whose city is no longer threatened) gives the role back.
    let holders = ai.tasks(game).roles.members(Role::Perimeter);
    for serial in holders {
        let to = ai.tasks(game).roles.objective_of(serial, Role::Perimeter);
        let wanted = to.is_some_and(|to| threats.iter().any(|t| t.square == to));
        let alive = {
            let view = PlayerView::new(game, me);
            view.my_units()
                .into_iter()
                .any(|u| view.serial_of(u) == serial)
        };
        if !wanted || !alive {
            ai.write(game).release(serial, Role::Perimeter);
        }
    }
    free.retain(|&u| is_fighter(&PlayerView::new(game, me), u));
    for threat in threats {
        let (their, pool, ratio) = {
            let view = PlayerView::new(game, me);
            let their = crate::view::fighting_strength(&view, &threat.units);
            // Soldiers that hold the role for this enemy or this city, and free ones in reach.
            let tasks = view.tasks();
            let mut pool: Vec<hecs::Entity> = view
                .my_units()
                .into_iter()
                .filter(|&u| {
                    let s = view.serial_of(u);
                    tasks.roles.is(s, Role::Perimeter)
                        && tasks
                            .roles
                            .objective_of(s, Role::Perimeter)
                            .is_some_and(|to| to == threat.square)
                })
                .collect();
            for &u in &free {
                let near = view
                    .unit_position(u)
                    .and_then(|p| view.distance(p, threat.square))
                    .is_some_and(|d| d <= view.levers().attack.perimeter_reach);
                if near && view.unit_moves_left(u) > 0 && !view.unit_has_order(u) {
                    pool.push(u);
                }
            }
            view.sort_by_serial(&mut pool);
            pool.dedup();
            // Soldiers that cannot go where the enemy stands (at sea) are not gathered for it.
            let pool = game.attackers_reaching(&pool, threat.square);
            (their, pool, view.levers().attack.attack_force)
        };
        if pool.is_empty() {
            continue;
        }
        // A battle is fought by a party of at most a battle side: more soldiers add nothing.
        let total = crate::view::fighting_strength(&PlayerView::new(game, me), &pool);
        // Whoever takes the role from the free list is spent for this turn.
        let idle: Vec<hecs::Entity> = {
            let view = PlayerView::new(game, me);
            pool.iter()
                .copied()
                .filter(|&u| !view.unit_has_order(u) && view.unit_moves_left(u) > 0)
                .collect()
        };
        let serials: Vec<_> = {
            let view = PlayerView::new(game, me);
            idle.iter().map(|&u| view.serial_of(u)).collect()
        };
        if total >= their * ratio {
            for &s in &serials {
                if ai.write(game).claim(s, Role::Perimeter, None).is_ok()
                    && ai.tasks(game).roles.objective_of(s, Role::Perimeter) != Some(threat.square)
                {
                    // The square to attack from is chosen below.
                    ai.write(game)
                        .stage(s, Role::Perimeter, None, threat.square);
                }
            }
            gather(ai, game, &idle, &threat, their * ratio, events);
        }
        free.retain(|u| !idle.contains(u));
    }
    free
}

/// The soldiers walk to a square beside the enemy. A battle is fought from one square (the game
/// refuses attackers standing apart, `BattleStartError::BadSides`): the strongest group that
/// shares a square beside the enemy stays where it is when it is enough, for the attack.
fn gather(
    ai: &mut Ai,
    game: &mut Game,
    idle: &[hecs::Entity],
    threat: &Threat,
    needed: f32,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let (walkers, standing): (Vec<hecs::Entity>, Option<Position>) = {
        let view = PlayerView::new(game, me);
        let mut groups: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        for &u in idle {
            let Some(at) = view.unit_position(u) else {
                continue;
            };
            if view.distance(at, threat.square) != Some(1) {
                continue;
            }
            match groups.iter_mut().find(|(p, _)| *p == at) {
                Some((_, list)) => list.push(u),
                None => groups.push((at, vec![u])),
            }
        }
        groups.sort_by_key(|(p, _)| (p.plane as u8, p.y, p.x));
        let ready = groups
            .into_iter()
            .map(|(_, list)| {
                let band = crate::view::party(&view, &list);
                (stack_strength(&view, &band), list)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|&(strength, _)| strength >= needed)
            .map(|(_, list)| list)
            .unwrap_or_default();
        // Everyone else (all of them, when no group is enough yet) gathers on one square beside
        // the enemy.
        let standing = ready.first().and_then(|&u| view.unit_position(u));
        let walkers = idle
            .iter()
            .copied()
            .filter(|u| !ready.contains(u))
            .collect();
        (walkers, standing)
    };
    // The square the force attacks from: the one a group that is enough stands on already, else
    // the last one on the way of the unit that arrives last (`assembly.rs`).
    if let Some(square) = standing {
        crate::assembly::set_beside(ai, game, threat.square, square);
        let ready: Vec<hecs::Entity> = idle
            .iter()
            .copied()
            .filter(|u| !walkers.contains(u))
            .collect();
        crate::assembly::take(ai, game, Role::Perimeter, &ready, square, threat.square);
    }
    // Soldiers that cannot get there now are tried again next turn.
    let _ = crate::assembly::march(ai, game, Role::Perimeter, &walkers, threat.square, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_master::Game;

    /// West's city holds one spearman; `n` of East's swordsmen stand two squares away and three
    /// of West's own wait three squares from them.
    fn besieged(n: usize) -> (Game, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(10, 6), 104 /* high_men_spearmen */, 1);
        spawn(&mut w, P1, at(8, 6), 105 /* high_men_swordsmen */, n);
        let spare = spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 3);
        w.make_contact(P0, P1);
        w.declare_war(P0, P1);
        (w, spare)
    }

    #[test]
    fn the_zone_is_the_work_area_of_the_city() {
        let w = game(COAST_CITY, 8 /* high_men */);
        let view = PlayerView::new(&w, P0);
        let squares = zone(&view, at(10, 6));
        assert!(squares.len() >= 20 && squares.contains(&at(8, 6)));
        assert!(!squares.contains(&at(8, 4)), "the corners are out");
    }

    /// A weak raider in the zone is gone for: the soldiers walk to a square beside it, with the
    /// role, that square as their target and the raider's as the objective.
    #[test]
    fn a_weak_stack_in_the_zone_is_gone_for_by_the_spare_soldiers() {
        let (mut w, spare) = besieged(1);
        let mut ai = Ai::new(P0);
        let _ = run(&mut ai, &mut w, spare.clone(), &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        for &u in &spare {
            assert!(view.unit_has_order(u));
            let (tasks, serial) = (ai.tasks(&w), view.serial_of(u));
            assert_eq!(
                tasks.roles.objective_of(serial, Role::Perimeter),
                Some(at(8, 6))
            );
            let from = tasks.roles.target_of(serial, Role::Perimeter);
            assert!(from.is_some_and(|p| view.distance(p, at(8, 6)) == Some(1)));
        }
    }

    /// With no enemy in the zone nobody keeps the role.
    #[test]
    fn the_role_ends_when_the_enemy_is_gone() {
        let (mut w, spare) = besieged(1);
        let mut ai = Ai::new(P0);
        let _ = run(&mut ai, &mut w, spare.clone(), &mut Vec::new());
        for u in w.units().player_units(P1) {
            w.cheat_remove_unit(u);
        }
        let _ = run(&mut ai, &mut w, Vec::new(), &mut Vec::new());
        assert!(!ai.tasks(&w).roles.any(Role::Perimeter));
    }

    /// Soldiers already beside a weak stack in the zone attack it.
    #[test]
    fn soldiers_beside_a_weak_stack_in_the_zone_attack_it() {
        let (mut w, _) = besieged(1);
        let beside = spawn(&mut w, P0, at(7, 6), 105 /* high_men_swordsmen */, 3);
        let mut ai = Ai::new(P0);
        let before = w.units().player_units(P1).len();
        let mut events = Vec::new();
        let _ = run(&mut ai, &mut w, beside, &mut events);
        crate::attack::run(&mut ai, &mut w, &mut events);
        assert!(
            w.units().player_units(P1).len() < before,
            "the raider fell: {events:?}"
        );
    }

    /// Soldiers beside the enemy from two squares do not start one battle together (the game
    /// refuses attackers standing apart): the stack that is enough attacks alone.
    #[test]
    fn soldiers_beside_the_enemy_on_two_squares_attack_from_one() {
        let (mut w, _) = besieged(1);
        let mut beside = spawn(&mut w, P0, at(7, 6), 105 /* high_men_swordsmen */, 3);
        beside.extend(spawn(
            &mut w,
            P0,
            at(7, 7),
            105, /* high_men_swordsmen */
            1,
        ));
        let mut ai = Ai::new(P0);
        ai.record_refusals = true;
        let before = w.units().player_units(P1).len();
        let mut events = Vec::new();
        let _ = run(&mut ai, &mut w, beside, &mut events);
        crate::attack::run(&mut ai, &mut w, &mut events);
        assert!(
            ai.refusals.borrow().is_empty(),
            "{:?}",
            ai.refusals.borrow()
        );
        assert!(
            w.units().player_units(P1).len() < before,
            "the raider fell: {events:?}"
        );
    }
}
