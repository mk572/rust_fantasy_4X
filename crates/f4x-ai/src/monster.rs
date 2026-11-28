//! The monsters that roam the map: the guards a new stack drove out of their lair,
//! and any other unit of the monsters' owner that stands in the open. They have no wizard, so
//! this is no wizard's AI: they scout, and when they find a stand nobody holds they settle in it
//! (the game turns the units into its guards when they arrive), and when they find a city they
//! attack it.

use crate::lever::Levered;
use std::collections::HashMap;

use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::MONSTERS;
use f4x_core::world::{Command, Event};

use crate::military::{engage, expected_defence};
use crate::scout::best_target;
use crate::view::{distance, stack_strength};
use crate::Ai;
use f4x_master::Game;

/// Play the monsters' turn.
pub fn roam(game: &mut Game) -> Vec<Event> {
    let mut ai = Ai::new(MONSTERS);
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
    events
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
            crate::military::neighbours(&view, pos),
            view.vacant_zones(),
            stack_strength(&view, &crate::view::party(&view, &units)),
            game.round(),
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
                if ai
                    .send(
                        game,
                        Command::MoveStack {
                            units: units.clone(),
                            to: n,
                        },
                        events,
                    )
                    .is_ok()
                {
                    return;
                }
            }
            Some((defenders, theirs)) => {
                // Only those that can go there attack: land monsters not onto the sea.
                let (band, strength) = {
                    let view = PlayerView::new(game, MONSTERS);
                    let band = crate::view::party(&view, &game.attackers_reaching(&units, n));
                    let strength = stack_strength(&view, &band);
                    (band, strength)
                };
                if !band.is_empty()
                    && game.can_attack_onto(&band, n)
                    && strength >= theirs * game.levers().attack.monster_edge
                {
                    engage(ai, game, band, defenders, events);
                    return;
                }
            }
            _ => {}
        }
    }
    // 2. A stand nobody holds, next to us.
    if let Some(&zone) = neighbours.iter().find(|n| zones.contains(n)) {
        if ai
            .send(
                game,
                Command::MoveStack {
                    units: units.clone(),
                    to: zone,
                },
                events,
            )
            .is_ok()
        {
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
            if mine
                < expected_defence(&view, &HashMap::new(), turn, city)
                    * view.levers().attack.monster_edge
            {
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
            ai.order(game, Command::GoTo { units, to: z }, events);
        }
        Some(Goal::City(c)) => {
            // A band that cannot get to the city stays where it is this turn; the reason is kept
            // in the refused-order log.
            if let Err(why) = march(ai, game, &units, c, events) {
                ai.note_failure(game, format!("march to {c:?}"), format!("{why:?}"));
            }
        }
        None => {
            let target = {
                let view = PlayerView::new(game, MONSTERS);
                best_target(&view, units[0], pos, &[])
            };
            if let Some(to) = target {
                ai.order(game, Command::GoTo { units, to }, events);
            }
        }
    }
}

/// Why a party could not be sent to a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarchError {
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

/// Send a band of monsters to the free square next to `site` that it can reach and is nearest to
/// it; the error says why it could not be sent.
fn march(
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
        if !view.reaches_any(units, &open) {
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
                crate::SendError::Rejected(f4x_core::world::Rejection::Move(why))
                | crate::SendError::Stopped(why),
            ) => {
                refused = Some(why);
            }
            Err(_) => {}
        }
    }
    Err(refused.map_or(MarchError::Unreachable, MarchError::Refused))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, INLAND_CITY, P0};
    use f4x_core::feature::entity::Placement;
    use f4x_core::feature::lair::Lair;
    use f4x_def::half_int::HalfInt;
    use f4x_master::Game;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn end_turn(w: &mut Game) {
        // The wizard's soldiers eat: put the citizens on the farms, or they starve at the turn.
        w.balance_farmers(P0);
        w.play_round();
    }

    /// The monsters know the land round `p`, as they would after walking there.
    fn know(w: &mut Game, p: Position) {
        w.explore(MONSTERS, p, HalfInt::from_int(4));
    }

    #[test]
    fn monsters_next_to_an_empty_city_walk_in_and_wreck_it() {
        let mut w = game(INLAND_CITY, 8 /* high_men */);
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(
            &mut w,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            174, /* zombies */
            2,
        );
        know(&mut w, city);
        end_turn(&mut w);
        let events = roam(&mut w);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityRampaged { pos, .. } if *pos == city)),
            "{events:?}"
        );
        // The city is wrecked and gone, or still the wizard's: never the monsters'.
        let owner = w
            .economy()
            .city_at(city)
            .and_then(|c| w.economy().city_owner(c));
        assert!(
            owner.is_none() || owner == Some(P0),
            "monsters never hold a city: {owner:?}"
        );
    }

    #[test]
    fn monsters_that_see_a_defended_city_they_can_beat_attack_it() {
        let mut w = game(INLAND_CITY, 8 /* high_men */);
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut w, P0, city, 104 /* high_men_spearmen */, 1);
        spawn(
            &mut w,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            164, /* great_drake */
            3,
        );
        know(&mut w, city);
        end_turn(&mut w);
        w.set_human(None);
        let events = roam(&mut w);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::BattleStarted { attacker, defender, site }
                    if *attacker == MONSTERS && *defender == P0 && *site == city
            )),
            "the monsters attack the city: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::BattleEnded { winner, loser } if *winner == MONSTERS && *loser == P0
            )),
            "three drakes beat one spearman: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::CityRampaged { pos, .. } if *pos == city)),
            "the winners wreck the city: {events:?}"
        );
        assert!(w.units().player_units(P0).is_empty(), "the spearman fell");
    }

    #[test]
    fn an_attack_on_the_human_waits_for_its_answer() {
        let mut w = game(INLAND_CITY, 8 /* high_men */);
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut w, P0, city, 104 /* high_men_spearmen */, 1);
        spawn(
            &mut w,
            MONSTERS,
            at(INLAND_CITY.0, INLAND_CITY.1 - 1),
            164, /* great_drake */
            3,
        );
        know(&mut w, city);
        end_turn(&mut w);
        w.set_human(Some(P0));
        roam(&mut w);
        assert!(w.waiting_attack().is_some_and(|a| a.defender == P0));
        assert!(!w.in_battle());
    }

    #[test]
    fn monsters_settle_in_a_cleared_node_they_have_seen() -> TestResult {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let node_at = at(5, 9);
        let feature = w
            .content()
            .features
            .id_numbered(4)
            .ok_or("chaos_node is shipped")?;
        let site = w.spawn_feature(Placement {
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
                hurt: Vec::new(),
                apparent: None,
            }),
        });
        spawn(&mut w, MONSTERS, at(5, 8), 174 /* zombies */, 2);
        know(&mut w, node_at);
        end_turn(&mut w);
        let events = roam(&mut w);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::MonstersSettled { count: 2, .. })),
            "{events:?}"
        );
        assert!(
            w.economy()
                .lair_of(site)
                .ok_or("the node has a lair")?
                .intact
        );
        assert!(w.units().player_units(MONSTERS).is_empty());
        Ok(())
    }

    #[test]
    fn monsters_that_know_nothing_go_scouting() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut w, MONSTERS, at(5, 5), 174 /* zombies */, 2);
        w.explore(MONSTERS, at(5, 5), HalfInt::from_raw(3));
        end_turn(&mut w);
        let events = roam(&mut w);
        assert!(
            events.iter().any(|e| matches!(e, Event::UnitsMoved { .. })),
            "{events:?}"
        );
        let view = PlayerView::new(&w, MONSTERS);
        assert_ne!(view.unit_position(units[0]), Some(at(5, 5)));
    }

    #[test]
    fn a_stack_too_weak_for_the_city_it_sees_does_not_march_on_it() {
        let mut w = game(INLAND_CITY, 8 /* high_men */);
        let city = at(INLAND_CITY.0, INLAND_CITY.1);
        spawn(&mut w, P0, city, 113 /* high_men_paladins */, 6);
        spawn(&mut w, MONSTERS, at(9, 9), 174 /* zombies */, 1);
        know(&mut w, city);
        end_turn(&mut w);
        let events = roam(&mut w);
        assert!(!events
            .iter()
            .any(|e| matches!(e, Event::CityRampaged { .. })));
    }
}
