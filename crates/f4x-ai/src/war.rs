//! The war job (task 646, `docs/ai-design.md` sections 14, 15 and "Gathering a force"): the target
//! the war plan names, the soldiers it needs, gathered on the way of the one that arrives last, and
//! the march on the square beside the target. How a force gathers is `assembly.rs`'s, the attack is
//! `attack.rs`'s, and `military.rs` keeps what the war jobs share: the stacks, the battle and the
//! strength a defence is expected to have.

use crate::assembly;
use crate::lever::Levered;
use crate::military::{average_strength, stacks};
use crate::roles::{MusterKind, Role};
use crate::view::{distance, fighting_strength, is_fighter, stack_strength, unit_strength};
use crate::{race, Ai};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::unit::travel::{mirror, Travel};
use f4x_core::world::serial::Serial;
use f4x_core::world::{Command, Event};
use f4x_master::Game;
use std::collections::HashMap;

pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let own_cities: Vec<Position> = PlayerView::new(game, me)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .collect();

    // The army's soldiers are the pool's (free, or gathering for the campaign). A unit with a task
    // of another job is not the army's.
    let pool = crate::dispatch::soldiers(ai, game, |roles, s| {
        roles.is_free(s) || roles.in_muster(s, MusterKind::Campaign)
    });

    // The pool is free to campaign.
    let (free, capital) = {
        let view = PlayerView::new(game, me);
        let free: Vec<hecs::Entity> = stacks(&view, pool)
            .into_iter()
            .flat_map(|(_, units)| units)
            .collect();
        let capital = view.fortress().or_else(|| own_cities.first().copied());
        (free, capital)
    };
    let Some(capital) = capital else {
        return;
    };

    choose_target(ai, game, events, capital, &free);
    let Some(target) = ai.campaign else {
        // Nothing to march on: the army raids the sites and towers, which also opens the way to
        // the other plane.
        // A march whose target has fallen is over: its units are available again where they
        // stand, for the dispatcher to garrison or send on.
        ai.write(game)
            .end_musters(crate::roles::MusterKind::Campaign);
        ai.write(game).release_role(crate::roles::Role::March);
        ai.march_target = None;
        return;
    };

    march_on(ai, game, events, &free, capital, target);
}

/// Choose (or keep) the city to attack, in `ai.campaign`: the war plan's, or a city across a
/// plane, or the landing in preparation. What is seen of enemy cities is remembered.
fn choose_target(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    capital: Position,
    free: &[hecs::Entity],
) {
    let me = ai.player;
    let mut new_crossing: Option<Position> = None;
    {
        let view = PlayerView::new(game, me);
        for (_, owner, pos) in view.known_cities() {
            if owner != me && view.sees(pos) {
                let strength = fighting_strength(&view, &view.hostile_at(pos).unwrap_or_default());
                ai.city_memory.insert(pos, (strength, game.round()));
            }
        }
        if ai.campaign.is_some_and(|c| {
            view.city_at(c)
                .and_then(|e| view.city_owner(e))
                .is_none_or(|o| o == me || !crate::attack::may_target(&view, o))
        }) {
            ai.campaign = None;
        }
        // A crossing square that can no longer be used: the campaign across is chosen anew.
        if ai
            .crossing
            .is_some_and(|c| !crate::wizardry::crossing_stands(&view, c))
        {
            ai.campaign = None;
            ai.crossing = None;
        }
        // A campaign against another wizard's city gives way to one against the wizard casting
        // the Spell of Mastery, when a city of theirs can be reached.
        if let (Some(c), Some(f)) = (ai.campaign, race::enemy(ai, &view)) {
            let owner = view.city_at(c).and_then(|e| view.city_owner(e));
            let other = view
                .known_cities()
                .iter()
                .any(|&(_, o, p)| o == f && !ai.unreachable.contains(&p));
            if owner != Some(f) && other {
                ai.campaign = None;
            }
        }
        // The war plan names the town to march on (`war_plan.rs`): it is revised every turn, and
        // keeps its choice until another is clearly better. A campaign the ships or a tower
        // carry (a landing, a crossing) is not the plan's to change.
        if ai.naval.is_none() && ai.crossing.is_none() {
            ai.campaign = None;
        }
        // The towns the army cannot walk to are put aside for the ships.
        while ai.campaign.is_none() {
            let focus = race::enemy(ai, &view);
            let towns = crate::war_plan::towns(
                &view,
                &ai.city_memory,
                game.round(),
                capital,
                &ai.unreachable,
                focus,
            );
            let before = ai.war_plan.clone();
            let revised = crate::war_plan::revise(&mut ai.war_plan, &view, &towns, capital);
            for (old, new, goal) in [
                (before.goal, ai.war_plan.goal, true),
                (before.next, ai.war_plan.next, false),
            ] {
                if old.is_some() && new.is_some() && old != new {
                    events.push(Event::WarPlanChanged { player: me, goal });
                }
            }
            match revised {
                Some(c) => ai.campaign = Some(c),
                None => {
                    // A goal on the other plane is the planar logic's below; one on this plane
                    // that cannot be walked to is left to the ships.
                    let Some(goal) = ai.war_plan.goal.filter(|g| g.plane == capital.plane) else {
                        break;
                    };
                    ai.unreachable.push(goal);
                    ai.unreachable_since = game.round();
                }
            }
        }
        // Nothing to walk to: a rival's city on the other plane, by an open tower. A rival's stack
        // standing in the tower is on both planes: the army meets it on the way and fights it.
        if ai.campaign.is_none() {
            if let Some((tower, city)) = planar_target(
                &view,
                capital,
                &ai.unreachable,
                &ai.city_memory,
                race::enemy(ai, &view),
            ) {
                ai.campaign = Some(city);
                ai.crossing = Some(tower);
                new_crossing = Some(city);
            }
        }
        // No tower to cross by: a wizard that knows Plane Shift or Astral Gate carries the army
        // over, to a rival's city on the other plane.
        if ai.campaign.is_none() {
            let far = mirror(capital);
            if let Some((city, square)) = best_enemy_city(
                &view,
                far,
                &ai.unreachable,
                &ai.city_memory,
                race::enemy(ai, &view),
            )
            .filter(|c| c.plane == far.plane)
            .and_then(|city| Some((city, crate::wizardry::crossing_square(&view, free, city)?)))
            {
                ai.campaign = Some(city);
                ai.crossing = Some(square);
            }
        }
        // A landing in preparation is the campaign.
        if let Some(plan) = ai.naval {
            ai.campaign = Some(plan.target);
        }
        if ai.campaign.is_none_or(|c| c.plane == capital.plane) {
            ai.crossing = None;
        }
    }
    // The crossing is an attack: the war comes first, so that what stands in the way (a rival's
    // unit on a tower) can be attacked. A pact or an alliance is not broken for it: the city is
    // given up.
    if let Some(city) = new_crossing {
        if !crate::naval::declare_war_on_city(ai, game, city, events) {
            ai.campaign = None;
            ai.crossing = None;
            ai.unreachable.push(city);
            ai.unreachable_since = game.round();
        }
    }
}

/// Call a march off when it is time, then gather the soldiers the target needs and march on it
/// when they are enough: over land, or to a landing for the ships (`land_on`).
fn march_on(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    free: &[hecs::Entity],
    capital: Position,
    target: Position,
) {
    let me = ai.player;
    // Targets that could not be reached are tried again after a while.
    if game.round().saturating_sub(ai.unreachable_since) > game.levers().attack.unreachable_turns {
        ai.unreachable.clear();
    }
    // A march that has gone on too long, or whose target has changed, is called off. A far
    // target gets the walk to it on top of the usual time (the slowest unit covers a square
    // a turn), or the stack would turn back before it arrived.
    let trip = {
        let view = PlayerView::new(game, me);
        let from = assembly::rally_of(&view.tasks(), target).unwrap_or(capital);
        distance(&view, from, target).unwrap_or(0)
    };
    let too_long = ai.tasks(game).roles.any(Role::March)
        && march_called_off(
            game.round(),
            ai.march_started,
            trip,
            game.levers().attack.march_turns,
        );
    if too_long || ai.march_target != Some(target) {
        let marching = ai.tasks(game).roles.count(Role::March);
        if marching > 0 {
            events.push(Event::MarchCalledOff {
                player: me,
                target: ai.march_target.unwrap_or(target),
                units: marching as u32,
                turns: game.round().saturating_sub(ai.march_started),
                target_changed: !too_long,
            });
        }
        ai.write(game).release_role(Role::March);
        ai.write(game).end_musters(MusterKind::Campaign);
        ai.march_target = Some(target);
    }
    match ai.naval.filter(|p| p.target == target) {
        Some(plan) => land_on(ai, game, events, free, target, plan.land),
        None => gather(ai, game, events, free, target),
    }
}

/// What the campaign has against its target already, and what it asks for.
struct Need {
    /// The strength and number of the units on the march against the target.
    strength: f32,
    units: usize,
    /// The strength the march waits for.
    wanted: f32,
    /// The strength soldiers are gathered up to.
    gathered: f32,
}

impl Need {
    fn of(ai: &Ai, view: &PlayerView, round: u32, target: Position) -> Self {
        let tasks = view.tasks();
        let marching: Vec<hecs::Entity> = view
            .my_units()
            .into_iter()
            .filter(|&u| tasks.roles.objective_of(view.serial_of(u), Role::March) == Some(target))
            .collect();
        let wanted = crate::war_plan::strength_wanted(view, &ai.city_memory, round, target);
        // In the win phase the wizard sends every free soldier it has, up to a full party:
        // stacks that wait for more than the march needs while the rivals build theirs stall the
        // late game (task 697). The march itself still leaves only when the force is strong
        // enough.
        let gathered = if crate::phase::phase(view, round) == crate::phase::Phase::Win {
            f32::INFINITY
        } else {
            wanted
        };
        Self {
            strength: stack_strength(view, &marching),
            units: marching.len(),
            wanted,
            gathered,
        }
    }

    /// Whether a force of `count` units of `strength`, the marching ones included, may march.
    fn enough(&self, view: &PlayerView, strength: f32, count: usize) -> bool {
        strength >= self.wanted && count >= view.levers().attack.min_march
    }

    /// The force picked by need from `candidates`, which come in the order they are wanted:
    /// until the strength to gather and a march's units are there, at most what a battle holds.
    fn pick(&self, view: &PlayerView, candidates: &[hecs::Entity]) -> Vec<hecs::Entity> {
        let (mut strength, mut count) = (self.strength, self.units);
        let mut picked = Vec::new();
        for &u in candidates {
            if (strength >= self.gathered && count >= view.levers().attack.min_march)
                || count >= view.max_battle_side()
            {
                break;
            }
            strength += unit_strength(view, u);
            count += 1;
            picked.push(u);
        }
        picked
    }
}

/// The soldiers of the campaign's muster come first (they are not swapped for others every turn),
/// then those that are there soonest.
fn by_arrival(view: &PlayerView, mut timed: Vec<(u32, hecs::Entity)>) -> Vec<hecs::Entity> {
    let tasks = view.tasks();
    timed.sort_by_key(|&(turns, u)| {
        let serial = view.serial_of(u);
        let member = tasks.roles.in_muster(serial, MusterKind::Campaign);
        (!member, turns, serial)
    });
    timed.into_iter().map(|(_, u)| u).collect()
}

/// The army over land, a tower or a gate: the soldiers that are there soonest gather at the rally
/// point, and march on the square beside the target when what is there and on the march is enough.
fn gather(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    free: &[hecs::Entity],
    target: Position,
) {
    let me = ai.player;
    // The units on the march that came beside the target on another square join the others.
    let stragglers: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        view.my_units()
            .into_iter()
            .filter(|&u| tasks.roles.objective_of(view.serial_of(u), Role::March) == Some(target))
            .filter(|&u| !view.unit_has_order(u))
            .filter(|&u| {
                view.unit_position(u)
                    .is_some_and(|p| distance(&view, p, target) == Some(1))
            })
            .collect()
    };
    if !stragglers.is_empty() {
        // One that cannot get to the square attacks from where it is, when that is enough.
        let _ = assembly::march(ai, game, Role::March, &stragglers, target, events);
    }
    let mut candidates: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let tasks = view.tasks();
        free.iter()
            .copied()
            .filter(|&u| !tasks.roles.is(view.serial_of(u), Role::March))
            .collect()
    };
    let crossing = ai.crossing.filter(|c| c.plane != target.plane);
    if let Some(square) = crossing {
        candidates = cross(ai, game, events, &candidates, target, square);
    }
    let mut travel = Travel::new(game, ai.player, ai.crossing);
    let (need, force, none_gets_there) = {
        let view = PlayerView::new(game, me);
        let need = Need::of(ai, &view, game.round(), target);
        let timed: Vec<(u32, hecs::Entity)> = candidates
            .iter()
            .filter_map(|&u| Some((travel.trip(&view, &[u], target)?.turns, u)))
            .collect();
        let min = view.levers().attack.min_march;
        let none_gets_there = timed.is_empty() && candidates.len() >= min;
        // Nothing is sent while too few units are free to make a march at all.
        let force = if timed.len() + need.units < min {
            Vec::new()
        } else {
            need.pick(&view, &by_arrival(&view, timed))
        };
        (need, force, none_gets_there)
    };
    if force.is_empty() {
        if none_gets_there && need.units == 0 {
            give_up(ai, game, target);
        }
        return;
    }
    let Ok(beside) = assembly::beside(ai, game, &mut travel, target, &force) else {
        give_up(ai, game, target);
        return;
    };
    // Those beside the target are there; the others gather on the rally point, or go straight
    // to the square beside the target when no square serves as one.
    let (near, away): (Vec<hecs::Entity>, Vec<hecs::Entity>) = {
        let view = PlayerView::new(game, me);
        force.iter().copied().partition(|&u| {
            view.unit_position(u)
                .is_some_and(|p| distance(&view, p, target) == Some(1))
        })
    };
    let point = assembly::rally(ai, game, &mut travel, target, beside, &away).unwrap_or(beside);
    let crossing_and_point: Vec<Position> = [Some(point), crossing].into_iter().flatten().collect();
    // Those beside the target are on the march at once, from the square beside it.
    if !near.is_empty() && !ai.tasks(game).roles.any(Role::March) {
        ai.march_started = game.round();
    }
    let (_, staged) = assembly::take(ai, game, Role::March, &near, beside, target);
    for u in staged {
        let (at, ordered) = {
            let view = PlayerView::new(game, me);
            (view.unit_position(u), view.unit_has_order(u))
        };
        if at != Some(beside) && !ordered {
            // One that cannot get to the square attacks from where it is, when that is enough.
            let _ = assembly::send(ai, game, &[u], beside, events);
        }
    }
    for &u in &away {
        muster_and_walk(ai, game, events, u, (target, point, &crossing_and_point));
    }
    assembly::campaign_muster(ai, game, target, point, &crossing_and_point, &[]);
    let ready: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let mut ready: Vec<hecs::Entity> = force
            .iter()
            .copied()
            .filter(|&u| near.contains(&u) || view.unit_position(u) == Some(point))
            .collect();
        let strength = need.strength + stack_strength(&view, &ready);
        if !need.enough(&view, strength, need.units + ready.len()) {
            return;
        }
        view.sort_by_serial(&mut ready);
        ready
    };
    set_out(ai, game, events, &ready, target);
}

/// The units march on the square beside the target.
fn set_out(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    units: &[hecs::Entity],
    target: Position,
) {
    let me = ai.player;
    match assembly::march(ai, game, Role::March, units, target, events) {
        Ok(()) => {}
        // A square that is full for now: the next turn tries again.
        Err(assembly::MarchError::Refused(_)) => return,
        Err(_) => {
            give_up(ai, game, target);
            return;
        }
    }
    events.push(Event::MarchLaunched {
        player: me,
        target,
        units: units.len() as u32,
    });
    ai.write(game).end_musters(MusterKind::Campaign);
    ai.march_started = game.round();
}

/// Make the unit part of the campaign's muster at `rally` (`rallies` are the ones that stand) and
/// walk it there, when it is not there and has no walk. One that cannot set out does not join.
/// Returns why it did not.
fn muster_and_walk(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    unit: hecs::Entity,
    (target, rally, rallies): (Position, Position, &[Position]),
) -> Option<assembly::MarchError> {
    let (serial, at, ordered) = {
        let view = PlayerView::new(game, ai.player);
        (
            view.serial_of(unit),
            view.unit_position(unit),
            view.unit_has_order(unit),
        )
    };
    let member = ai.tasks(game).roles.in_muster(serial, MusterKind::Campaign);
    assembly::campaign_muster(ai, game, target, rally, rallies, &[serial]);
    if at == Some(rally) || ordered || !ai.tasks(game).roles.is(serial, Role::Muster) {
        return None;
    }
    let why = assembly::send(ai, game, &[unit], rally, events).err()?;
    if !member {
        ai.write(game).release(serial, Role::Muster);
    }
    Some(why)
}

/// No way leads to the target: it is put aside for a while and the army goes for another city
/// (ships or a gate may open the way later).
fn give_up(ai: &mut Ai, game: &Game, target: Position) {
    ai.unreachable.push(target);
    ai.unreachable_since = game.round();
    ai.campaign = None;
}

/// The soldiers on the near side of the campaign's crossing walk to it and cross: through a
/// tower or an Astral Gate in parties, or with the Plane Shift cast on them there
/// (`wizardry::shift_target`). Returns the soldiers on the target's side, those that have just
/// crossed among them.
fn cross(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    candidates: &[hecs::Entity],
    target: Position,
    square: Position,
) -> Vec<hecs::Entity> {
    let me = ai.player;
    let near: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        candidates
            .iter()
            .copied()
            .filter(|&u| {
                view.unit_position(u)
                    .is_some_and(|p| p.plane == square.plane)
            })
            .collect()
    };
    let side = game.max_battle_side();
    let mut there: Vec<hecs::Entity> = Vec::new();
    let mut members: Vec<Serial> = Vec::new();
    // Set when the square refused a unit for want of room: no other unit is sent this turn (a
    // full square stays full, and each refusal was one more ignored order).
    let mut full = false;
    for &u in &near {
        let (serial, at, ordered, by_itself) = {
            let view = PlayerView::new(game, me);
            (
                view.serial_of(u),
                view.unit_position(u),
                view.unit_has_order(u),
                view.unit_has_flag(u, f4x_def::rules::unit_flag::UnitFlag::PlanarTravel)
                    && view.can_shift_plane(&[u]),
            )
        };
        // A unit that travels between the planes by itself needs no crossing square.
        if by_itself {
            ai.order(game, Command::ShiftPlane { units: vec![u] }, events);
            continue;
        }
        if at == Some(square) {
            there.push(u);
            members.push(serial);
        } else if ordered {
            if ai.tasks(game).roles.in_muster(serial, MusterKind::Campaign) {
                members.push(serial);
            }
        } else if !full && members.len() < side && Ai::room_at(game, square, 1) {
            let point = assembly::rally_of(&ai.tasks(game), target);
            let rallies: Vec<Position> = [Some(square), point].into_iter().flatten().collect();
            let walk = (target, square, rallies.as_slice());
            match muster_and_walk(ai, game, events, u, walk) {
                None => members.push(serial),
                Some(why) => {
                    full = why
                        == assembly::MarchError::Refused(
                            f4x_core::unit::movement::MoveError::StackFull,
                        );
                }
            }
        }
    }
    let point = assembly::rally_of(&ai.tasks(game), target);
    let rallies: Vec<Position> = [Some(square), point].into_iter().flatten().collect();
    assembly::campaign_muster(ai, game, target, square, &rallies, &members);
    // The troops cross in stacks of what a battle holds.
    let min = game.levers().attack.min_crossing.min(near.len());
    for chunk in there.chunks(side) {
        if chunk.len() < min || !PlayerView::new(game, me).can_shift_plane(chunk) {
            continue;
        }
        ai.order(
            game,
            Command::ShiftPlane {
                units: chunk.to_vec(),
            },
            events,
        );
    }
    let view = PlayerView::new(game, me);
    candidates
        .iter()
        .copied()
        .filter(|&u| {
            view.unit_position(u)
                .is_some_and(|p| p.plane == target.plane)
        })
        .collect()
}

/// The army across the sea: the landing site `land` is where it gathers and where it attacks
/// from. When the soldiers it can send are enough, war is declared (a pact or an alliance is not
/// broken for a landing: the city is given up) and they gather there, those that cannot walk
/// tagged for a ship (`pickup.rs`). What has landed marches when it is enough.
fn land_on(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    free: &[hecs::Entity],
    target: Position,
    land: Position,
) {
    let me = ai.player;
    let (need, force, begun) = {
        let view = PlayerView::new(game, me);
        let need = Need::of(ai, &view, game.round(), target);
        let tasks = view.tasks();
        let timed: Vec<(u32, hecs::Entity)> = free
            .iter()
            .copied()
            .filter(|&u| !tasks.roles.is(view.serial_of(u), Role::March))
            .filter_map(|u| {
                let p = view.unit_position(u)?;
                Some((distance(&view, p, land)?, u))
            })
            .collect();
        let force = need.pick(&view, &by_arrival(&view, timed));
        let begun = need.units > 0
            || tasks.roles.musters().iter().any(|(_, m, _)| {
                m.kind == MusterKind::Campaign && m.objective == Some(target) && m.rally == land
            });
        let strength = need.strength + stack_strength(&view, &force);
        if force.is_empty() || !(begun || need.enough(&view, strength, need.units + force.len())) {
            return;
        }
        (need, force, begun)
    };
    if !crate::naval::declare_war_on_city(ai, game, target, events) {
        ai.naval_bad.push(target);
        ai.naval = None;
        ai.campaign = None;
        return;
    }
    let beside_target = distance(&PlayerView::new(game, me), land, target) == Some(1);
    if beside_target {
        assembly::set_beside(ai, game, target, land);
    }
    let members: Vec<Serial> = {
        let view = PlayerView::new(game, me);
        force.iter().map(|&u| view.serial_of(u)).collect()
    };
    assembly::campaign_muster(ai, game, target, land, &[land], &members);
    if !begun {
        events.push(Event::MarchLaunched {
            player: me,
            target,
            units: members.len() as u32,
        });
    }
    let mut travel = Travel::new(game, ai.player, ai.crossing);
    for (&u, &serial) in force.iter().zip(&members) {
        let (at, ordered, walker, walks) = {
            let view = PlayerView::new(game, me);
            (
                view.unit_position(u),
                view.unit_has_order(u),
                view.is_walker(u),
                travel.trip(&view, &[u], land).is_some(),
            )
        };
        if at == Some(land) {
            ai.write(game).set_crossing(serial, None);
        } else if !walks && walker {
            // Only walkers wait for the ships; the fliers and swimmers cross by themselves.
            ai.write(game)
                .set_crossing(serial, Some(crate::roles::Crossing::Sea));
        } else if !ordered {
            // A square that is full for now: the next turn tries again.
            if ai.tasks(game).roles.is(serial, Role::Muster) {
                let _ = assembly::send(ai, game, &[u], land, events);
            }
        }
    }
    // What has landed marches when it is enough: from the landing site, or on to the square
    // beside the target.
    let ashore: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, me);
        let ashore: Vec<hecs::Entity> = force
            .iter()
            .copied()
            .filter(|&u| {
                view.unit_position(u)
                    .is_some_and(|p| p == land || distance(&view, p, target) == Some(1))
            })
            .collect();
        let strength = need.strength + stack_strength(&view, &ashore);
        if !need.enough(&view, strength, need.units + ashore.len()) {
            return;
        }
        ashore
    };
    set_out(ai, game, events, &ashore, target);
}

/// Has a march gone on too long? It has `march_turns` and the walk to a far target on top (the
/// slowest unit covers a square a turn), so a stack bound for a distant city is not called back
/// on the way.
fn march_called_off(now: u32, started: u32, trip: u32, march_turns: u32) -> bool {
    now.saturating_sub(started) > march_turns + trip
}

/// The towers the wizard has cleared, on the plane of its capital, that its troops can walk to,
/// the nearest first.
fn open_towers(view: &PlayerView, capital: Position) -> Vec<Position> {
    let Some(walker) = view
        .garrison(capital)
        .into_iter()
        .find(|&u| is_fighter(view, u))
    else {
        return Vec::new();
    };
    let mut towers: Vec<Position> = view
        .towers()
        .into_iter()
        .filter(|&(p, intact)| !intact && p.plane == capital.plane && view.can_reach(&[walker], p))
        .map(|(p, _)| p)
        .collect();
    towers.sort_by_key(|&p| (distance(view, capital, p), p.y, p.x));
    towers
}

/// A rival's city on the other plane worth crossing for, and the open tower to cross by: the
/// nearest tower from whose far side a city can be walked to.
fn planar_target(
    view: &PlayerView,
    capital: Position,
    skip: &[Position],
    memory: &HashMap<Position, (f32, u32)>,
    focus: Option<PlayerId>,
) -> Option<(Position, Position)> {
    let towers = view.towers();
    open_towers(view, capital).into_iter().find_map(|tower| {
        // The far side of the tower must be seen, or the army would cross blind.
        let far = towers
            .iter()
            .map(|&(p, _)| p)
            .find(|p| p.plane != tower.plane && (p.x, p.y) == (tower.x, tower.y))?;
        let city = best_enemy_city(view, far, skip, memory, focus)?;
        Some((tower, city))
    })
}

/// What a city is worth beyond its distance and its garrison (task 502): the rival's fortress city
/// first, then the cities that lie between the army and that fortress. The nearest known fortress
/// city of the same owner is the one the road leads to.
fn strategic_bonus(view: &PlayerView, from: Position, owner: PlayerId, city: Position) -> f32 {
    if !view.is_rival_wizard(owner) {
        return 0.0;
    }
    let known = view.known_cities();
    let Some(&(_, _, fort)) = known
        .iter()
        .find(|&&(c, o, _)| o == owner && view.is_fortress_city(c))
    else {
        return 0.0;
    };
    if fort == city {
        return view.levers().target.fortress_bonus;
    }
    let (Some(direct), Some(to_city), Some(on)) = (
        distance(view, from, fort),
        distance(view, from, city),
        distance(view, city, fort),
    ) else {
        return 0.0;
    };
    if to_city + on <= direct + view.levers().target.route_slack && to_city < direct {
        view.levers().target.route_bonus
    } else {
        0.0
    }
}

/// The enemy city worth marching on: near, and lightly held as far as the AI knows. Each average
/// unit of garrison counts as a few squares of walking.
fn best_enemy_city(
    view: &PlayerView,
    from: Position,
    skip: &[Position],
    memory: &HashMap<Position, (f32, u32)>,
    focus: Option<PlayerId>,
) -> Option<Position> {
    let average = average_strength(view).max(1.0);
    let cities = view.known_cities();
    // A wizard about to win with the Spell of Mastery comes first, if any of its cities is known.
    let focus = focus.filter(|&f| {
        cities
            .iter()
            .any(|&(_, owner, p)| owner == f && !skip.contains(&p))
    });
    cities
        .into_iter()
        .filter(|(_, owner, p)| !skip.contains(p) && focus.is_none_or(|f| *owner == f))
        // Not the cities it may not attack: a pact, an alliance or a peace period.
        .filter(|&(_, owner, _)| crate::attack::may_target(view, owner))
        .filter_map(|(_, owner, p)| {
            let d = distance(view, from, p)? as f32;
            let guess = view.levers().attack.guess;
            let held = memory.get(&p).map_or(guess, |m| m.0 / average);
            Some((
                d + view.levers().target.defence_squares * held
                    - strategic_bonus(view, from, owner, p),
                p,
            ))
        })
        .min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1.y, a.1.x).cmp(&(b.1.y, b.1.x)))
        })
        .map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn min_march() -> usize {
        crate::testworld::shipped()
            .rules
            .ai_strategy
            .attack
            .min_march
    }

    fn march_turns() -> u32 {
        crate::testworld::shipped()
            .rules
            .ai_strategy
            .attack
            .march_turns
    }
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::world::World;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// P1 has its capital in the west of the east island and a second city, its seat, at `seat`;
    /// P0 sees both and is at war with it.
    fn rival_with_a_seat(seat: (u32, u32)) -> Result<Game, Box<dyn std::error::Error>> {
        use f4x_core::wizard::fortress::Fortress;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let capital = at(
            crate::testworld::ENEMY_CITY.0,
            crate::testworld::ENEMY_CITY.1,
        );
        let events = w.command(Command::FoundCity {
            owner: P1,
            pos: at(seat.0, seat.1),
            name: "Seat".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        w.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .put(Some(Fortress(at(seat.0, seat.1))));
        for p in [capital, at(seat.0, seat.1)] {
            w.explore(P0, p, HalfInt::from_raw(3));
        }
        w.declare_war(P0, P1);
        Ok(w)
    }

    #[test]
    fn the_rivals_fortress_is_taken_before_a_nearer_city() -> TestResult {
        let w = rival_with_a_seat((30, 10))?;
        let here = at(COAST_CITY.0, COAST_CITY.1);
        let pick = best_enemy_city(&PlayerView::new(&w, P0), here, &[], &HashMap::new(), None);
        // The capital is nearer (18 squares, the seat 20) but is not the seat.
        assert_eq!(pick, Some(at(30, 10)));
        Ok(())
    }

    #[test]
    fn a_wizard_with_a_pact_is_not_marched_on_but_one_at_war_is() {
        use f4x_core::diplomacy::Treaty;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let east = at(
            crate::testworld::ENEMY_CITY.0,
            crate::testworld::ENEMY_CITY.1,
        );
        w.explore(P0, east, HalfInt::from_raw(3));
        let here = at(COAST_CITY.0, COAST_CITY.1);
        let pick =
            |w: &World| best_enemy_city(&PlayerView::new(w, P0), here, &[], &HashMap::new(), None);
        assert_eq!(pick(&w), Some(east), "at peace the city is a target");
        w.start_treaty(P0, P1, Treaty::Pact);
        assert_eq!(pick(&w), None, "a pact partner is left alone");
        w.start_treaty(P0, P1, Treaty::None);
        w.declare_war(P0, P1);
        assert_eq!(pick(&w), Some(east));
    }

    /// How many of the units the campaign has taken: gathering or on the march.
    fn taken(ai: &Ai, w: &World, units: &[hecs::Entity]) -> usize {
        use crate::roles::Role;
        let view = PlayerView::new(w, P0);
        units
            .iter()
            .filter(|&&u| {
                let s = view.serial_of(u);
                ai.tasks(w).roles.is(s, Role::Muster) || ai.tasks(w).roles.is(s, Role::March)
            })
            .count()
    }

    /// The same soldiers, the same target: a wizard that wins the game sends them all, one that
    /// builds up only the few the march needs (task 697).
    #[test]
    fn in_the_win_phase_every_free_soldier_is_sent_and_early_only_those_needed() -> TestResult {
        let sent = |turn: u32| -> Result<usize, Box<dyn std::error::Error>> {
            let mut w = rival_with_a_seat((4, 4))?;
            w.set_round(turn);
            let troops = spawn(&mut w, P0, at(5, 9), 113 /* high_men_paladins */, 9);
            w.reveal_map(P0);
            let mut ai = Ai::new(P0);
            crate::roles::register(&mut ai, &mut w);
            run(&mut ai, &mut w, &mut Vec::new());
            Ok(taken(&ai, &w, &troops))
        };
        let (early, late) = (sent(1)?, sent(400)?);
        assert_eq!(early, min_march());
        assert!(late > early, "{late} against {early}");
        Ok(())
    }

    #[test]
    fn soldiers_gathering_for_the_campaign_hold_a_muster_until_the_march_leaves() -> TestResult {
        use crate::roles::Role;
        // A rival city on the west island, and soldiers apart from each other.
        let mut w = rival_with_a_seat((4, 4))?;
        let target = at(4, 4);
        let mut troops = Vec::new();
        for p in [at(9, 9), at(9, 3), at(7, 10), at(10, 9), at(10, 3)] {
            troops.extend(spawn(&mut w, P0, p, 113 /* high_men_paladins */, 1));
        }
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.campaign, Some(target));
        let musters = ai.tasks(&w).roles.musters();
        assert_eq!(musters.len(), 1, "one muster");
        let (_, muster, members) = &musters[0];
        assert_eq!(muster.kind, MusterKind::Campaign);
        assert_eq!(muster.objective, Some(target));
        assert_eq!(
            Some(muster.rally),
            crate::assembly::rally_of(&ai.tasks(&w), target),
            "the rally point"
        );
        // Only the units the march needs are taken; the others stay where they are, available.
        assert_eq!(members.len(), min_march(), "{members:?}");
        let view = PlayerView::new(&w, P0);
        assert!(view.city_at(muster.rally).is_none(), "no city: {muster:?}");
        let (sent, left): (Vec<_>, Vec<_>) = troops
            .iter()
            .copied()
            .partition(|&u| ai.tasks(&w).roles.is(view.serial_of(u), Role::Muster));
        assert_eq!(sent.len(), min_march());
        for u in left {
            assert!(!view.unit_has_order(u), "left alone");
        }
        // They arrive and are strong enough: the march leaves and the muster is over.
        let rally = muster.rally;
        for &u in &sent {
            w.cheat_move_unit(u, rally);
        }
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(
            ai.tasks(&w).roles.musters().is_empty(),
            "the muster ended: {:?} {:?}",
            ai.tasks(&w).roles.musters(),
            ai.campaign
        );
        assert!(ai.tasks(&w).roles.any(Role::March), "the march left");
        Ok(())
    }

    /// A force that stands together needs no rally point of its own: it marches at once.
    #[test]
    fn a_force_that_stands_together_marches_at_once() -> TestResult {
        use crate::roles::Role;
        let mut w = rival_with_a_seat((4, 4))?;
        let troops = spawn(&mut w, P0, at(8, 9), 113 /* high_men_paladins */, 3);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let view = PlayerView::new(&w, P0);
        assert!(
            troops.iter().all(|&u| ai
                .tasks(&w)
                .roles
                .objective_of(view.serial_of(u), Role::March)
                == Some(at(4, 4))),
            "{events:?}"
        );
        Ok(())
    }

    /// A force beside its target that is too weak gets more soldiers: they march to its square.
    #[test]
    fn a_weak_force_at_the_target_gets_more_units() -> TestResult {
        use crate::roles::Role;
        let mut w = rival_with_a_seat((4, 4))?;
        let target = at(4, 4);
        spawn(&mut w, P1, target, 113 /* high_men_paladins */, 3);
        let front = at(5, 5);
        let weak = spawn(&mut w, P0, front, 104 /* high_men_spearmen */, 1);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        let marching = PlayerView::new(&w, P0).serial_of(weak[0]);
        let _ = ai.write(&mut w).claim(marching, Role::March, Some(target));
        ai.march_target = Some(target);
        ai.march_started = w.round();
        crate::assembly::set_beside(&ai, &mut w, target, front);
        let more = spawn(&mut w, P0, at(8, 9), 113 /* high_men_paladins */, 8);
        crate::roles::register(&mut ai, &mut w);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let view = PlayerView::new(&w, P0);
        let sent = more
            .iter()
            .filter(|&&u| {
                ai.tasks(&w)
                    .roles
                    .objective_of(view.serial_of(u), Role::March)
                    == Some(target)
            })
            .count();
        assert!(sent > 0, "{events:?} {:?}", ai.tasks(&w).roles);
        assert!(sent < w.max_battle_side(), "one battle party");
        Ok(())
    }

    /// Units that need rest are not mustered: they keep no role and no order while the healthy
    /// ones of the same square are sent.
    #[test]
    fn hurt_soldiers_are_kept_out_of_the_muster() -> TestResult {
        use crate::roles::Role;
        let mut w = rival_with_a_seat((4, 4))?;
        let troops = spawn(&mut w, P0, at(5, 9), 113 /* high_men_paladins */, 6);
        let (hurt, healthy) = troops.split_at(3);
        for &u in hurt {
            let (_, max) = PlayerView::new(&w, P0).unit_hits(u);
            w.scenario_set_hurt(u, max * 4 / 5); // damage taken
        }
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        let view = PlayerView::new(&w, P0);
        for &u in hurt {
            assert!(crate::recover::needs_rest(&view, u));
            assert!(
                !ai.tasks(&w).roles.is(view.serial_of(u), Role::Muster),
                "hurt unit mustered"
            );
            assert!(
                !ai.tasks(&w).roles.is(view.serial_of(u), Role::March),
                "hurt unit marched"
            );
        }
        assert_eq!(
            taken(&ai, &w, healthy),
            min_march(),
            "the healthy ones are sent"
        );
        Ok(())
    }

    /// When the march's target falls, its units stay where they stand with no role and no order:
    /// the dispatcher decides what they do next (garrison, a new target).
    #[test]
    fn after_a_capture_the_marching_units_stay_put_and_are_available() -> TestResult {
        use crate::roles::Role;
        // P1's second city is the only one P0 knows, and it is not a fortress: taking it does
        // not end the game.
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.make_contact(P0, P1);
        let town = at(4, 4);
        let events = w.command(Command::FoundCity {
            owner: P1,
            pos: town,
            name: "Town".into(),
            race: w.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        w.explore(P0, town, HalfInt::from_raw(3));
        w.declare_war(P0, P1);
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let tile = w.max_tile();
        let troops = spawn(&mut w, P0, capital, 113 /* high_men_paladins */, tile);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_eq!(ai.campaign, Some(town));
        assert!(ai.tasks(&w).roles.any(Role::March), "the march left");
        let city = w
            .economy()
            .cities_of(P1)
            .into_iter()
            .find(|&(_, p)| p == town)
            .ok_or("the town is a city of P1")?
            .0;
        w.conquer_city(city, P0, 0);
        w.explore(P0, town, HalfInt::from_raw(3));
        let before: Vec<_> = {
            let view = PlayerView::new(&w, P0);
            troops.iter().map(|&u| view.unit_position(u)).collect()
        };
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert_ne!(ai.campaign, Some(town), "the taken town is no target");
        assert!(!ai.tasks(&w).roles.any(Role::March), "the march is over");
        assert!(ai.tasks(&w).roles.musters().is_empty());
        // Nothing sends the soldiers anywhere: they stay where they are with no task, for the
        // dispatcher to keep as a garrison or give a new target.
        let view = PlayerView::new(&w, P0);
        for (&u, was) in troops.iter().zip(before) {
            assert_eq!(view.unit_position(u), was, "stays where it stands");
            assert!(
                !ai.tasks(&w).roles.has_role(view.serial_of(u)),
                "no task: {:?}",
                ai.tasks(&w).roles.role_of(view.serial_of(u))
            );
        }
        Ok(())
    }

    /// West's city holds one spearman; four of East's swordsmen stand two squares away and three
    /// of West's own wait four squares from the city.
    fn besieged(war: bool) -> (Game, Vec<hecs::Entity>) {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut w, P0, at(10, 6), 104 /* high_men_spearmen */, 1);
        spawn(&mut w, P1, at(8, 6), 105 /* high_men_swordsmen */, 4);
        let spare = spawn(&mut w, P0, at(6, 3), 105 /* high_men_swordsmen */, 3);
        w.make_contact(P0, P1);
        if war {
            w.declare_war(P0, P1);
        }
        (w, spare)
    }

    #[test]
    fn a_city_with_a_stronger_enemy_stack_near_calls_in_the_troops_and_asks_for_more() {
        let (mut w, spare) = besieged(true);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        // The dispatcher's job 0 calls the troops in; the army follows.
        crate::dispatch::run(&mut ai, &mut w, &mut events);
        assert_eq!(ai.threatened, vec![at(10, 6)], "{events:?}");
        let view = PlayerView::new(&w, P0);
        assert!(
            spare.iter().all(|&u| view.unit_has_order(u)),
            "the spare troops are on their way: {events:?}"
        );
    }

    #[test]
    fn at_peace_the_same_stack_is_no_threat() {
        let (mut w, spare) = besieged(false);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert!(ai.threatened.is_empty());
        // No relief is called in: the spare troops are no part of a march or a raid. (Idle
        // soldiers outside a city go home, which is no relief either.)
        let _ = spare;
        assert!(
            !ai.tasks(&w).roles.any(crate::roles::Role::March)
                && !ai.tasks(&w).roles.any(crate::roles::Role::Raid)
        );
    }

    #[test]
    fn the_force_wanted_grows_with_the_garrison_the_ai_remembers_in_the_target() {
        let (w, _) = besieged(true);
        let view = PlayerView::new(&w, P0);
        let target = at(2, 12);
        let none = HashMap::new();
        let weak = HashMap::from([(target, (1.0, 1))]);
        let strong = HashMap::from([(target, (average_strength(&view) * 9.0, 1))]);
        let wanted = |m: &HashMap<_, _>| {
            crate::war_plan::units_for(&view, crate::war_plan::strength_wanted(&view, m, 2, target))
        };
        let least = view.levers().attack.min_march;
        assert_eq!(wanted(&weak), least, "the fewest a march starts with");
        assert!(wanted(&none) >= least);
        assert!(
            wanted(&strong) > wanted(&none),
            "{} {}",
            wanted(&strong),
            wanted(&none)
        );
        assert!(
            wanted(&strong)
                <= crate::testworld::shipped()
                    .rules
                    .ai_strategy
                    .target
                    .max_force
        );
        // The army attacks from one square: no more is wanted than a square holds.
        assert!(wanted(&strong) <= view.max_battle_side());
    }

    /// The march is one stack on one square: the strongest units of the rally square up to what a
    /// square holds; a unit standing beside the rally is not counted as gathered, and what is
    /// left over keeps no order.
    #[test]
    fn the_march_is_one_stack_from_the_rally_square_and_the_rest_stay_available() -> TestResult {
        use crate::roles::Role;
        let mut w = rival_with_a_seat((4, 4))?;
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let (max, tile) = (w.max_battle_side(), w.max_tile());
        let stack = spawn(&mut w, P0, capital, 113 /* high_men_paladins */, tile);
        let beside = spawn(&mut w, P0, at(COAST_CITY.0 - 1, COAST_CITY.1), 113, 2);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        crate::roles::register(&mut ai, &mut w);
        let view = PlayerView::new(&w, P0);
        let marching: Vec<_> = stack
            .iter()
            .chain(&beside)
            .copied()
            .filter(|&u| ai.tasks(&w).roles.is(view.serial_of(u), Role::March))
            .collect();
        assert!(!marching.is_empty(), "the march left");
        assert!(marching.len() <= max);
        // The registry knows where the march goes, so a lost walk can be given again.
        let ahead = ai.campaign;
        assert!(ahead.is_some());
        assert!(marching.iter().all(|&u| ai
            .tasks(&w)
            .roles
            .objective_of(view.serial_of(u), Role::March)
            == ahead));
        // The task's own square is the one the force attacks from.
        assert!(marching.iter().all(|&u| ai
            .tasks(&w)
            .roles
            .target_of(view.serial_of(u), Role::March)
            .zip(ahead)
            .is_some_and(|(from, target)| view.distance(from, target) == Some(1))));
        let squares: std::collections::HashSet<_> = marching
            .iter()
            .filter_map(|&u| view.unit_destination(u))
            .collect();
        assert_eq!(squares.len(), 1, "one square: {squares:?}");
        assert!(
            marching.iter().all(|u| stack.contains(u)),
            "only the rally square's units march"
        );
        Ok(())
    }

    #[test]
    fn an_army_with_nothing_to_walk_to_crosses_by_an_open_tower_to_a_city_on_the_other_plane(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::testworld::spawn_tower;
        use f4x_core::map::Plane;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        w.command(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [tower, on_myrror(tower), far_city, at(28, 6)] {
            w.explore(P0, p, HalfInt::from_raw(3));
        }
        let troops = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* spearmen */
            6,
        );
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert_eq!(ai.crossing, Some(tower), "{events:?}");
        assert_eq!(ai.campaign, Some(far_city));
        assert!(
            troops
                .iter()
                .any(|&u| PlayerView::new(&w, P0).unit_has_order(u)),
            "the troops walk to the tower"
        );
        // Standing on the tower they cross.
        let party = spawn(&mut w, P0, tower, 104 /* spearmen */, 4);
        run(&mut ai, &mut w, &mut events);
        assert!(
            party
                .iter()
                .any(|&u| w.units().unit_position(u).map(|p| p.plane) == Some(Plane::Myrror)),
            "{events:?}"
        );
        Ok(())
    }

    /// A crossing is an attack on the wizard whose city is the target: the war is declared before
    /// the army goes, so that what stands in its way can be attacked.
    #[test]
    fn a_crossing_to_a_rivals_city_declares_war_first() -> Result<(), Box<dyn std::error::Error>> {
        use crate::testworld::spawn_tower;
        use f4x_core::map::Plane;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut w, tower, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        w.command(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [tower, on_myrror(tower), far_city, at(28, 6)] {
            w.explore(P0, p, HalfInt::from_raw(3));
        }
        spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 6);
        w.make_contact(P0, P1);
        assert_ne!(w.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert_eq!(ai.crossing, Some(tower), "{events:?}");
        assert_eq!(w.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
        Ok(())
    }

    /// A rival's stack standing in the nearest tower is on both planes (the original's
    /// `in_tower`): the army does not set the tower aside, it walks onto it and the move is an
    /// attack on the holder.
    #[test]
    fn a_tower_held_by_a_rival_is_walked_onto_and_fought() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::testworld::spawn_tower;
        use f4x_core::map::Plane;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let near = at(8, 6);
        spawn_tower(&mut w, near, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        w.command(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [near, on_myrror(near), far_city, at(28, 6)] {
            w.explore(P0, p, HalfInt::from_raw(3));
        }
        // The rival's unit stands in the tower, on the far plane's side of it.
        let holder = spawn(&mut w, P1, on_myrror(near), 104 /* spearmen */, 1);
        let troops = spawn(&mut w, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 6);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut events);
        assert_eq!(
            ai.crossing,
            Some(near),
            "the tower is not set aside: {events:?}"
        );
        // The way onto the tower from our side ends in the holder.
        let mover = spawn(&mut w, P0, at(7, 6), 104 /* spearmen */, 1)[0];
        assert!(!troops.is_empty());
        match w.economy().plan_step(&[mover], near) {
            Ok((_, f4x_core::unit::movement::Entry::Hostile(found))) => assert_eq!(found, holder),
            other => panic!("the holder is not met: {other:?}"),
        }
        Ok(())
    }

    /// Units that travel between the planes themselves need no tower and no spell: when the
    /// campaign is on the other plane (set by a means the plan knows), they get across with no
    /// order of the war job's own and no spell cast for them.
    #[test]
    fn shadow_demons_shift_to_the_plane_of_the_campaign_by_themselves(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use f4x_core::map::Plane;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let far_city = Position {
            plane: Plane::Myrror,
            ..at(28, 7)
        };
        w.command(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        w.explore(P0, far_city, HalfInt::from_raw(3));
        // The campaign is set by a means the plan knows (Plane Shift); the demons need none of it.
        w.cheat_learn_spell(P0, f4x_def::rules::SpellId::new(132 /* plane_shift */));
        w.treasury_mut(P0).ok_or("a treasury")?.mana = HalfInt::from_int(5_000);
        w.wizard_mut(P0)
            .ok_or("a wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        let demons = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            171, /* shadow_demons */
            12,
        );
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        let shifted = demons
            .iter()
            .filter(|&&u| w.units().unit_position(u).map(|p| p.plane) == Some(Plane::Myrror))
            .count();
        assert!(shifted > 0, "{events:?} {:?}", ai.campaign);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, f4x_core::world::Event::PlaneShifted { .. })),
            "no spell was cast: {events:?}"
        );
        Ok(())
    }

    #[test]
    fn a_wizard_with_plane_shift_carries_the_army_to_a_city_on_the_other_plane(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use f4x_core::map::Plane;
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let far_city = Position {
            plane: Plane::Myrror,
            ..at(28, 7)
        };
        w.command(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        w.explore(P0, far_city, HalfInt::from_raw(3));
        w.cheat_learn_spell(P0, f4x_def::rules::SpellId::new(132) /* plane_shift */);
        w.treasury_mut(P0).ok_or("a treasury")?.mana = HalfInt::from_int(5_000);
        w.wizard_mut(P0)
            .ok_or("a wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        let troops = spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* spearmen */
            12,
        );
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        run(&mut ai, &mut w, &mut events);
        assert_eq!(ai.campaign, Some(far_city), "{events:?}");
        let square = ai.crossing.ok_or("a square to cross by")?;
        assert!(
            PlayerView::new(&w, P0).city_at(square).is_none(),
            "no city: {square:?}"
        );
        assert_eq!(square.plane, Plane::Arcanus);
        // The army walks to the square and the spell is paid for over some turns.
        for _ in 0..30 {
            run(&mut ai, &mut w, &mut events);
            crate::wizardry::run(&mut ai, &mut w, &mut events);
            if events
                .iter()
                .any(|e| matches!(e, f4x_core::world::Event::PlaneShifted { .. }))
            {
                break;
            }
            w.play_round();
        }
        let shifted = troops
            .iter()
            .filter(|&&u| w.units().unit_position(u).map(|p| p.plane) == Some(Plane::Myrror))
            .count();
        assert!(shifted >= 6, "{shifted} crossed: {events:?}");
        Ok(())
    }

    /// A target far from the rally square: a march that has gone on past the usual time but not
    /// past the usual time plus the walk is not called back on the way (task 436).
    #[test]
    fn a_march_to_a_target_far_from_the_cities_is_not_called_off_on_the_way() -> TestResult {
        use crate::roles::Role;
        let mut w = rival_with_a_seat((4, 4))?;
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let tile = w.max_tile();
        spawn(&mut w, P0, capital, 113 /* high_men_paladins */, tile);
        w.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &mut w);
        run(&mut ai, &mut w, &mut Vec::new());
        assert!(ai.tasks(&w).roles.any(Role::March), "the march left");
        let target = ai.campaign.ok_or("a campaign")?;
        let trip = distance(&PlayerView::new(&w, P0), capital, target).ok_or("a way")?;
        assert!(trip > 3, "the target is far from the rally: {trip}");
        // Past the usual time, within the walk on top of it.
        ai.march_started = w.round();
        for _ in 0..=march_turns() {
            w.play_round();
        }
        let mut events = Vec::new();
        ai.march_started = w.round() - (march_turns() + 1);
        let before = events.len();
        run(&mut ai, &mut w, &mut events);
        assert!(
            !events[before..]
                .iter()
                .any(|e| matches!(e, Event::MarchCalledOff { .. })),
            "{events:?}"
        );
        Ok(())
    }

    #[test]
    fn a_march_to_a_far_target_gets_the_walk_on_top_of_the_usual_time() {
        // A near target: called off one turn past the usual time.
        assert!(!march_called_off(
            100 + march_turns(),
            100,
            0,
            march_turns()
        ));
        assert!(march_called_off(
            100 + march_turns() + 1,
            100,
            0,
            march_turns()
        ));
        // A target 30 squares away has 30 turns more.
        assert!(!march_called_off(
            100 + march_turns() + 30,
            100,
            30,
            march_turns()
        ));
        assert!(march_called_off(
            100 + march_turns() + 31,
            100,
            30,
            march_turns()
        ));
        // A clock that ran backwards (a loaded save) does not call it off.
        assert!(!march_called_off(5, 100, 0, march_turns()));
    }

    /// A rival's city across the sea: the army, strong enough, is tagged for a ship to the
    /// landing site and the war is declared (task 647). Nothing is loaded by hand any more.
    fn landing_world(n: usize) -> Result<(Game, Ai), Box<dyn std::error::Error>> {
        use crate::testworld::{build, ENEMY_CITY};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        build(
            &mut w,
            at(COAST_CITY.0, COAST_CITY.1),
            &[
                12, /* ship_wrights_guild */
                15, /* sawmill */
                13, /* ship_yard */
            ],
        );
        w.reveal_map(P0);
        w.make_contact(P0, P1);
        w.treasury_mut(P0).ok_or("a treasury")?.gold = HalfInt::from_int(1_000_000);
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            36, /* galley */
            1,
        );
        // One stays as the garrison; the others are the army.
        let keep =
            crate::view::garrison_wanted(&PlayerView::new(&w, P0), at(COAST_CITY.0, COAST_CITY.1));
        spawn(
            &mut w,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            113, /* high_men_paladins */
            n + keep,
        );
        w.balance_farmers(P0);
        let mut ai = Ai::new(P0);
        ai.unreachable.push(at(ENEMY_CITY.0, ENEMY_CITY.1));
        Ok((w, ai))
    }

    #[test]
    fn the_army_is_tagged_for_a_ship_to_the_landing_site_and_the_war_is_declared() -> TestResult {
        use crate::roles::Crossing;
        let (mut w, mut ai) = landing_world(6)?;
        assert_ne!(w.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
        for _ in 0..3 {
            crate::roles::register(&mut ai, &mut w);
            crate::naval::run(&mut ai, &mut w, &mut Vec::new());
            run(&mut ai, &mut w, &mut Vec::new());
        }
        assert_eq!(w.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
        let tagged = ai.tasks(&w).roles.needing(Crossing::Sea);
        assert!(tagged.len() >= min_march(), "{:?}", ai.tasks(&w).roles);
        // Their muster is the landing site, which they are bound for.
        let land = ai.naval.ok_or("a landing")?.land;
        assert!(tagged
            .iter()
            .all(|&(s, to)| to == land && ai.tasks(&w).roles.in_muster(s, MusterKind::Campaign)));
        Ok(())
    }

    /// A pact is not broken for a landing: the city is put aside.
    #[test]
    fn a_pact_stops_the_landing_before_the_army_is_tagged() -> TestResult {
        use crate::roles::Crossing;
        let (mut w, mut ai) = landing_world(6)?;
        w.start_treaty(P0, P1, f4x_core::diplomacy::Treaty::Pact);
        for _ in 0..3 {
            crate::roles::register(&mut ai, &mut w);
            crate::naval::run(&mut ai, &mut w, &mut Vec::new());
            run(&mut ai, &mut w, &mut Vec::new());
        }
        assert!(ai.tasks(&w).roles.needing(Crossing::Sea).is_empty());
        assert_eq!(w.treaty(P0, P1), f4x_core::diplomacy::Treaty::Pact);
        Ok(())
    }

    /// The whole landing: the army is tagged, a ship fetches it and puts it ashore beside the
    /// city, and from there it attacks.
    #[test]
    fn the_army_is_carried_across_the_sea_and_lands_beside_the_city() -> TestResult {
        use crate::testworld::ENEMY_CITY;
        let (mut w, mut ai) = landing_world(6)?;
        let enemy = at(ENEMY_CITY.0, ENEMY_CITY.1);
        let theirs = w
            .economy()
            .cities_of(P1)
            .first()
            .ok_or("the enemy has a city")?
            .0;
        let mut landed = false;
        for _ in 0..60 {
            let mut events = Vec::new();
            w.balance_farmers(P0);
            crate::roles::register(&mut ai, &mut w);
            crate::dispatch::revalidate(&mut ai, &mut w, &mut events);
            crate::naval::run(&mut ai, &mut w, &mut events);
            run(&mut ai, &mut w, &mut events);
            crate::pickup::run(&mut ai, &mut w, &mut events);
            w.play_round();
            let view = PlayerView::new(&w, P0);
            landed = view.my_units().into_iter().any(|u| {
                is_fighter(&view, u)
                    && view.unit_position(u).is_some_and(|p| {
                        p.x >= 26 && distance(&view, p, enemy).is_some_and(|d| d <= 3)
                    })
            });
            if landed {
                break;
            }
        }
        assert!(
            landed,
            "no soldier came ashore near the enemy city: {:?}",
            ai.tasks(&w).roles
        );
        for _ in 0..20 {
            if w.economy().city_owner(theirs) != Some(P1) {
                break;
            }
            let mut events = Vec::new();
            w.balance_farmers(P0);
            crate::dispatch::run(&mut ai, &mut w, &mut events);
            w.play_round();
        }
        assert_ne!(
            w.economy().city_owner(theirs),
            Some(P1),
            "the army ashore did not attack: {:?}",
            ai.tasks(&w).roles
        );
        Ok(())
    }

    /// A guarded tower across the sea, with no rival known: the army is tagged and carried like
    /// any landing (task 647), and comes ashore beside the tower.
    #[test]
    fn the_army_is_carried_across_the_sea_to_a_guarded_tower() -> TestResult {
        use crate::testworld::{build, spawn_tower};
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let port = at(COAST_CITY.0, COAST_CITY.1);
        build(&mut w, port, &[12, 15, 13]);
        let tower = at(33, 9);
        spawn_tower(&mut w, tower, true)?;
        w.explore(P0, tower, HalfInt::from_raw(5));
        w.treasury_mut(P0).ok_or("a treasury")?.gold = HalfInt::from_int(1_000_000);
        spawn(&mut w, P0, port, 36 /* galley */, 1);
        let keep = crate::view::garrison_wanted(&PlayerView::new(&w, P0), port);
        // A full battle side of paladins: enough for what is guessed of the guards.
        let side = w.max_battle_side();
        spawn(
            &mut w,
            P0,
            port,
            113, /* high_men_paladins */
            side + keep,
        );
        let mut ai = Ai::new(P0);
        let mut landed = false;
        for _ in 0..60 {
            let mut events = Vec::new();
            w.balance_farmers(P0);
            crate::roles::register(&mut ai, &mut w);
            crate::dispatch::revalidate(&mut ai, &mut w, &mut events);
            crate::naval::run(&mut ai, &mut w, &mut events);
            run(&mut ai, &mut w, &mut events);
            crate::pickup::run(&mut ai, &mut w, &mut events);
            w.play_round();
            let view = PlayerView::new(&w, P0);
            landed = view.my_units().into_iter().any(|u| {
                is_fighter(&view, u)
                    && view
                        .unit_position(u)
                        .is_some_and(|p| distance(&view, p, tower).is_some_and(|d| d <= 2))
            });
            if landed {
                break;
            }
        }
        assert!(
            landed,
            "no soldier came ashore at the tower: {:?}",
            ai.tasks(&w).roles
        );
        Ok(())
    }
}
