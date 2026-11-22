//! What the roaming monsters (`monster.rs`) and the sites (`site.rs`) share: marching a party to a
//! site, assaulting it, striking a stack next to it. (The independent cities have no AI of their
//! own: the neutral player of each race plays as any AI does, task 522.)

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::distance;
use crate::{Ai, Attack};

/// How strong the guards of a site are, as `unit_strength` measures units, when the player has
/// fought for the site once; `None` when it does not know them.
pub(crate) fn guard_strength(view: &PlayerView, site: Position) -> Option<f32> {
    guard_parts(view, site).map(|(all, _)| all)
}

/// The guards of a site as [`guard_strength`] measures them, and of that the part with weapon
/// immunity (`weapon_immunity`, 72): normal weapons barely hurt those.
pub(crate) fn guard_parts(view: &PlayerView, site: Position) -> Option<(f32, f32)> {
    let rules = &view.content().rules;
    let guards = view.lair_guards(site)?;
    let (mut all, mut immune) = (0.0, 0.0);
    for (id, n) in &guards {
        let def = rules.units.get(*id);
        let worth = f4x_battle::power::def_power(def, rules) * *n as f32;
        all += worth;
        if def.abilities.contains(&f4x_core::rules::Ability::new(
            72, /* weapon_immunity */
        )) {
            immune += worth;
        }
    }
    Some((all, immune))
}

/// Why a party could not be sent to a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarchError {
    /// There is no party.
    NoUnits,
    /// No square next to the site is free of units and lairs.
    NoOpenSquare,
    /// The party cannot walk to any square next to the site (the sea, guarded ground, a full
    /// square in the way).
    Unreachable,
    /// A way exists but the game refused the order, for the reason given.
    Refused(f4x_core::unit::movement::MoveError),
}

/// The facts of a march the game refused for want of room: how many the party is and how full
/// each square beside the site is (the limit of a square is `max_tile`).
pub(crate) fn march_detail(ai: &Ai, game: &Game, units: &[hecs::Entity], site: Position) -> String {
    let view = PlayerView::new(game, ai.player);
    let squares: Vec<String> = crate::military::neighbours(&view, site)
        .into_iter()
        .filter(|&p| !game.economy().army_at(p).is_empty())
        .map(|p| {
            format!(
                "({}, {}) holds {}",
                p.x,
                p.y,
                game.economy().army_at(p).len()
            )
        })
        .collect();
    format!(
        "party of {} to ({}, {}), room for {} on a square; beside it: {}",
        units.len(),
        site.x,
        site.y,
        game.max_tile(),
        if squares.is_empty() {
            "no square with units".to_string()
        } else {
            squares.join(", ")
        }
    )
}

/// Send a party to the free square next to `site` that it can reach and is nearest to it; the
/// error says why it could not be sent.
pub(crate) fn march(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> Result<(), MarchError> {
    march_with(
        &mut crate::view::ReachCache::default(),
        ai,
        game,
        units,
        site,
        events,
    )
}

/// [`march`] with answers to "can it walk there" kept by the caller for as long as nothing moves,
/// for parties that go through the same sites one after the other.
pub(crate) fn march_with(
    reached: &mut crate::view::ReachCache,
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) -> Result<(), MarchError> {
    let Some(&first) = units.first() else {
        return Err(MarchError::NoUnits);
    };
    let spots = {
        let view = PlayerView::new(game, ai.player);
        let from = view.unit_position(first).unwrap_or(site);
        let open: Vec<Position> = crate::military::neighbours(&view, site)
            .into_iter()
            .filter(|&p| view.hostile_at(p).is_none() && !view.has_lair(p) && p != from)
            .collect();
        // One search that stops at the first square found tells whether any of them can be
        // reached: when none can (a site across the water) the party does not search once for
        // each square, each of them over the whole plane.
        if open.is_empty() {
            return Err(MarchError::NoOpenSquare);
        }
        if !reached.can_reach_round(&view, units, site, &open) {
            return Err(MarchError::Unreachable);
        }
        let mut spots = open;
        spots.sort_by_key(|&p| (distance(&view, from, p), p.y, p.x));
        spots
    };
    // The nearest square that can be reached, asked one at a time: the first that can is the one
    // the party is sent to, so the others need no search.
    let mut refused = None;
    for spot in spots {
        if !PlayerView::new(game, ai.player).can_reach(units, spot) {
            continue;
        }
        // A square with no room for the party is left alone (a way exists: it may be free later).
        if !Ai::room_at(game, spot, units.len()) {
            refused = Some(f4x_core::unit::movement::MoveError::StackFull);
            continue;
        }
        let sent = ai.send(
            game,
            Command::GoTo {
                units: units.to_vec(),
                to: spot,
            },
            events,
        );
        // The reason the game gave, for the caller; a square that cannot take the party (full,
        // no room) is tried with the next one round the site.
        match sent {
            Ok(()) => return Ok(()),
            Err(
                crate::SendError::Rejected(f4x_core::game::Rejection::Move(why))
                | crate::SendError::Stopped(why),
            ) => {
                refused = Some(why);
            }
            Err(_) => {}
        }
    }
    Err(refused.map_or(MarchError::Unreachable, MarchError::Refused))
}

/// Assault the site with the units that still have moves, and fight the battle out.
pub(crate) fn assault(
    ai: &Ai,
    game: &mut Game,
    units: &[hecs::Entity],
    site: Position,
    events: &mut Vec<Event>,
) {
    let fighters: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, ai.player);
        units
            .iter()
            .copied()
            .filter(|&u| view.unit_moves_left(u) > 0)
            .collect()
    };
    if fighters.is_empty() {
        return;
    }
    // The guards cannot be seen from outside: a site that has none left refuses, and the stack
    // then walks onto it (below).
    let sent = ai
        .probe(
            game,
            Command::EnterSite {
                units: fighters.clone(),
                to: site,
            },
            events,
        )
        .is_ok();
    if !sent {
        // A site nobody guards (the guards cannot be seen from outside) is not fought for: the
        // stack walks onto it and takes the treasure.
        ai.order(
            game,
            Command::GoTo {
                units: fighters,
                to: site,
            },
            events,
        );
        return;
    }
    // The battle AI plays both sides; a battle that stalls ends with the active side fleeing.
    for _ in 0..20_000 {
        let Some(command) = game.next_battle_command() else {
            break;
        };
        if !ai.send(game, Command::Battle(command), events).is_ok() {
            break;
        }
    }
    while game.in_battle() {
        if !ai
            .send(
                game,
                Command::Battle(f4x_battle::battle::BattleCommand::Flee),
                events,
            )
            .is_ok()
        {
            break;
        }
    }
}

/// Fight the battle the AI means to fight: the human defends in person when `ai` leaves the
/// defence to them (the host plays it out), else the battle AI plays both sides.
pub(crate) fn strike(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    if crate::military::held_by_treaty(ai, game, &defenders) {
        return;
    }
    let in_person = defenders
        .first()
        .and_then(|&d| game.units().unit_owner(d))
        .is_some_and(|owner| ai.defended_by.contains(&owner));
    if in_person {
        ai.attacks.push(Attack {
            attackers,
            defenders,
        });
    } else {
        crate::military::resolve(ai, game, attackers, defenders, events);
    }
}
