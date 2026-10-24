//! The monsters that roam the map (task 376): the guards a new stack drove out of their lair,
//! and any other unit of the monsters' owner that stands in the open. They have no wizard, so
//! this is no wizard's AI: they scout, and when they find a stand nobody holds they settle in it
//! (the game turns the units into its guards when they arrive), and when they find a city they
//! attack it.

use std::collections::HashMap;

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::{PlayerId, MONSTERS};

use crate::military::expected_defence;
use crate::neutral::{around, march, strike};
use crate::scout::best_target;
use crate::view::{distance, stack_strength};
use crate::{Ai, Attack};

/// Monsters attack a city when they are this much stronger than what they expect to meet.
const EDGE: f32 = 1.0;

/// Play the monsters' turn. A battle against a player in `defended_by` is returned, for the
/// host to play out.
pub fn roam(game: &mut Game, defended_by: &[PlayerId]) -> (Vec<Event>, Vec<Attack>) {
    let mut ai = Ai::new(MONSTERS);
    ai.leave_defence_to(defended_by);
    let mut events = Vec::new();
    let stacks: Vec<(Position, Vec<hecs::Entity>)> = {
        let view = PlayerView::new(game, MONSTERS);
        let mut by_square: HashMap<Position, Vec<hecs::Entity>> = HashMap::new();
        for u in view.my_units() {
            if let Some(p) = view.unit_position(u) {
                by_square.entry(p).or_default().push(u);
            }
        }
        let mut list: Vec<_> = by_square.into_iter().collect();
        for (_, units) in &mut list {
            view.sort_by_serial(units);
        }
        list.sort_by_key(|(_, units)| view.serial_of(units[0]));
        list
    };
    for (pos, units) in stacks {
        act(&mut ai, game, pos, units, &mut events);
    }
    let attacks = ai.take_attacks();
    (events, attacks)
}

/// What one stack does: hit a city next to it, settle in a stand next to it, or head for the
/// nearest city or stand it knows, or look for new ground.
fn act(
    ai: &mut Ai,
    game: &mut Game,
    pos: Position,
    units: Vec<hecs::Entity>,
    events: &mut Vec<Event>,
) {
    // Units lost in an earlier fight this turn are gone.
    let units: Vec<hecs::Entity> = {
        let view = PlayerView::new(game, MONSTERS);
        units
            .into_iter()
            .filter(|&u| view.unit_position(u) == Some(pos) && view.unit_moves_left(u) > 0)
            .collect()
    };
    if units.is_empty() {
        return;
    }
    // 1. A city next to us.
    let (neighbours, zones, mine, turn) = {
        let view = PlayerView::new(game, MONSTERS);
        (
            around(&view, pos),
            view.vacant_zones(),
            stack_strength(&view, &units),
            view.turn(),
        )
    };
    for &n in &neighbours {
        let defence = {
            let view = PlayerView::new(game, MONSTERS);
            match (view.city_at(n), view.hostile_at(n)) {
                (Some(_), Some(defenders)) => {
                    Some((defenders, expected_defence(&view, &HashMap::new(), turn, n)))
                }
                _ => None,
            }
        };
        match defence {
            Some((defenders, _)) if defenders.is_empty() => {
                if ai.send(
                    game,
                    Command::MoveStack {
                        units: units.clone(),
                        to: n,
                    },
                    events,
                ) {
                    return;
                }
            }
            Some((defenders, theirs)) if mine >= theirs * EDGE => {
                strike(ai, game, units.clone(), defenders, events);
                return;
            }
            _ => {}
        }
    }
    // 2. A stand nobody holds, next to us.
    if let Some(&zone) = neighbours.iter().find(|n| zones.contains(n)) {
        if ai.send(
            game,
            Command::MoveStack {
                units: units.clone(),
                to: zone,
            },
            events,
        ) {
            return;
        }
    }
    // 3. On the way already.
    {
        let view = PlayerView::new(game, MONSTERS);
        if units.iter().any(|&u| view.unit_has_order(u)) {
            return;
        }
    }
    // 4. The nearest city we are strong enough for or stand we know; else new ground.
    enum Goal {
        Zone(Position),
        City(Position),
    }
    let goal = {
        let view = PlayerView::new(game, MONSTERS);
        let mut best: Option<(u32, u8, Position, bool)> = None;
        for &z in &zones {
            if z == pos {
                continue;
            }
            let key = (distance(&view, pos, z).unwrap_or(u32::MAX), 0u8, z, true);
            if best.is_none_or(|b| (key.0, key.1, key.2.y, key.2.x) < (b.0, b.1, b.2.y, b.2.x)) {
                best = Some(key);
            }
        }
        for (_, _, city) in view.known_cities() {
            if mine < expected_defence(&view, &HashMap::new(), turn, city) * EDGE {
                continue;
            }
            let key = (
                distance(&view, pos, city).unwrap_or(u32::MAX),
                1u8,
                city,
                false,
            );
            if best.is_none_or(|b| (key.0, key.1, key.2.y, key.2.x) < (b.0, b.1, b.2.y, b.2.x)) {
                best = Some(key);
            }
        }
        match best {
            Some((_, _, p, true)) => Some(Goal::Zone(p)),
            Some((_, _, p, false)) => Some(Goal::City(p)),
            None => None,
        }
    };
    match goal {
        Some(Goal::Zone(z)) => {
            ai.send(game, Command::GoTo { units, to: z }, events);
        }
        Some(Goal::City(c)) => {
            march(ai, game, &units, c, events);
        }
        None => {
            let target = {
                let view = PlayerView::new(game, MONSTERS);
                best_target(&view, units[0], pos, &[])
            };
            if let Some(to) = target {
                ai.send(game, Command::GoTo { units, to }, events);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::feature::entity::Placement;
    use f4x_core::feature::lair::Lair;

    fn end_turn(g: &mut Game) {
        // The wizard's soldiers eat: put the citizens on the farms, or they starve at the turn.
        g.balance_farmers(P0);
        g.apply(Command::EndTurn);
    }

    /// The monsters know the land round `p`, as they would after walking there.
    fn know(g: &mut Game, p: Position) {
        g.explore(MONSTERS, p, 8);
    }

    #[test]
    fn monsters_next_to_an_empty_city_walk_in_and_wreck_it() {
        let mut g = game(INLAND_CITY, "high_men");
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(
            &mut g,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            "zombies",
            2,
        );
        know(&mut g, city);
        end_turn(&mut g);
        let (events, _) = roam(&mut g, &[]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityRampaged { pos, .. } if *pos == city)),
            "{events:?}"
        );
        if let Some(c) = g.economy().city_at(city) {
            assert_eq!(
                g.economy().city_owner(c),
                Some(P0),
                "monsters never hold a city"
            );
        }
    }

    #[test]
    fn monsters_that_see_a_defended_city_they_can_beat_attack_it() {
        let mut g = game(INLAND_CITY, "high_men");
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut g, P0, city, "high_men_spearmen", 1);
        spawn(
            &mut g,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            "great_drake",
            3,
        );
        know(&mut g, city);
        end_turn(&mut g);
        let before = g.units().player_units(P0).len();
        let (events, attacks) = roam(&mut g, &[]);
        assert!(attacks.is_empty());
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::CityRampaged { .. } | Event::Battle(_) | Event::UnitsMoved { .. }
            )) || g.units().player_units(P0).len() != before,
            "{events:?}"
        );
    }

    #[test]
    fn an_attack_on_the_human_is_kept_for_the_host() {
        let mut g = game(INLAND_CITY, "high_men");
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut g, P0, city, "high_men_spearmen", 1);
        spawn(
            &mut g,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            "great_drake",
            3,
        );
        know(&mut g, city);
        end_turn(&mut g);
        let (_, attacks) = roam(&mut g, &[P0]);
        assert_eq!(attacks.len(), 1);
        assert!(!g.in_battle());
    }

    #[test]
    fn monsters_settle_in_a_cleared_node_they_have_seen() {
        let mut g = game(COAST_CITY, "high_men");
        let node_at = at(5, 9);
        let feature = g.content().features.id_of("chaos_node").unwrap();
        let site = g.spawn_feature(Placement {
            at: node_at,
            feature,
            node: None,
            lair: Some(Lair {
                intact: false,
                guards: Vec::new(),
                gold: 0,
                mana: 0,
                items: Vec::new(),
                prisoner: false,
                special: 0,
                fought_by: Vec::new(),
            }),
        });
        spawn(&mut g, MONSTERS, at(5, 8), "zombies", 2);
        know(&mut g, node_at);
        end_turn(&mut g);
        let (events, _) = roam(&mut g, &[]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MonstersSettled { count: 2, .. })),
            "{events:?}"
        );
        assert!(g.economy().lair_of(site).unwrap().intact);
        assert!(g.units().player_units(MONSTERS).is_empty());
    }

    #[test]
    fn monsters_that_know_nothing_go_scouting() {
        let mut g = game(COAST_CITY, "high_men");
        let units = spawn(&mut g, MONSTERS, at(5, 5), "zombies", 2);
        g.explore(MONSTERS, at(5, 5), 3);
        end_turn(&mut g);
        let (events, _) = roam(&mut g, &[]);
        assert!(
            events.iter().any(|e| matches!(e, Event::UnitsMoved { .. })),
            "{events:?}"
        );
        let view = PlayerView::new(&g, MONSTERS);
        assert_ne!(view.unit_position(units[0]), Some(at(5, 5)));
    }

    #[test]
    fn a_stack_too_weak_for_the_city_it_sees_does_not_march_on_it() {
        let mut g = game(INLAND_CITY, "high_men");
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut g, P0, city, "high_men_paladins", 6);
        let units = spawn(&mut g, MONSTERS, at(9, 9), "zombies", 1);
        know(&mut g, city);
        end_turn(&mut g);
        let (events, _) = roam(&mut g, &[]);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::CityRampaged { .. })));
        let _ = units;
    }
}
