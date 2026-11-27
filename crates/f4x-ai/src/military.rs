//! What the war jobs share (`war.rs`, `strike.rs`, `site.rs`, `monster.rs`): the ratios an attack
//! is judged by, the stacks of a square, the battle and the strength a defence is expected to have.

//! What the war jobs share (`war.rs`, `strike.rs`, `site.rs`, `monster.rs`): the ratios an attack
//! is judged by, the stacks of a square, the battle and the strength a defence is expected to have.

use std::collections::HashMap;

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::world::{Command, Event, World};

use crate::view::{fighting_strength, is_fighter, unit_strength};
use crate::Ai;
use f4x_master::Game;

/// The AI attacks when its force is this much stronger than what it expects to meet.
pub(crate) fn attack_ratio() -> f32 {
    crate::knob("ATTACK", 1.4)
}
/// The edge over what it expects to meet that an army wants before it marches or strikes, by the
/// phase of the game and the lead of the wizard (task 697). The numbers come from the ratings, not
/// from soaks: in `duel teams` the side with the larger sum of unit ratings wins 74% of the time at
/// 1.2 times the other's, 87% at 1.5 and 95% at 2 (`docs/combat-rating.md`, task 721). While it
/// builds up the wizard marches only for a near-sure win; in the secure phase for a good one; in
/// the win phase it takes the odds, and the more it leads the fewer it asks, down to a near-even
/// fight at three times the best rival's power.
pub(crate) fn edge(phase: crate::phase::Phase, lead: f32) -> f32 {
    match phase {
        crate::phase::Phase::Build => crate::knob("EDGE_BUILD", 2.0),
        crate::phase::Phase::Secure => crate::knob("EDGE_SECURE", 1.5),
        crate::phase::Phase::Win => {
            let (first, last) = (crate::knob("EDGE_WIN", 1.3), crate::knob("EDGE_LEAD", 1.05));
            let share = ((lead - 1.0) / 2.0).clamp(0.0, 1.0);
            first + (last - first) * share
        }
    }
}

/// [`edge`] for this wizard now.
pub(crate) fn campaign_ratio(world: &World, me: f4x_core::player::PlayerId, round: u32) -> f32 {
    let phase = crate::phase::phase(&PlayerView::new(world, me), round);
    edge(phase, crate::statecraft::lead_of(world, me))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::Phase;

    /// The edge asked grows with caution: most while building up, least in the win phase.
    #[test]
    fn the_edge_asked_falls_from_the_build_phase_to_the_win_phase() {
        let (build, secure, win) = (
            edge(Phase::Build, 1.0),
            edge(Phase::Secure, 1.0),
            edge(Phase::Win, 1.0),
        );
        assert!(
            build > secure && secure > win && win > 1.0,
            "{build} {secure} {win}"
        );
    }

    /// In the win phase a lead lowers the edge, never below even odds; the other phases do not read it.
    #[test]
    fn a_lead_lowers_the_edge_in_the_win_phase_only() {
        let (level, ahead, far_ahead, beyond) = (
            edge(Phase::Win, 1.0),
            edge(Phase::Win, 2.0),
            edge(Phase::Win, 3.0),
            edge(Phase::Win, 30.0),
        );
        assert!(
            level > ahead && ahead > far_ahead,
            "{level} {ahead} {far_ahead}"
        );
        assert_eq!(far_ahead, beyond, "the fall stops");
        assert!(far_ahead > 1.0);
        assert_eq!(edge(Phase::Secure, 3.0), edge(Phase::Secure, 1.0));
        assert_eq!(edge(Phase::Build, 3.0), edge(Phase::Build, 1.0));
    }

    /// The same game asks a bolder edge late than early.
    #[test]
    fn the_same_wizard_asks_a_smaller_edge_late_in_the_game_than_early() {
        let mut w = crate::testworld::game(crate::testworld::COAST_CITY, 8 /* high_men */);
        w.set_round(1);
        let early = campaign_ratio(&w, crate::testworld::P0, w.round());
        w.set_round(400);
        let late = campaign_ratio(&w, crate::testworld::P0, w.round());
        assert!(late < early, "{late} against {early}");
    }

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
