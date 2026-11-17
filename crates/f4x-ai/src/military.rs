//! The army: keep cities garrisoned, hit enemies that are weaker, gather a force and march
//! it on the nearest enemy city, and send spare units to explore.

use std::collections::HashMap;

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;
use f4x_core::unit::pathing::ReachSignature;

use crate::roles::MusterKind;
use crate::view::{distance, fighting_strength, is_fighter, party, stack_strength, unit_strength};
use crate::{race, Ai};

/// The AI attacks when its force is this much stronger than what it expects to meet.
pub(crate) fn attack_ratio() -> f32 {
    crate::knob("ATTACK", 1.4)
}
/// An attack out of a city's garrison needs a much bigger edge.
pub(crate) fn sortie_ratio() -> f32 {
    crate::knob("SORTIE", 2.5)
}
/// Strength assumed for a defence the AI cannot see.
const GUESS_DEFENDERS: f32 = 3.0;
/// How long a remembered garrison is trusted, in turns.
const MEMORY_TURNS: u32 = 80;
/// Squares of walking one average unit of garrison is worth when choosing a target.
const HELD_WEIGHT: f32 = 1.5;
/// A force that has marched and not fought for this many turns goes home to gather again.
const MARCH_TURNS: u32 = 15;
/// What a landing party wants over the expected defence before it marches on the city.
const COMMITTED_LANDING: f32 = 1.2;
/// Turns an enemy city that no march could reach is left alone.
const UNREACHABLE_TURNS: u32 = 40;
/// The most fighters the war plan asks the cities to build for one campaign.
const MAX_FORCE: usize = 20;
/// Once marched, a force attacks with this much of an edge.
pub(crate) fn committed_ratio() -> f32 {
    crate::knob("COMMITTED", 1.1)
}

/// How near an enemy stack must be to threaten a city.
pub(crate) const THREAT_RADIUS: i32 = 3;

/// The enemy has the better of the garrison when it is this share of its strength.
pub(crate) const THREAT_RATIO: f32 = 0.8;

const NEIGHBOURS: &[(i32, i32)] = &[
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

pub(crate) fn neighbours(view: &PlayerView, at: Position) -> Vec<Position> {
    NEIGHBOURS
        .iter()
        .filter_map(|&(dx, dy)| view.shift(at, dx, dy))
        .collect()
}

/// The units grouped by square, in a stable order.
pub(crate) fn stacks(
    view: &PlayerView,
    units: Vec<hecs::Entity>,
) -> Vec<(Position, Vec<hecs::Entity>)> {
    let mut by_pos: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
    for u in units {
        if let Some(p) = view.unit_position(u) {
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

/// Attack: a human defender fights the battle in person (kept for the client), any other is
/// resolved at once.
pub(crate) fn engage(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    let defended_in_person = defenders
        .first()
        .and_then(|&d| game.units().unit_owner(d))
        .is_some_and(|owner| ai.defended_by.contains(&owner));
    if defended_in_person {
        ai.attacks.push(crate::Attack {
            attackers,
            defenders,
        });
    } else {
        resolve(ai, game, attackers, defenders, events);
    }
}

/// Fight the battle at once, the mana the wizard may spend in it set by `budget.rs` (a hard
/// fight for a city gets all it may, a sure win in the open little).
pub(crate) fn resolve(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    let mana_budget =
        crate::budget::for_attack(&PlayerView::new(game, ai.player), &attackers, &defenders);
    ai.order(
        game,
        Command::AutoResolve {
            mana_budget,
            attackers,
            defenders,
        },
        events,
    );
}

pub fn army(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
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

    choose_target(ai, game, events, capital);
    let Some(target) = ai.campaign else {
        // Nothing to march on: the army raids the sites and towers, which also opens the way to
        // the other plane.
        // A march whose target has fallen is over: its units are available again where they
        // stand, for the dispatcher to garrison or send on.
        ai.roles.end_musters(crate::roles::MusterKind::Campaign);
        ai.roles.release_role(crate::roles::Role::March);
        ai.march_target = None;
        crate::site::deploy(ai, game, &free, events);
        return;
    };

    march_on(ai, game, events, &free, &own_cities, capital, target);
}

/// Choose (or keep) the city to attack, in `ai.campaign`: the war plan's, or a city across a
/// plane, or the landing in preparation. What is seen of enemy cities is remembered.
fn choose_target(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>, capital: Position) {
    let me = ai.player;
    let mut new_crossing: Option<Position> = None;
    {
        let view = PlayerView::new(game, me);
        for (_, owner, pos) in view.known_cities() {
            if owner != me && view.sees(pos) {
                let strength = fighting_strength(&view, &view.hostile_at(pos).unwrap_or_default());
                ai.city_memory.insert(pos, (strength, game.turn));
            }
        }
        if ai.campaign.is_some_and(|c| {
            view.city_at(c)
                .and_then(|e| view.city_owner(e))
                .is_none_or(|o| o == me)
        }) {
            ai.campaign = None;
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
                game.turn,
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
                    ai.unreachable_since = game.turn;
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
        // No tower to cross by: a wizard that knows Plane Shift carries the army over from the
        // capital, to a rival's city on the other plane.
        let gather = ai
            .campaign
            .is_none()
            .then(|| crate::wizardry::shift_gather(&view))
            .flatten();
        if let Some(gather) = gather {
            let other = match gather.plane {
                f4x_core::map::Plane::Arcanus => f4x_core::map::Plane::Myrror,
                f4x_core::map::Plane::Myrror => f4x_core::map::Plane::Arcanus,
            };
            let far = Position {
                plane: other,
                ..gather
            };
            if let Some(city) = best_enemy_city(
                &view,
                far,
                &ai.unreachable,
                &ai.city_memory,
                race::enemy(ai, &view),
            )
            .filter(|c| c.plane == other)
            {
                ai.campaign = Some(city);
                ai.crossing = Some(gather);
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
            ai.unreachable_since = game.turn;
        }
    }
}

/// Gather the soldiers at the rally city and march on `target` when they are enough.
fn march_on(
    ai: &mut Ai,
    game: &mut Game,
    events: &mut Vec<Event>,
    free: &[hecs::Entity],
    own_cities: &[Position],
    capital: Position,
    target: Position,
) {
    let me = ai.player;
    // Targets that could not be reached are tried again after a while.
    if game.turn.saturating_sub(ai.unreachable_since) > UNREACHABLE_TURNS {
        ai.unreachable.clear();
    }
    // Gather at the own city nearest the target; march when strong enough.
    let (rally, expected, at_rally, elsewhere, force) = {
        let view = PlayerView::new(game, me);
        let rally = own_cities
            .iter()
            .copied()
            .filter(|&c| distance(&view, c, target).is_some())
            .min_by_key(|&c| distance(&view, c, target))
            .unwrap_or(capital);
        let rally = ai
            .naval
            .filter(|p| p.target == target)
            .map_or(rally, |p| p.port);
        // Across the plane the army gathers at the far side of the tower.
        let rally = match ai.crossing {
            Some(tower) if target.plane != tower.plane => Position {
                plane: target.plane,
                ..tower
            },
            _ => rally,
        };
        // A march that has gone on too long, or whose target has changed, is called off. A far
        // target gets the walk to it on top of the usual time (the slowest unit covers a square
        // a turn), or the stack would turn back before it arrived.
        let trip = distance(&view, rally, target).unwrap_or(0);
        let too_long = march_called_off(game.turn, ai.march_started, trip);
        if too_long || ai.march_target != Some(target) {
            if ai.roles.any(crate::roles::Role::March) {
                events.push(Event::MarchCalledOff {
                    player: me,
                    target: ai.march_target.unwrap_or(target),
                    units: ai.roles.count(crate::roles::Role::March) as u32,
                    turns: game.turn.saturating_sub(ai.march_started),
                    target_changed: !too_long,
                });
            }
            ai.roles.release_role(crate::roles::Role::March);
            ai.roles.end_musters(crate::roles::MusterKind::Campaign);
            ai.march_target = Some(target);
        }
        let expected = expected_defence(&view, &ai.city_memory, game.turn, target);
        // The army attacks as one stack from one square: the units on the rally square, the
        // strongest of them up to what a square holds. The rest of them stay available.
        let (mut at_rally, elsewhere): (Vec<_>, Vec<_>) = free
            .iter()
            .copied()
            .filter(|&u| !ai.roles.is(view.serial_of(u), crate::roles::Role::March))
            .partition(|&u| view.unit_position(u) == Some(rally));
        at_rally.sort_by(|&a, &b| {
            unit_strength(&view, b)
                .total_cmp(&unit_strength(&view, a))
                .then(view.serial_of(a).cmp(&view.serial_of(b)))
        });
        at_rally.truncate(view.max_battle_side());
        let force = stack_strength(&view, &at_rally);
        (rally, expected, at_rally, elsewhere, force)
    };
    // The port of a landing keeps a slot free: a ship cannot enter a city holding a full tile.
    let naval_rally = ai.naval.is_some_and(|p| p.target == target);
    // No city is crowded beyond `garrison::city_cap`: there is no defence in more.
    let mut room = {
        let view = PlayerView::new(game, me);
        let cap = crate::garrison::city_cap(&view, rally);
        let limit = if naval_rally {
            cap.min(view.max_tile() - 1)
        } else {
            cap
        };
        limit.saturating_sub(view.garrison(rally).len())
    };
    let mut at_tower: Vec<hecs::Entity> = Vec::new();
    // Set when the tower's square refused a unit for want of room: no other unit is sent this
    // turn (a full square stays full, and each refusal was one more ignored order).
    let mut tower_full = false;
    // Units of a kind on a square get one path search between them (nothing moves meanwhile:
    // the orders given here only start a walk).
    let mut reached = crate::view::ReachCache::default();
    // The units sent to the rally this turn (they hold `Muster` from now on).
    let mut sent_to_rally: Vec<hecs::Entity> = Vec::new();
    // The force wanted is gathered by need: the nearest units are sent while the strength at the
    // rally and on its way is below what the march needs (and fewer than three). The others stay
    // where they are, available to the dispatcher's other jobs, as do the units of a town just
    // taken: nothing sends them anywhere.
    let needed = expected * attack_ratio() + 1.0;
    let mut elsewhere = elsewhere;
    let (mut have, mut have_units) = {
        let view = PlayerView::new(game, me);
        elsewhere.sort_by_key(|&u| {
            (
                view.unit_position(u)
                    .and_then(|p| distance(&view, p, rally))
                    .unwrap_or(u32::MAX),
                view.serial_of(u),
            )
        });
        let on_its_way = elsewhere.iter().copied().filter(|&u| {
            view.unit_has_order(u) && ai.roles.in_muster(view.serial_of(u), MusterKind::Campaign)
        });
        let (strength, units) = on_its_way.fold((force, at_rally.len()), |(s, n), u| {
            (s + unit_strength(&view, u), n + 1)
        });
        (strength, units)
    };
    let available = at_rally.len() + elsewhere.len();
    let mut counted = 0;
    for u in &elsewhere {
        for &sent in &sent_to_rally[counted..] {
            have += unit_strength(&PlayerView::new(game, me), sent);
            have_units += 1;
        }
        counted = sent_to_rally.len();
        if let Some(tower) = ai.crossing.filter(|t| t.plane != rally.plane) {
            let (here, ordered, reach) = {
                let view = PlayerView::new(game, me);
                let here = view.unit_position(*u);
                let ordered = view.unit_has_order(*u);
                // Only a unit that is not at the tower, has no order and is on the tower's plane
                // needs to know whether it can walk there.
                let asks =
                    here != Some(tower) && !ordered && here.is_some_and(|p| p.plane == tower.plane);
                (
                    here,
                    ordered,
                    asks && reached.can_reach(&view, &[*u], tower),
                )
            };
            if here == Some(tower) {
                at_tower.push(*u);
            } else if !ordered
                && reach
                && !tower_full
                && Ai::room_at(game, tower, 1)
                && at_tower.len() < PlayerView::new(game, me).max_battle_side()
                && here.is_some_and(|p| p.plane == tower.plane)
            {
                let sent = ai.send(
                    game,
                    Command::GoTo {
                        units: vec![*u],
                        to: tower,
                    },
                    events,
                );
                if sent.is_ok() {
                    sent_to_rally.push(*u);
                }
                tower_full = sent.is_err_and(|why| why.is_no_room());
            }
            continue;
        }
        // Nothing is sent while too few units are free to make a march at all.
        // The party is full at the most a battle holds (more only crowd the squares), or strong
        // enough, or there are too few free units to make a march at all.
        let party_size = PlayerView::new(game, me).max_battle_side();
        if (have >= needed && have_units >= MIN_MARCH)
            || have_units >= party_size
            || available < MIN_MARCH
        {
            continue;
        }
        // A gate is quicker than the road: an Earth Gate city sends its troops to the rally
        // city when that has one too, and an Astral Gate city sends those on the wrong plane
        // across (the original's AI does the second, `AIMOVE.c`).
        let ordered = PlayerView::new(game, me).unit_has_order(*u);
        let here = game.units().unit_position(*u);
        if !ordered {
            if game.can_use_earth_gate(&[*u], rally).is_ok() {
                ai.order(
                    game,
                    Command::EarthGate {
                        units: vec![*u],
                        to: rally,
                    },
                    events,
                );
                sent_to_rally.push(*u);
                continue;
            }
            if here.is_some_and(|p| {
                p.plane != rally.plane
                    && game
                        .economy()
                        .has_gate(p, me, f4x_core::city::gate::ASTRAL_GATE)
            }) {
                ai.order(game, Command::ShiftPlane { units: vec![*u] }, events);
                sent_to_rally.push(*u);
                continue;
            }
        }
        let comes_home = {
            let view = PlayerView::new(game, me);
            !view.unit_has_order(*u)
                && view
                    .unit_position(*u)
                    .is_some_and(|p| p.plane == rally.plane)
                && reached.can_reach(&view, &[*u], rally)
        };
        if comes_home && room > 0 {
            // The room in the rally square is taken only by a unit that goes.
            let sent = ai.send(
                game,
                Command::GoTo {
                    units: vec![*u],
                    to: rally,
                },
                events,
            );
            if sent.is_ok() {
                room = room.saturating_sub(1);
                sent_to_rally.push(*u);
            }
        }
    }
    // The gathering is a muster of the registry: the units at the rally and on their way to it.
    // (A landing gathers at the port for the ship, `naval.rs`: not labelled.)
    if !naval_rally {
        let members: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            at_rally
                .iter()
                .chain(&sent_to_rally)
                .chain(
                    elsewhere
                        .iter()
                        .filter(|&&u| ai.roles.in_muster(view.serial_of(u), MusterKind::Campaign)),
                )
                .map(|&u| view.serial_of(u))
                .collect()
        };
        let stale = ai
            .roles
            .musters()
            .into_iter()
            .find(|(_, m, _)| m.kind == MusterKind::Campaign)
            .filter(|(_, m, _)| m.rally != rally || m.objective != Some(target))
            .map(|(id, ..)| id);
        if let Some(id) = stale {
            ai.roles.end_muster(id);
        }
        match ai
            .roles
            .musters()
            .into_iter()
            .find(|(_, m, _)| m.kind == MusterKind::Campaign)
        {
            Some((id, ..)) => {
                ai.roles.join_muster(id, &members, game.turn);
            }
            None => {
                ai.roles.begin_muster(
                    MusterKind::Campaign,
                    rally,
                    Some(target),
                    &members,
                    game.turn,
                );
            }
        }
    }
    // The troops at the tower cross in stacks of what a square holds.
    let stack = game.max_battle_side();
    for chunk in at_tower.chunks(stack) {
        if chunk.len() < MIN_CROSSING.min(elsewhere.len()) {
            continue;
        }
        if !PlayerView::new(game, me).can_shift_plane(chunk) {
            continue;
        }
        {
            ai.order(
                game,
                Command::ShiftPlane {
                    units: chunk.to_vec(),
                },
                events,
            );
        }
    }
    // Troops for a landing wait at the port; the ship takes them. Those already ashore on the
    // target's land mass march on it when they are strong enough.
    if ai.naval.is_some_and(|p| p.target == target) {
        march_landed(ai, game, free, target, events);
        return;
    }
    if force >= expected * attack_ratio() + 1.0 && at_rally.len() >= MIN_MARCH {
        let mut launched: Vec<Serial> = Vec::new();
        let mut refused = false;
        // Units march as stacks: those on one square, as many as a square holds at most.
        let mut by_square: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
        {
            let view = PlayerView::new(game, me);
            for &u in &at_rally {
                let Some(p) = view.unit_position(u) else {
                    continue;
                };
                match by_square.iter_mut().find(|(q, _)| *q == p) {
                    Some((_, list)) => list.push(u),
                    None => by_square.push((p, vec![u])),
                }
            }
        }
        for (_, units) in &by_square {
            for chunk in units.chunks(game.max_battle_side()) {
                let serials: Vec<Serial> = {
                    let view = PlayerView::new(game, me);
                    chunk.iter().map(|&u| view.serial_of(u)).collect()
                };
                // A stack already next to the target has arrived: it attacks from where it is.
                let beside = {
                    let view = PlayerView::new(game, me);
                    chunk.iter().all(|&u| {
                        view.unit_position(u)
                            .and_then(|p| distance(&view, p, target))
                            .is_some_and(|d| d <= 1)
                    })
                };
                if beside {
                    launched.extend(serials);
                    continue;
                }
                match crate::assault::march(ai, game, chunk, target, events) {
                    Ok(()) => launched.extend(serials),
                    Err(crate::assault::MarchError::Refused(_)) => refused = true,
                    Err(_) => {}
                }
            }
        }
        if launched.is_empty() && !refused {
            // No square next to the target can be reached: give it up for a while and go for
            // the next nearest city (ships or a gate may open the way later).
            ai.unreachable.push(target);
            ai.unreachable_since = game.turn;
            ai.campaign = None;
        } else if !launched.is_empty() {
            events.push(Event::MarchLaunched {
                player: me,
                target,
                units: launched.len() as u32,
            });
            ai.roles.end_musters(MusterKind::Campaign);
            ai.roles.assign(
                crate::roles::Role::March,
                launched.iter().copied(),
                game.turn,
            );
            for &s in &launched {
                ai.roles
                    .set_target(s, crate::roles::Role::March, Some(target));
            }
            ai.march_started = game.turn;
        }
    }
}

/// The own cities an enemy stack within [`THREAT_RADIUS`] could take: the strength near is more
/// than [`THREAT_RATIO`] of the strength at home. With the strength still wanted.
pub(crate) fn threats(view: &PlayerView) -> Vec<(Position, f32)> {
    let mut found = Vec::new();
    for (_, pos) in view.my_cities() {
        let near: f32 = view
            .enemies_near(pos, THREAT_RADIUS)
            .iter()
            .map(|(_, units)| fighting_strength(view, units))
            .sum();
        if near <= 0.0 {
            continue;
        }
        let home: Vec<_> = view
            .garrison(pos)
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        let held = fighting_strength(view, &home);
        if near * THREAT_RATIO > held {
            found.push((pos, near / THREAT_RATIO - held));
        }
    }
    found
}

/// Has a march gone on too long? It has [`MARCH_TURNS`] and the walk to a far target on top (the
/// slowest unit covers a square a turn), so a stack bound for a distant city is not called back
/// on the way.
fn march_called_off(now: u32, started: u32, trip: u32) -> bool {
    now.saturating_sub(started) > MARCH_TURNS + trip
}

/// The mean strength of the AI's fighters (20 with none). Worked out once for a view: it walks
/// every own unit, and the plans ask for it once per target.
pub(crate) fn average_strength(view: &PlayerView) -> f32 {
    view.memo(|| {
        let units: Vec<_> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        if units.is_empty() {
            return 20.0;
        }
        units.iter().map(|&u| unit_strength(view, u)).sum::<f32>() / units.len() as f32
    })
}

/// What the AI expects to meet in an enemy city: the strength it last saw there (with the city's
/// own walls counted in), or a guess of a few average units if it never saw it or saw it long
/// ago.
pub(crate) fn expected_defence(
    view: &PlayerView,
    memory: &HashMap<Position, (f32, u32)>,
    turn: u32,
    at: Position,
) -> f32 {
    // What the enemy city screen shows a human of a city it has explored: the units in it.
    if let Some(report) = view.city_at(at).and_then(|c| view.city_report(c)) {
        return fighting_strength(view, &report.garrison) * 1.3;
    }
    if view.sees(at) {
        return fighting_strength(view, &view.hostile_at(at).unwrap_or_default()) * 1.3;
    }
    match memory.get(&at) {
        Some(&(s, seen)) if turn.saturating_sub(seen) <= MEMORY_TURNS => s * 1.3,
        // A guess of a few average units: only now is the average worked out.
        _ => crate::knob("GUESS", GUESS_DEFENDERS) * average_strength(view),
    }
}

/// How many average fighters the war plan needs to march on `target`: enough to beat what is
/// expected there with the margin the march demands, and at least the three a march needs.
pub(crate) fn force_wanted(
    view: &PlayerView,
    memory: &HashMap<Position, (f32, u32)>,
    turn: u32,
    target: Position,
) -> usize {
    let needed = expected_defence(view, memory, turn, target) * attack_ratio() + 1.0;
    ((needed / average_strength(view).max(1.0)).ceil() as usize)
        .clamp(3, MAX_FORCE.min(view.max_battle_side()))
}

/// The fewest units a march starts with.
const MIN_MARCH: usize = 3;

/// The fewest troops that cross a plane together.
const MIN_CROSSING: usize = 3;

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

/// Squares of walking a rival's fortress city is worth: taking the seat ends its wizard.
const FORTRESS_BONUS: f32 = 12.0;
/// Squares of walking a city on the way to the rival's fortress is worth: taking it opens the road
/// and cuts the fortress off.
const ROUTE_BONUS: f32 = 6.0;
/// A city is "on the way" when going through it makes the walk to the fortress this much longer.
const ROUTE_SLACK: u32 = 2;

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
        return FORTRESS_BONUS;
    }
    let (Some(direct), Some(to_city), Some(on)) = (
        distance(view, from, fort),
        distance(view, from, city),
        distance(view, city, fort),
    ) else {
        return 0.0;
    };
    if to_city + on <= direct + ROUTE_SLACK && to_city < direct {
        ROUTE_BONUS
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
        // A wizard it has a pact or an alliance with is not attacked: the attack would break the
        // treaty.
        .filter(|&(_, owner, _)| {
            !view.is_rival_wizard(owner)
                || !matches!(
                    view.treaty_with(owner),
                    f4x_core::diplomacy::Treaty::Pact | f4x_core::diplomacy::Treaty::Alliance
                )
        })
        .filter_map(|(_, owner, p)| {
            let d = distance(view, from, p)? as f32;
            let held = memory.get(&p).map_or(GUESS_DEFENDERS, |m| m.0 / average);
            Some((
                d + HELD_WEIGHT * held - strategic_bonus(view, from, owner, p),
                p,
            ))
        })
        .min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1.y, a.1.x).cmp(&(b.1.y, b.1.x)))
        })
        .map(|(_, p)| p)
}

/// Troops that came ashore (they can walk to the target, which the rest cannot) march on the city
/// when together they are strong enough for what is expected there.
/// Set `F4X_CHECK_REACH` to make the search for every unit anyway and compare it with the
/// answer from the region of an earlier search that found nothing (slow).
fn check_regions() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("F4X_CHECK_REACH").is_some())
}

fn march_landed(
    ai: &mut Ai,
    game: &mut Game,
    free: &[hecs::Entity],
    target: Position,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let (landed, expected) = {
        let view = PlayerView::new(game, me);
        let open: Vec<Position> = neighbours(&view, target)
            .into_iter()
            .filter(|&s| view.hostile_at(s).is_none())
            .collect();
        // Units of one kind on one square walk to the same places: one search (that stops at the
        // first square next to the target it finds) serves them all. A search that finds none
        // has run over the whole region the unit can walk to, and a unit of the same kind
        // standing anywhere in that region cannot reach one either (the squares it can walk to
        // are the same, as every step costs the same both ways): the second search is not made.
        let mut known: std::collections::HashMap<_, bool> = std::collections::HashMap::new();
        let mut regions: Vec<(ReachSignature, f4x_core::fast::FastSet<Position>)> = Vec::new();
        let check = check_regions();
        let landed: Vec<hecs::Entity> = free
            .iter()
            .copied()
            .filter(|&u| !ai.roles.is(view.serial_of(u), crate::roles::Role::March))
            .filter(|&u| {
                if open.is_empty() {
                    return false;
                }
                let Some(at) = view.unit_position(u) else {
                    return false;
                };
                let squares: Vec<Position> = open.iter().copied().filter(|&s| s != at).collect();
                if squares.is_empty() {
                    return false;
                }
                let Some(key) = view.reach_signature(&[u]) else {
                    return view.reaches_any(&[u], &squares);
                };
                let in_region = regions
                    .iter()
                    .any(|(sig, region)| sig.moves_like(&key) && region.contains(&key.at()));
                if in_region && !check {
                    return false;
                }
                let known_key = key.clone();
                let reached = *known.entry(key.clone()).or_insert_with(|| {
                    match view.unreached_region(&[u], &squares) {
                        Some(region) => {
                            // A unit that stands on a square next to the target left it out of
                            // its targets, and the region then holds a target for the others.
                            if !open.contains(&at) {
                                regions.push((key.clone(), region));
                            }
                            false
                        }
                        None => true,
                    }
                });
                if in_region && reached {
                    let from = regions
                        .iter()
                        .find(|(sig, region)| sig.moves_like(&known_key) && region.contains(&known_key.at()))
                        .map(|(sig, r)| (sig.at(), r.len()));
                    panic!(
                        "a unit in the region of a failed search reached the target: unit {:?} target {target:?} region from {from:?} open {open:?}",
                        known_key.at()
                    );
                }
                reached
            })
            .collect();
        let expected = expected_defence(&view, &ai.city_memory, game.turn, target);
        (landed, expected)
    };
    // One battle is one square's party: each square is judged on its own best nine, and the
    // soldiers beyond them stay where they are.
    let mut by_square: Vec<(Position, Vec<hecs::Entity>)> = Vec::new();
    {
        let view = PlayerView::new(game, me);
        for &u in &landed {
            let Some(p) = view.unit_position(u) else {
                continue;
            };
            match by_square.iter_mut().find(|(q, _)| *q == p) {
                Some((_, list)) => list.push(u),
                None => by_square.push((p, vec![u])),
            }
        }
    }
    let mut launched: Vec<Serial> = Vec::new();
    for (_, units) in &by_square {
        let band = {
            let view = PlayerView::new(game, me);
            party(&view, units)
        };
        let strong_enough = {
            let view = PlayerView::new(game, me);
            band.len() >= 2 && stack_strength(&view, &band) >= expected * COMMITTED_LANDING
        };
        if !strong_enough {
            continue;
        }
        let serials: Vec<Serial> = {
            let view = PlayerView::new(game, me);
            band.iter().map(|&u| view.serial_of(u)).collect()
        };
        if crate::assault::march(ai, game, &band, target, events).is_ok() {
            launched.extend(serials);
        }
    }
    if !launched.is_empty() {
        events.push(Event::MarchLaunched {
            player: me,
            target,
            units: launched.len() as u32,
        });
        ai.roles.assign(
            crate::roles::Role::March,
            launched.iter().copied(),
            game.turn,
        );
        for &s in &launched {
            ai.roles
                .set_target(s, crate::roles::Role::March, Some(target));
        }
        ai.march_started = game.turn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_def::half_int::HalfInt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// P1 has its capital in the west of the east island and a second city, its seat, at `seat`;
    /// P0 sees both and is at war with it.
    fn rival_with_a_seat(seat: (u32, u32)) -> Result<Game, Box<dyn std::error::Error>> {
        use f4x_core::wizard::fortress::Fortress;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let capital = at(
            crate::testworld::ENEMY_CITY.0,
            crate::testworld::ENEMY_CITY.1,
        );
        let events = g.apply(Command::FoundCity {
            owner: P1,
            pos: at(seat.0, seat.1),
            name: "Seat".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        g.wizard_mut(P1)
            .ok_or("player 1 is a wizard")?
            .put(Some(Fortress(at(seat.0, seat.1))));
        for p in [capital, at(seat.0, seat.1)] {
            g.explore(P0, p, HalfInt::from_raw(3));
        }
        g.declare_war(P0, P1);
        Ok(g)
    }

    #[test]
    fn the_rivals_fortress_is_taken_before_a_nearer_city() -> TestResult {
        let g = rival_with_a_seat((30, 10))?;
        let here = at(COAST_CITY.0, COAST_CITY.1);
        let pick = best_enemy_city(&PlayerView::new(&g, P0), here, &[], &HashMap::new(), None);
        // The capital is nearer (18 squares, the seat 20) but is not the seat.
        assert_eq!(pick, Some(at(30, 10)));
        Ok(())
    }

    #[test]
    fn a_wizard_with_a_pact_is_not_marched_on_but_one_at_war_is() {
        use f4x_core::diplomacy::Treaty;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let east = at(
            crate::testworld::ENEMY_CITY.0,
            crate::testworld::ENEMY_CITY.1,
        );
        g.explore(P0, east, HalfInt::from_raw(3));
        let here = at(COAST_CITY.0, COAST_CITY.1);
        let pick =
            |g: &Game| best_enemy_city(&PlayerView::new(g, P0), here, &[], &HashMap::new(), None);
        assert_eq!(pick(&g), Some(east), "at peace the city is a target");
        g.start_treaty(P0, P1, Treaty::Pact);
        assert_eq!(pick(&g), None, "a pact partner is left alone");
        g.start_treaty(P0, P1, Treaty::None);
        g.declare_war(P0, P1);
        assert_eq!(pick(&g), Some(east));
    }

    #[test]
    fn soldiers_gathering_for_the_campaign_hold_a_muster_until_the_march_leaves() -> TestResult {
        use crate::roles::Role;
        // A rival city on the west island, and soldiers away from the own city.
        let mut g = rival_with_a_seat((4, 4))?;
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let troops = spawn(&mut g, P0, at(5, 9), 113 /* high_men_paladins */, 9);
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.campaign, Some(at(4, 4)));
        let musters = ai.roles.musters();
        assert_eq!(musters.len(), 1, "one muster");
        let (_, muster, members) = &musters[0];
        assert_eq!(muster.kind, MusterKind::Campaign);
        assert_eq!(muster.objective, Some(at(4, 4)));
        assert_eq!(muster.rally, capital);
        // Only the units the march needs are sent; the others stay where they are, available.
        assert_eq!(members.len(), MIN_MARCH, "{members:?}");
        let view = PlayerView::new(&g, P0);
        let sent: Vec<_> = troops
            .iter()
            .copied()
            .filter(|&u| ai.roles.is(view.serial_of(u), Role::Muster))
            .collect();
        assert_eq!(sent.len(), MIN_MARCH);
        for &u in troops.iter().filter(|u| !sent.contains(u)) {
            assert!(!view.unit_has_order(u), "left alone");
            assert_eq!(view.unit_position(u), Some(at(5, 9)));
        }
        // They arrive and are strong enough: the march leaves and the muster is over.
        for &u in &sent {
            g.cheat_move_unit(u, capital);
        }
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        assert!(
            ai.roles.musters().is_empty(),
            "the muster ended: {:?} {:?}",
            ai.roles.musters(),
            ai.campaign
        );
        assert!(ai.roles.any(Role::March), "the march left");
        Ok(())
    }

    /// Units that need rest are not mustered: they keep no role and no order while the healthy
    /// ones of the same square are sent.
    #[test]
    fn hurt_soldiers_are_kept_out_of_the_muster() -> TestResult {
        use crate::roles::Role;
        let mut g = rival_with_a_seat((4, 4))?;
        let troops = spawn(&mut g, P0, at(5, 9), 113 /* high_men_paladins */, 6);
        let (hurt, healthy) = troops.split_at(3);
        for &u in hurt {
            let (_, max) = PlayerView::new(&g, P0).unit_hits(u);
            g.scenario_set_hurt(u, max * 4 / 5); // damage taken
        }
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        let view = PlayerView::new(&g, P0);
        for &u in hurt {
            assert!(crate::recover::needs_rest(&view, u));
            assert!(
                !ai.roles.is(view.serial_of(u), Role::Muster),
                "hurt unit mustered"
            );
            assert!(
                !ai.roles.is(view.serial_of(u), Role::March),
                "hurt unit marched"
            );
        }
        let sent = healthy
            .iter()
            .filter(|&&u| ai.roles.is(view.serial_of(u), Role::Muster))
            .count();
        assert_eq!(sent, MIN_MARCH, "the healthy ones are sent");
        Ok(())
    }

    /// When the march's target falls, its units stay where they stand with no role and no order:
    /// the dispatcher decides what they do next (garrison, a new target).
    #[test]
    fn after_a_capture_the_marching_units_stay_put_and_are_available() -> TestResult {
        use crate::roles::Role;
        // P1's second city is the only one P0 knows, and it is not a fortress: taking it does
        // not end the game.
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.make_contact(P0, P1);
        let town = at(4, 4);
        let events = g.apply(Command::FoundCity {
            owner: P1,
            pos: town,
            name: "Town".into(),
            race: g.content().rules.races.id_numbered(8),
            population: 8,
        });
        assert!(
            !events.iter().any(|e| matches!(e, Event::Rejected(_))),
            "{events:?}"
        );
        g.explore(P0, town, HalfInt::from_raw(3));
        g.declare_war(P0, P1);
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let tile = g.max_tile();
        let troops = spawn(&mut g, P0, capital, 113 /* high_men_paladins */, tile);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        assert_eq!(ai.campaign, Some(town));
        assert!(ai.roles.any(Role::March), "the march left");
        let city = g
            .economy()
            .cities_of(P1)
            .into_iter()
            .find(|&(_, p)| p == town)
            .ok_or("the town is a city of P1")?
            .0;
        g.conquer_city(city, P0, 0);
        g.explore(P0, town, HalfInt::from_raw(3));
        let before: Vec<_> = {
            let view = PlayerView::new(&g, P0);
            troops.iter().map(|&u| view.unit_position(u)).collect()
        };
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        assert_ne!(ai.campaign, Some(town), "the taken town is no target");
        assert!(!ai.roles.any(Role::March), "the march is over");
        assert!(ai.roles.musters().is_empty());
        // Nothing sends the soldiers anywhere: they stay where they are with no task, for the
        // dispatcher to keep as a garrison or give a new target.
        let view = PlayerView::new(&g, P0);
        for (&u, was) in troops.iter().zip(before) {
            assert_eq!(view.unit_position(u), was, "stays where it stands");
            assert!(
                !ai.roles.has_role(view.serial_of(u)),
                "no task: {:?}",
                ai.roles.role_of(view.serial_of(u))
            );
        }
        Ok(())
    }

    /// West's city holds one spearman; four of East's swordsmen stand two squares away and three
    /// of West's own wait four squares from the city.
    fn besieged(war: bool) -> (Game, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        spawn(&mut g, P0, at(10, 6), 104 /* high_men_spearmen */, 1);
        spawn(&mut g, P1, at(8, 6), 105 /* high_men_swordsmen */, 4);
        let spare = spawn(&mut g, P0, at(6, 3), 105 /* high_men_swordsmen */, 3);
        g.make_contact(P0, P1);
        if war {
            g.declare_war(P0, P1);
        }
        (g, spare)
    }

    #[test]
    fn a_city_with_a_stronger_enemy_stack_near_calls_in_the_troops_and_asks_for_more() {
        let (mut g, spare) = besieged(true);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        // The dispatcher's job 0 calls the troops in; the army follows.
        crate::dispatch::run(&mut ai, &mut g, &mut events);
        assert_eq!(ai.threatened, vec![at(10, 6)], "{events:?}");
        let view = PlayerView::new(&g, P0);
        assert!(
            spare.iter().all(|&u| view.unit_has_order(u)),
            "the spare troops are on their way: {events:?}"
        );
    }

    #[test]
    fn at_peace_the_same_stack_is_no_threat() {
        let (mut g, spare) = besieged(false);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        assert!(ai.threatened.is_empty());
        // No relief is called in: the spare troops are no part of a march or a raid. (Idle
        // soldiers outside a city go home, which is no relief either.)
        let _ = spare;
        assert!(
            !ai.roles.any(crate::roles::Role::March) && !ai.roles.any(crate::roles::Role::Raid)
        );
    }

    #[test]
    fn the_force_wanted_grows_with_the_garrison_the_ai_remembers_in_the_target() {
        let (g, _) = besieged(true);
        let view = PlayerView::new(&g, P0);
        let target = at(2, 12);
        let none = HashMap::new();
        let weak = HashMap::from([(target, (1.0, 1))]);
        let strong = HashMap::from([(target, (average_strength(&view) * 9.0, 1))]);
        let wanted = |m: &HashMap<_, _>| force_wanted(&view, m, 2, target);
        assert_eq!(wanted(&weak), 3, "a march needs three at least");
        assert!(wanted(&none) >= 3);
        assert!(
            wanted(&strong) > wanted(&none),
            "{} {}",
            wanted(&strong),
            wanted(&none)
        );
        assert!(wanted(&strong) <= MAX_FORCE);
        // The army attacks from one square: no more is wanted than a square holds.
        assert!(wanted(&strong) <= view.max_battle_side());
    }

    /// The march is one stack on one square: the strongest units of the rally square up to what a
    /// square holds; a unit standing beside the rally is not counted as gathered, and what is
    /// left over keeps no order.
    #[test]
    fn the_march_is_one_stack_from_the_rally_square_and_the_rest_stay_available() -> TestResult {
        use crate::roles::Role;
        let mut g = rival_with_a_seat((4, 4))?;
        let capital = at(COAST_CITY.0, COAST_CITY.1);
        let (max, tile) = (g.max_battle_side(), g.max_tile());
        let stack = spawn(&mut g, P0, capital, 113 /* high_men_paladins */, tile);
        let beside = spawn(&mut g, P0, at(COAST_CITY.0 - 1, COAST_CITY.1), 113, 2);
        g.reveal_map(P0);
        let mut ai = Ai::new(P0);
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut Vec::new());
        crate::roles::register(&mut ai, &g);
        let view = PlayerView::new(&g, P0);
        let marching: Vec<_> = stack
            .iter()
            .chain(&beside)
            .copied()
            .filter(|&u| ai.roles.is(view.serial_of(u), Role::March))
            .collect();
        assert!(!marching.is_empty(), "the march left");
        assert!(marching.len() <= max);
        // The registry knows where the march goes, so a lost walk can be given again.
        let ahead = ai.campaign;
        assert!(ahead.is_some());
        assert!(marching
            .iter()
            .all(|&u| ai.roles.target_of(view.serial_of(u), Role::March) == ahead));
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut g, tower, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        g.apply(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [tower, on_myrror(tower), far_city, at(28, 6)] {
            g.explore(P0, p, HalfInt::from_raw(3));
        }
        let troops = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* spearmen */
            6,
        );
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        assert_eq!(ai.crossing, Some(tower), "{events:?}");
        assert_eq!(ai.campaign, Some(far_city));
        assert!(
            troops
                .iter()
                .any(|&u| PlayerView::new(&g, P0).unit_has_order(u)),
            "the troops walk to the tower"
        );
        // Standing on the tower they cross.
        let party = spawn(&mut g, P0, tower, 104 /* spearmen */, 4);
        army(&mut ai, &mut g, &mut events);
        assert!(
            party
                .iter()
                .any(|&u| g.units().unit_position(u).map(|p| p.plane) == Some(Plane::Myrror)),
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let tower = at(8, 6);
        spawn_tower(&mut g, tower, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        g.apply(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [tower, on_myrror(tower), far_city, at(28, 6)] {
            g.explore(P0, p, HalfInt::from_raw(3));
        }
        spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 6);
        g.make_contact(P0, P1);
        assert_ne!(g.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        assert_eq!(ai.crossing, Some(tower), "{events:?}");
        assert_eq!(g.treaty(P0, P1), f4x_core::diplomacy::Treaty::War);
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
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let near = at(8, 6);
        spawn_tower(&mut g, near, false)?;
        let on_myrror = |p: Position| Position {
            plane: Plane::Myrror,
            ..p
        };
        let far_city = on_myrror(at(28, 7));
        g.apply(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        for p in [near, on_myrror(near), far_city, at(28, 6)] {
            g.explore(P0, p, HalfInt::from_raw(3));
        }
        // The rival's unit stands in the tower, on the far plane's side of it.
        let holder = spawn(&mut g, P1, on_myrror(near), 104 /* spearmen */, 1);
        let troops = spawn(&mut g, P0, at(COAST_CITY.0, COAST_CITY.1), 104, 6);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        crate::roles::register(&mut ai, &g);
        army(&mut ai, &mut g, &mut events);
        assert_eq!(
            ai.crossing,
            Some(near),
            "the tower is not set aside: {events:?}"
        );
        // The way onto the tower from our side ends in the holder.
        let mover = spawn(&mut g, P0, at(7, 6), 104 /* spearmen */, 1)[0];
        assert!(!troops.is_empty());
        match g.economy().plan_step(&[mover], near) {
            Ok((_, f4x_core::unit::movement::Entry::Hostile(found))) => assert_eq!(found, holder),
            other => panic!("the holder is not met: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn a_wizard_with_plane_shift_carries_the_army_to_a_city_on_the_other_plane(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use f4x_core::map::Plane;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let far_city = Position {
            plane: Plane::Myrror,
            ..at(28, 7)
        };
        g.apply(Command::FoundCity {
            owner: P1,
            pos: far_city,
            name: "Farport".into(),
            race: None,
            population: 3,
        });
        g.explore(P0, far_city, HalfInt::from_raw(3));
        g.cheat_learn_spell(P0, f4x_def::rules::SpellId::new(132) /* plane_shift */);
        g.treasury_mut(P0).ok_or("a treasury")?.mana = HalfInt::from_int(5_000);
        g.wizard_mut(P0)
            .ok_or("a wizard")?
            .get_mut::<f4x_core::wizard::sheet::Skill>()
            .left = 500;
        let troops = spawn(
            &mut g,
            P0,
            at(COAST_CITY.0, COAST_CITY.1),
            104, /* spearmen */
            12,
        );
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        assert_eq!(ai.campaign, Some(far_city), "{events:?}");
        assert_eq!(ai.crossing, Some(at(COAST_CITY.0, COAST_CITY.1)));
        // The spell is paid for over some turns.
        for _ in 0..30 {
            crate::wizardry::run(&mut ai, &mut g, &mut events);
            if events
                .iter()
                .any(|e| matches!(e, f4x_core::game::Event::PlaneShifted { .. }))
            {
                break;
            }
            g.apply(Command::EndTurn);
        }
        let shifted = troops
            .iter()
            .filter(|&&u| g.units().unit_position(u).map(|p| p.plane) == Some(Plane::Myrror))
            .count();
        assert!(shifted >= 6, "{shifted} crossed: {events:?}");
        Ok(())
    }

    #[test]
    fn a_march_to_a_far_target_gets_the_walk_on_top_of_the_usual_time() {
        // A near target: called off one turn past the usual time.
        assert!(!march_called_off(100 + MARCH_TURNS, 100, 0));
        assert!(march_called_off(100 + MARCH_TURNS + 1, 100, 0));
        // A target 30 squares away has 30 turns more.
        assert!(!march_called_off(100 + MARCH_TURNS + 30, 100, 30));
        assert!(march_called_off(100 + MARCH_TURNS + 31, 100, 30));
        // A clock that ran backwards (a loaded save) does not call it off.
        assert!(!march_called_off(5, 100, 0));
    }
}
