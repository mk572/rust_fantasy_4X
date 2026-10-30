//! The army: keep cities garrisoned, hit enemies that are weaker, gather a force and march
//! it on the nearest enemy city, and send spare units to explore.

use std::collections::HashMap;

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

use crate::view::{distance, garrison_wanted, is_fighter, stack_strength, unit_strength};
use crate::{race, Ai};

/// The AI attacks when its force is this much stronger than what it expects to meet.
fn attack_ratio() -> f32 {
    crate::knob("ATTACK", 1.4)
}
/// An attack out of a city's garrison needs a much bigger edge.
fn sortie_ratio() -> f32 {
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
/// Units this close to the rally city count as gathered (a full city cannot hold them all).
const GATHER_RADIUS: u32 = 2;
/// What a landing party wants over the expected defence before it marches on the city.
const COMMITTED_LANDING: f32 = 1.2;
/// Turns an enemy city that no march could reach is left alone.
const UNREACHABLE_TURNS: u32 = 40;
/// The most fighters the war plan asks the cities to build for one campaign.
const MAX_FORCE: usize = 20;
/// Once marched, a force attacks with this much of an edge.
fn committed_ratio() -> f32 {
    crate::knob("COMMITTED", 1.1)
}

/// How near an enemy stack must be to threaten a city.
const THREAT_RADIUS: i32 = 3;

/// The enemy has the better of the garrison when it is this share of its strength.
const THREAT_RATIO: f32 = 0.8;

/// A relief force is sent to a threatened city only from this far or nearer.
const RELIEF_RANGE: u32 = 12;

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
fn stacks(
    view: &PlayerView,
    scouts: &[Serial],
    ferrying: &[Serial],
) -> Vec<(Position, Vec<hecs::Entity>)> {
    let mut by_pos: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
    for u in view.my_units() {
        if scouts.contains(&view.serial_of(u)) || ferrying.contains(&view.serial_of(u)) {
            continue;
        }
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
    /// An enemy city with no one in it, on this square: walk in.
    WalkIn(Position),
    /// Defended: fight if this force is enough.
    Fight(Vec<hecs::Entity>, f32),
}

pub fn army(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    // Units on a ship's errand or a raid on a site are not the army's to command.
    let mut ferrying: Vec<Serial> = ai
        .ferry
        .as_ref()
        .map(|f| f.group.clone())
        .unwrap_or_default();
    ferrying.extend(ai.raiders.iter().copied());
    let own_cities: Vec<Position> = PlayerView::new(game, me)
        .my_cities()
        .into_iter()
        .map(|(_, p)| p)
        .collect();

    // 1. Hit what stands next to us, when we are clearly the stronger.
    let mut spent: Vec<hecs::Entity> = Vec::new();
    let stack_list = stacks(&PlayerView::new(game, me), &ai.scouts, &ferrying);
    for (pos, units) in stack_list {
        let at_city = own_cities.contains(&pos);
        // One view for the whole stack: nothing changes until a command is sent, and a command
        // that works ends the stack's turn.
        let (ratio, mine, sorties) = {
            let view = PlayerView::new(game, me);
            if view.game_over() {
                return;
            }
            let committed = units
                .iter()
                .any(|&u| ai.marching.contains(&view.serial_of(u)));
            let ratio = if at_city {
                sortie_ratio()
            } else if committed {
                committed_ratio()
            } else {
                attack_ratio()
            };
            let sorties: Vec<Sortie> = neighbours(&view, pos)
                .into_iter()
                .map(|target| match view.hostile_at(target) {
                    None => Sortie::Nothing,
                    Some(defenders) if defenders.is_empty() => Sortie::WalkIn(target),
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
                })
                .collect();
            (ratio, stack_strength(&view, &units), sorties)
        };
        for sortie in sorties {
            match sortie {
                Sortie::Nothing => continue,
                Sortie::WalkIn(target) => {
                    // An empty enemy city: walk in, with the units that can still move.
                    let walkers: Vec<hecs::Entity> = {
                        let view = PlayerView::new(game, me);
                        units
                            .iter()
                            .copied()
                            .filter(|&u| view.unit_moves_left(u) > 0)
                            .collect()
                    };
                    if walkers.is_empty() {
                        continue;
                    }
                    if ai.send(
                        game,
                        Command::MoveStack {
                            units: walkers,
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
                            ai.send(
                                game,
                                Command::AutoResolve {
                                    mana_budget: None,
                                    attackers,
                                    defenders,
                                },
                                events,
                            );
                        }
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
        for (pos, units) in stacks(&view, &ai.scouts, &ferrying) {
            let skip = if own_cities.contains(&pos) {
                garrison_wanted(&view, pos)
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
        let capital = view.fortress().or_else(|| own_cities.first().copied());
        (free, capital)
    };

    // 2b. A city with a hostile stack near and too few defenders pulls the nearest free troops in.
    let free = relieve(ai, game, free, events);
    let Some(capital) = capital else {
        return;
    };

    // 3. Choose (or keep) a city to attack. What is seen of enemy cities is remembered.
    {
        let view = PlayerView::new(game, me);
        for (_, owner, pos) in view.known_cities() {
            if owner != me && view.sees(pos) {
                let strength = stack_strength(&view, &view.hostile_at(pos).unwrap_or_default());
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
        if let (Some(c), Some(f)) = (ai.campaign, race::war_target(ai)) {
            let owner = view.city_at(c).and_then(|e| view.city_owner(e));
            let other = view
                .known_cities()
                .iter()
                .any(|&(_, o, p)| o == f && !ai.unreachable.contains(&p));
            if owner != Some(f) && other {
                ai.campaign = None;
            }
        }
        // The best city that can be walked to; those that cannot are put aside for the ships.
        while ai.campaign.is_none() {
            let Some(c) = best_enemy_city(
                &view,
                capital,
                &ai.unreachable,
                &ai.city_memory,
                race::war_target(ai),
            ) else {
                break;
            };
            if on_foot(&view, capital, c) {
                ai.campaign = Some(c);
            } else {
                ai.unreachable.push(c);
                ai.unreachable_since = game.turn;
            }
        }
        // A landing in preparation is the campaign.
        if let Some(plan) = ai.naval {
            ai.campaign = Some(plan.target);
        }
    }
    let Some(target) = ai.campaign else {
        return;
    };

    // Targets that could not be reached are tried again after a while.
    if game.turn.saturating_sub(ai.unreachable_since) > UNREACHABLE_TURNS {
        ai.unreachable.clear();
    }
    // A march that has gone on too long, or whose target has changed, is called off.
    if game.turn.saturating_sub(ai.march_started) > MARCH_TURNS || ai.march_target != Some(target) {
        ai.marching.clear();
        ai.march_target = Some(target);
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
        let expected = expected_defence(&view, &ai.city_memory, game.turn, target);
        let (at_rally, elsewhere): (Vec<_>, Vec<_>) = free
            .iter()
            .copied()
            .filter(|&u| !ai.marching.contains(&view.serial_of(u)))
            .partition(|&u| {
                view.unit_position(u)
                    .is_some_and(|p| distance(&view, p, rally).is_some_and(|d| d <= GATHER_RADIUS))
            });
        let force = stack_strength(&view, &at_rally);
        (rally, expected, at_rally, elsewhere, force)
    };
    // The port of a landing keeps a slot free: a ship cannot enter a city holding nine units.
    let naval_rally = ai.naval.is_some_and(|p| p.target == target);
    let mut room = if naval_rally {
        let held = PlayerView::new(game, me).garrison(rally).len();
        (f4x_core::unit::MAX_STACK - 1).saturating_sub(held)
    } else {
        usize::MAX
    };
    for u in &elsewhere {
        // A gate is quicker than the road: an Earth Gate city sends its troops to the rally
        // city when that has one too, and an Astral Gate city sends those on the wrong plane
        // across (the original's AI does the second, `AIMOVE.c`).
        let ordered = PlayerView::new(game, me).unit_has_order(*u);
        let here = game.units().unit_position(*u);
        if !ordered {
            if game.can_use_earth_gate(&[*u], rally).is_ok() {
                ai.send(
                    game,
                    Command::EarthGate {
                        units: vec![*u],
                        to: rally,
                    },
                    events,
                );
                continue;
            }
            if here.is_some_and(|p| {
                p.plane != rally.plane
                    && game
                        .economy()
                        .has_gate(p, me, f4x_core::city::gate::ASTRAL_GATE)
            }) {
                ai.send(game, Command::ShiftPlane { units: vec![*u] }, events);
                continue;
            }
        }
        let comes_home = {
            let view = PlayerView::new(game, me);
            !view.unit_has_order(*u)
                && view
                    .unit_position(*u)
                    .is_some_and(|p| p.plane == rally.plane)
                && view.can_reach(&[*u], rally)
        };
        if comes_home && room > 0 {
            room = room.saturating_sub(1);
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
    // Troops for a landing wait at the port; the ship takes them. Those already ashore on the
    // target's land mass march on it when they are strong enough.
    if ai.naval.is_some_and(|p| p.target == target) {
        march_landed(ai, game, &free, target, events);
        return;
    }
    if force >= expected * attack_ratio() + 1.0 && at_rally.len() >= 3 {
        let mut launched: Vec<Serial> = Vec::new();
        let mut refused = false;
        // Units march as stacks: those on one square, nine at most.
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
            for chunk in units.chunks(f4x_core::unit::MAX_STACK) {
                let serials: Vec<Serial> = {
                    let view = PlayerView::new(game, me);
                    chunk.iter().map(|&u| view.serial_of(u)).collect()
                };
                match march(ai, game, chunk, target, events) {
                    March::Sent => launched.extend(serials),
                    March::Refused => refused = true,
                    March::NoWay => {}
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
            ai.marching = launched;
            ai.march_started = game.turn;
        }
    }
}

/// The own cities an enemy stack within [`THREAT_RADIUS`] could take: the strength near is more
/// than [`THREAT_RATIO`] of the strength at home. With the strength still wanted.
fn threats(view: &PlayerView) -> Vec<(Position, f32)> {
    let mut found = Vec::new();
    for (_, pos) in view.my_cities() {
        let near: f32 = view
            .enemies_near(pos, THREAT_RADIUS)
            .iter()
            .map(|(_, units)| stack_strength(view, units))
            .sum();
        if near <= 0.0 {
            continue;
        }
        let home: Vec<_> = view
            .garrison(pos)
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        let held = stack_strength(view, &home);
        if near * THREAT_RATIO > held {
            found.push((pos, near / THREAT_RATIO - held));
        }
    }
    found
}

/// Sends the nearest free troops to each threatened city until they make up the shortfall, and
/// notes the cities for the economy to build in. Returns those still free.
fn relieve(
    ai: &mut Ai,
    game: &mut Game,
    mut free: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) -> Vec<hecs::Entity> {
    let me = ai.player;
    let threatened = threats(&PlayerView::new(game, me));
    ai.threatened = threatened.iter().map(|&(pos, _)| pos).collect();
    for (city, mut short) in threatened {
        let mut sent: Vec<hecs::Entity> = Vec::new();
        {
            let view = PlayerView::new(game, me);
            let mut candidates: Vec<(u32, hecs::Entity)> = free
                .iter()
                .copied()
                .filter(|&u| !ai.marching.contains(&view.serial_of(u)))
                .filter(|&u| !view.unit_has_order(u))
                .filter_map(|u| {
                    let at = view.unit_position(u)?;
                    let d = distance(&view, at, city)?;
                    (d > 0 && d <= RELIEF_RANGE && view.can_reach(&[u], city)).then_some((d, u))
                })
                .collect();
            candidates.sort_by_key(|&(d, u)| (d, view.serial_of(u)));
            for (_, u) in candidates {
                if short <= 0.0 {
                    break;
                }
                short -= unit_strength(&view, u);
                sent.push(u);
            }
        }
        if sent.is_empty() {
            continue;
        }
        for &u in &sent {
            ai.send(
                game,
                Command::GoTo {
                    units: vec![u],
                    to: city,
                },
                events,
            );
        }
        free.retain(|u| !sent.contains(u));
    }
    free
}

pub(crate) fn average_strength(view: &PlayerView) -> f32 {
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

/// Can the AI's soldiers walk to a square beside the city? Judged with a fighter of the capital.
fn on_foot(view: &PlayerView, capital: Position, city: Position) -> bool {
    let Some(walker) = view
        .garrison(capital)
        .into_iter()
        .find(|&u| is_fighter(view, u))
    else {
        return true;
    };
    neighbours(view, city)
        .into_iter()
        .any(|s| view.hostile_at(s).is_none() && view.can_reach(&[walker], s))
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
    let guess = crate::knob("GUESS", GUESS_DEFENDERS) * average_strength(view);
    if view.sees(at) {
        return stack_strength(view, &view.hostile_at(at).unwrap_or_default()) * 1.3;
    }
    match memory.get(&at) {
        Some(&(s, seen)) if turn.saturating_sub(seen) <= MEMORY_TURNS => s * 1.3,
        _ => guess,
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
    ((needed / average_strength(view).max(1.0)).ceil() as usize).clamp(3, MAX_FORCE)
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
        .filter_map(|(_, _, p)| {
            let d = distance(view, from, p)? as f32;
            let held = memory.get(&p).map_or(GUESS_DEFENDERS, |m| m.0 / average);
            Some((d + HELD_WEIGHT * held, p))
        })
        .min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1.y, a.1.x).cmp(&(b.1.y, b.1.x)))
        })
        .map(|(_, p)| p)
}

/// Send a stack towards a square next to the target; it attacks from there next turn.
fn march(
    ai: &mut Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    target: Position,
    events: &mut Vec<Event>,
) -> March {
    let mut way = false;
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
            way = true;
        }
        let sent = ai.send(
            game,
            Command::GoTo {
                units: units.to_vec(),
                to: spot,
            },
            events,
        );
        if sent {
            return March::Sent;
        }
    }
    if way {
        March::Refused
    } else {
        March::NoWay
    }
}

/// How a march order came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum March {
    /// The units are on their way.
    Sent,
    /// A way exists but the order was refused now (no moves left, say): try again next turn.
    Refused,
    /// No square next to the target can be walked to.
    NoWay,
}

/// Troops that came ashore (they can walk to the target, which the rest cannot) march on the city
/// when together they are strong enough for what is expected there.
fn march_landed(
    ai: &mut Ai,
    game: &mut Game,
    free: &[hecs::Entity],
    target: Position,
    events: &mut Vec<Event>,
) {
    let me = ai.player;
    let (landed, force, expected) = {
        let view = PlayerView::new(game, me);
        let open: Vec<Position> = neighbours(&view, target)
            .into_iter()
            .filter(|&s| view.hostile_at(s).is_none())
            .collect();
        // One search for each unit finds every square it can walk to, instead of one path
        // search for each square next to the target.
        let landed: Vec<hecs::Entity> = free
            .iter()
            .copied()
            .filter(|&u| !ai.marching.contains(&view.serial_of(u)))
            .filter(|&u| {
                if open.is_empty() {
                    return false;
                }
                let Some(at) = view.unit_position(u) else {
                    return false;
                };
                let reach = view.reachable(&[u]);
                open.iter().any(|&s| s != at && reach.contains(&s))
            })
            .collect();
        let force = stack_strength(&view, &landed);
        let expected = expected_defence(&view, &ai.city_memory, game.turn, target);
        (landed, force, expected)
    };
    if landed.len() < 2 || force < expected * COMMITTED_LANDING {
        return;
    }
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
        for chunk in units.chunks(f4x_core::unit::MAX_STACK) {
            let serials: Vec<Serial> = {
                let view = PlayerView::new(game, me);
                chunk.iter().map(|&u| view.serial_of(u)).collect()
            };
            if march(ai, game, chunk, target, events) == March::Sent {
                launched.extend(serials);
            }
        }
    }
    if !launched.is_empty() {
        ai.marching = launched;
        ai.march_started = game.turn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY};
    use f4x_core::player::PlayerId;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    /// West's six swordsmen stand next to one of East's spearmen, away from every city.
    fn field() -> (Game, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, at(6, 3), "high_men_swordsmen", 6);
        let defenders = spawn(&mut g, P1, at(7, 3), "high_men_spearmen", 1);
        (g, defenders)
    }

    #[test]
    fn a_wizard_with_a_pact_is_not_marched_on_but_one_at_war_is() {
        use f4x_core::diplomacy::Treaty;
        let mut g = game(COAST_CITY, "high_men");
        g.make_contact(P0, P1);
        let east = at(
            crate::testworld::ENEMY_CITY.0,
            crate::testworld::ENEMY_CITY.1,
        );
        g.explore(P0, east, 3);
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
    fn an_attack_on_a_wizard_who_defends_in_person_is_kept_not_fought() {
        let (mut g, defenders) = field();
        let mut ai = Ai::new(P0);
        ai.leave_defence_to(&[P1]);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        let attacks = ai.take_attacks();
        assert_eq!(attacks.len(), 1, "{events:?}");
        assert_eq!(attacks[0].defenders, defenders);
        assert_eq!(attacks[0].attackers.len(), 6);
        assert!(
            g.units().unit_owner(defenders[0]).is_some(),
            "nothing was fought"
        );
        assert!(ai.take_attacks().is_empty(), "taken once");
    }

    #[test]
    fn without_one_the_same_attack_is_fought_at_once() {
        let (mut g, defenders) = field();
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        army(&mut ai, &mut g, &mut events);
        assert!(ai.take_attacks().is_empty());
        assert!(
            g.units().unit_owner(defenders[0]).is_none(),
            "the spearman fell: {events:?}"
        );
    }

    /// West's city holds one spearman; four of East's swordsmen stand two squares away and three
    /// of West's own wait four squares from the city.
    fn besieged(war: bool) -> (Game, Vec<hecs::Entity>) {
        let mut g = game(COAST_CITY, "high_men");
        spawn(&mut g, P0, at(10, 6), "high_men_spearmen", 1);
        spawn(&mut g, P1, at(8, 6), "high_men_swordsmen", 4);
        let spare = spawn(&mut g, P0, at(6, 3), "high_men_swordsmen", 3);
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
        army(&mut ai, &mut g, &mut events);
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
        let view = PlayerView::new(&g, P0);
        assert!(spare.iter().all(|&u| !view.unit_has_order(u)));
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
    }
}
