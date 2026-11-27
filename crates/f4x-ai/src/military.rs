//! What the war jobs share (`war.rs`, `attack.rs`, `site.rs`, `monster.rs`): the stacks of a
//! square, the battle and the strength a defence is expected to have.

use crate::lever::Levered;
use std::collections::HashMap;

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event, World};

use crate::view::{fighting_strength, is_fighter, unit_strength};
use crate::Ai;
use f4x_master::Game;

/// The eight squares round a square, as steps.
pub(crate) const NEIGHBOURS: [(i32, i32); 8] = [
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

/// Whether the defenders belong to a wizard the AI has a pact or an alliance with: the game refuses an
/// attack on them until a player has broken the treaty, and the AI declares war (which breaks it) before it
/// chooses to attack, never by a stray order (task 776).
pub(crate) fn held_by_treaty(ai: &Ai, world: &World, defenders: &[hecs::Entity]) -> bool {
    defenders
        .first()
        .and_then(|&d| world.units().unit_owner(d))
        .is_some_and(|owner| world.bound_by_treaty(ai.player, owner))
}

/// Attack: the battle is fought at once, the mana the wizard may spend in it set by `budget.rs`
/// (a hard fight for a city gets all it may, a sure win in the open little). An attack on the
/// human waits for its answer instead (`f4x-master`, `Game::waiting_attack`).
pub(crate) fn engage(
    ai: &mut Ai,
    game: &mut Game,
    attackers: Vec<hecs::Entity>,
    defenders: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    if held_by_treaty(ai, game, &defenders) {
        return;
    }
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

/// The own cities an enemy stack within `near_city_radius` could take: the strength near is more
/// than `threat_ratio` of the strength at home. With the strength still wanted.
pub(crate) fn threats(view: &PlayerView) -> Vec<(Position, f32)> {
    let radius = view.levers().shared.near_city_radius;
    let ratio = view.levers().attack.threat_ratio;
    let mut found = Vec::new();
    for (_, pos) in view.my_cities() {
        let near: f32 = view
            .enemies_near(pos, radius)
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
        if near * ratio > held {
            found.push((pos, near / ratio - held));
        }
    }
    found
}

/// The mean strength of the AI's fighters (20 with none). Worked out once for a view: it walks
/// every own unit, and the plans ask for it once per target.
pub(crate) fn average_strength(view: &PlayerView) -> f32 {
    let mean = view.memo(|| {
        let units: Vec<_> = view
            .my_units()
            .into_iter()
            .filter(|&u| is_fighter(view, u))
            .collect();
        if units.is_empty() {
            return None;
        }
        Some(units.iter().map(|&u| unit_strength(view, u)).sum::<f32>() / units.len() as f32)
    });
    mean.unwrap_or(20.0)
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
    let levers = view.levers();
    let in_city = levers.shared.city_defence_factor;
    // What the enemy city screen shows a human of a city it has explored: the units in it.
    if let Some(report) = view.city_at(at).and_then(|c| view.city_report(c)) {
        return fighting_strength(view, &report.garrison) * in_city;
    }
    if view.sees(at) {
        return fighting_strength(view, &view.hostile_at(at).unwrap_or_default()) * in_city;
    }
    match memory.get(&at) {
        Some(&(s, seen)) if turn.saturating_sub(seen) <= levers.attack.memory_turns => s * in_city,
        // A guess of a few average units: only now is the average worked out.
        _ => levers.attack.guess * average_strength(view),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An attack on a pact or alliance partner is never sent (the game would refuse it); at war it is.
    #[test]
    fn no_attack_is_sent_on_a_treaty_partner() {
        use crate::testworld::{at, spawn, COAST_CITY, P0, P1};
        use f4x_core::diplomacy::Treaty;
        let mut w = crate::diplo_b_stance::tests::pair(8, 8, 100);
        let units = |w: &World, p| w.units_of(p).len();
        assert!(units(&w, P0) > 0 && units(&w, P1) > 0);
        let attackers: Vec<hecs::Entity> = w
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| w.units().unit_owner(u) == Some(P0))
            .collect();
        spawn(&mut w, P1, at(COAST_CITY.0 + 1, COAST_CITY.1), 95, 1);
        let defenders: Vec<hecs::Entity> = w
            .units()
            .all_units()
            .into_iter()
            .filter(|&u| w.units().unit_owner(u) == Some(P1))
            .collect();
        for treaty in [Treaty::Pact, Treaty::Alliance] {
            w.start_treaty(P0, P1, treaty);
            let mut ai = Ai::new(P0);
            let mut events = Vec::new();
            engage(
                &mut ai,
                &mut w,
                attackers.clone(),
                defenders.clone(),
                &mut events,
            );
            assert!(events.is_empty(), "{treaty:?}: {events:?}");
        }
        w.declare_war(P0, P1);
        let mut ai = Ai::new(P0);
        let mut events = Vec::new();
        engage(&mut ai, &mut w, attackers, defenders, &mut events);
        assert!(!events.is_empty(), "at war the attack is sent");
    }
}
