//! What the war jobs share (`war.rs`, `strike.rs`, `site.rs`, `monster.rs`): the ratios an attack
//! is judged by, the stacks of a square, the battle and the strength a defence is expected to have.

//! What the war jobs share (`war.rs`, `strike.rs`, `site.rs`, `monster.rs`): the ratios an attack
//! is judged by, the stacks of a square, the battle and the strength a defence is expected to have.

use std::collections::HashMap;

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;

use crate::view::{fighting_strength, is_fighter, unit_strength};
use crate::Ai;

/// The AI attacks when its force is this much stronger than what it expects to meet.
pub(crate) fn attack_ratio() -> f32 {
    crate::knob("ATTACK", 1.4)
}
/// An attack out of a city's garrison needs a much bigger edge.
pub(crate) fn sortie_ratio() -> f32 {
    crate::knob("SORTIE", 2.5)
}
/// Strength assumed for a defence the AI cannot see.
pub(crate) const GUESS_DEFENDERS: f32 = 3.0;
/// How long a remembered garrison is trusted, in turns.
pub(crate) const MEMORY_TURNS: u32 = 80;
/// Squares of walking one average unit of garrison is worth when choosing a target.
pub(crate) const HELD_WEIGHT: f32 = 1.5;
/// A force that has marched and not fought for this many turns goes home to gather again.
pub(crate) const MARCH_TURNS: u32 = 15;
/// What a landing party wants over the expected defence before it marches on the city.
pub(crate) const COMMITTED_LANDING: f32 = 1.2;
/// Turns an enemy city that no march could reach is left alone.
pub(crate) const UNREACHABLE_TURNS: u32 = 40;
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
