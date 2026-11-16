//! Wounded units rest (task 502). A unit hurt below [`REST_PCT`] of its hit points is no use in a
//! raid or a march: it heals 5% a turn in the field, 10% in a city, 17% in a city with an Animists'
//! Guild and 20% more beside a Healer. So it goes to the nearest healer's stack within
//! [`HEALER_RANGE`] squares, else to the nearest own city, and the plans that pick troops
//! (`military.rs`, `site.rs`, `garrison.rs`) leave it out until it has mended.
//!
//! Tests: below, on shipped units.

use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_def::rules::Ability;

use crate::view::{distance, is_fighter};
use crate::Ai;

/// A unit below this share of its hit points rests.
pub(crate) const REST_PCT: u32 = 60;
/// How far a wounded unit walks to a stack with a Healer, in squares.
const HEALER_RANGE: u32 = 4;
/// The Healer ability of the unit table.
const HEALER: Ability = Ability::new(27);

/// Is the unit hurt enough to rest?
pub(crate) fn needs_rest(view: &PlayerView, unit: hecs::Entity) -> bool {
    let (hits, max) = view.unit_hits(unit);
    max > 0 && hits * 100 < max * REST_PCT
}

/// Send the wounded units that are out in the field to a healer or home, and let go of the tasks
/// they were on.
pub fn run(ai: &mut Ai, game: &mut Game, events: &mut Vec<Event>) {
    let me = ai.player;
    let orders: Vec<(hecs::Entity, Position)> = {
        let view = PlayerView::new(game, me);
        let cities: Vec<Position> = view.my_cities().into_iter().map(|(_, p)| p).collect();
        let busy = |serial| {
            ai.sea_party.contains(&serial)
                || ai.roles.has_role(serial)
                || ai.ferries.iter().any(|f| f.group.contains(&serial))
        };
        let healers: Vec<Position> = view
            .my_units()
            .into_iter()
            .filter(|&u| view.unit_has_ability(u, HEALER) && !needs_rest(&view, u))
            .filter_map(|u| view.unit_position(u))
            .collect();
        let mut reached = crate::view::ReachCache::default();
        let mut out = Vec::new();
        for u in view.my_units() {
            if !is_fighter(&view, u) || !needs_rest(&view, u) || busy(view.serial_of(u)) {
                continue;
            }
            let Some(at) = view.unit_position(u) else {
                continue;
            };
            // In a city it mends where it stands, and on its way home it is left alone (a raid or a
            // march it was on has an order too, which it gives up).
            let serial = view.serial_of(u);
            let on_a_raid = ai.raiders.contains(&serial) || ai.marching.contains(&serial);
            if cities.contains(&at) || (view.unit_has_order(u) && !on_a_raid) {
                continue;
            }
            let near = |&p: &Position| distance(&view, at, p).map(|d| (d, p.y, p.x));
            let mut nearest = |list: &[Position], range: u32| {
                list.iter()
                    // A square with no room for one more is no place to send it (`StackFull`).
                    .filter(|&&p| view.garrison(p).len() < view.max_tile())
                    .filter(|&&p| p != at && near(&p).is_some_and(|k| k.0 <= range))
                    .filter(|&&p| reached.can_reach(&view, &[u], p))
                    .min_by_key(|p| near(p))
                    .copied()
            };
            // A square with a healer is the first choice when one is near.
            let healer = nearest(&healers, HEALER_RANGE);
            let to = healer.or_else(|| nearest(&cities, u32::MAX));
            if let Some(to) = to {
                out.push((u, to));
            }
        }
        out
    };
    for (unit, to) in orders {
        let serial = PlayerView::new(game, me).serial_of(unit);
        ai.raiders.retain(|s| *s != serial);
        ai.marching.retain(|s| *s != serial);
        ai.raid_targets.remove(&serial);
        ai.order(game, Command::ClearOrders { units: vec![unit] }, events);
        ai.order(
            game,
            Command::GoTo {
                units: vec![unit],
                to,
            },
            events,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_unit_below_sixty_percent_rests_and_one_above_does_not() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let u = spawn(&mut g, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        assert!(!needs_rest(&PlayerView::new(&g, P0), u));
        let (_, max) = PlayerView::new(&g, P0).unit_hits(u);
        g.scenario_set_hurt(u, max * 2 / 5 + 1);
        assert!(needs_rest(&PlayerView::new(&g, P0), u));
        g.scenario_set_hurt(u, max / 5);
        assert!(!needs_rest(&PlayerView::new(&g, P0), u));
        Ok(())
    }

    #[test]
    fn a_wounded_unit_in_the_field_walks_home_and_drops_its_raid() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let u = spawn(&mut g, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&g, P0).unit_hits(u);
        g.scenario_set_hurt(u, max * 3 / 4);
        let serial = PlayerView::new(&g, P0).serial_of(u);
        let mut ai = Ai::new(P0);
        ai.raiders.push(serial);
        run(&mut ai, &mut g, &mut Vec::new());
        assert!(ai.raiders.is_empty(), "the raid is dropped");
        assert!(
            PlayerView::new(&g, P0).unit_has_order(u),
            "it is on its way"
        );
        Ok(())
    }

    #[test]
    fn a_wounded_unit_in_a_city_stays_to_mend() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let u = spawn(&mut g, P0, city, 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&g, P0).unit_hits(u);
        g.scenario_set_hurt(u, max * 3 / 4);
        run(&mut Ai::new(P0), &mut g, &mut Vec::new());
        assert!(!PlayerView::new(&g, P0).unit_has_order(u));
        Ok(())
    }

    #[test]
    fn a_wounded_unit_is_not_sent_to_a_full_city_and_not_ordered_twice() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let city = at(COAST_CITY.0, COAST_CITY.1);
        let u = spawn(&mut g, P0, at(5, 5), 113 /* high_men_paladins */, 1)[0];
        let (_, max) = PlayerView::new(&g, P0).unit_hits(u);
        g.scenario_set_hurt(u, max * 3 / 4);
        // The only city has no room for one more.
        let room = PlayerView::new(&g, P0).max_tile();
        spawn(&mut g, P0, city, 104 /* high_men_spearmen */, room);
        run(&mut Ai::new(P0), &mut g, &mut Vec::new());
        assert!(
            !PlayerView::new(&g, P0).unit_has_order(u),
            "no room: it stays"
        );
        // With room again it is sent once, and a second turn leaves its order alone.
        for _ in 0..3 {
            let last = g.units().units_at(city).pop().ok_or("a unit in the city")?;
            g.cheat_remove_unit(last);
        }
        let mut events = Vec::new();
        let mut ai = Ai::new(P0);
        run(&mut ai, &mut g, &mut events);
        assert!(PlayerView::new(&g, P0).unit_has_order(u));
        let before = events.len();
        run(&mut ai, &mut g, &mut events);
        assert_eq!(events.len(), before, "the order is not given again");
        Ok(())
    }
}
