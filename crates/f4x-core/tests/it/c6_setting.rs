//! Class 6: where the outcome depends on a setting, run the action under each value of its range and
//! check the outcome follows. The range and the defaults are read from the settings, never copied.

use crate::support::worlds;
use f4x_core::city::FoundCityError;
use f4x_core::game::Command;
use f4x_core::map::{Plane, Position};

/// `min_city_distance`: a square that could hold a city under the loosest value holds one exactly
/// when it is at least the setting away from every city, and is "too close" otherwise.
#[test]
fn the_least_distance_between_cities_follows_its_setting() {
    for (name, mut g) in worlds().into_iter().step_by(2) {
        let cities: Vec<Position> = g.economy().cities().into_iter().map(|c| c.2).collect();
        assert!(!cities.is_empty(), "{name}: no city");
        let (width, height) = {
            let m = g.map().expect("a map");
            (m.width(), m.height())
        };
        let range = f4x_def::settings::MIN_CITY_DISTANCE_RANGE;
        // The squares, with their distance to the nearest city.
        let mut squares = Vec::new();
        for plane in [Plane::Arcanus, Plane::Myrror] {
            let grid = g.map().expect("a map").plane(plane);
            for y in 0..height {
                for x in 0..width {
                    let near = cities
                        .iter()
                        .filter(|c| c.plane == plane)
                        .map(|c| grid.chebyshev((c.x, c.y), (x, y)))
                        .min();
                    if let Some(d) = near {
                        squares.push((Position { x, y, plane }, d));
                    }
                }
            }
        }
        let mut tested = 0;
        for setting in range.clone() {
            g.scenario_change_settings(|s| s.rules.min_city_distance = setting);
            for &(pos, d) in &squares {
                let result = g.can_found_city(pos);
                match result {
                    Err(FoundCityError::TooCloseToCity) => {
                        // (A square with a city on it can also be too close to another: either
                        // reason is true.)
                        assert!(
                            d < u32::from(setting),
                            "{name}: {pos:?} at {d} is too close under {setting}"
                        );
                    }
                    Err(FoundCityError::CityAlreadyHere) => assert_eq!(d, 0, "{name}: {pos:?}"),
                    Ok(()) => {
                        tested += 1;
                        assert!(
                            d >= u32::from(setting),
                            "{name}: {pos:?} at {d} may hold a city under {setting}"
                        );
                    }
                    // Water, a node, a lair: the ground says no whatever the setting.
                    Err(_) => {}
                }
            }
        }
        assert!(tested > 100, "{name}: only {tested} squares were allowed");
        // And the setting does change something: the loosest value allows squares the strictest
        // does not.
        let count = |g: &mut f4x_core::game::Game, s: u8| {
            g.scenario_change_settings(|x| x.rules.min_city_distance = s);
            squares
                .iter()
                .filter(|(p, _)| g.can_found_city(*p).is_ok())
                .count()
        };
        let loose = count(&mut g, *range.start());
        let strict = count(&mut g, *range.end());
        assert!(loose > strict, "{name}: {loose} against {strict}");
    }
}

/// `max_tile`: a square takes units walking onto it up to the setting and refuses the next one with the
/// stack-full reason, under each value of the range.
#[test]
fn the_most_units_on_a_square_follows_its_setting() {
    use crate::support::around;
    use f4x_core::game::testutil::fresh;
    use f4x_core::game::{Event, Rejection};
    use f4x_core::rules::UnitId;
    use f4x_core::unit::movement::MoveError;
    let (wizards, seed) = crate::support::WORLDS[0];
    let base = fresh(wizards, seed).expect("a fresh game");
    let wizard = base
        .player_ids()
        .into_iter()
        .find(|&p| base.wizard(p).is_some())
        .expect("a wizard");
    let from = base
        .economy()
        .cities()
        .into_iter()
        .find(|c| c.1.owner == wizard)
        .expect("a city")
        .2;
    // A unit that walks, and a neighbouring square it can walk to: found by trying.
    let walker = |g: &mut f4x_core::game::Game, unit: UnitId, to| {
        g.apply(Command::CreateUnit {
            owner: wizard,
            pos: from,
            unit,
        });
        let u = *g.units().all_units().last().expect("the new unit");
        g.apply(Command::MoveStack { units: vec![u], to })
    };
    let moved = |events: &[Event]| events.iter().any(|e| matches!(e, Event::UnitsMoved { .. }));
    let mut found = None;
    'search: for def in base.content().rules.units.defs() {
        let unit = UnitId::new(def.num);
        for to in around(&base, from) {
            let mut probe = fresh(wizards, seed).expect("a fresh game");
            if moved(&walker(&mut probe, unit, to)) {
                found = Some((unit, to));
                break 'search;
            }
        }
    }
    let (unit, to) = found.expect("some unit walks to some neighbour of the city");
    let low = base.settings().rules.max_battle_side;
    for limit in low..=f4x_def::settings::MAX_TILE_LIMIT {
        let mut g = fresh(wizards, seed).expect("a fresh game");
        g.scenario_change_settings(|s| s.rules.max_tile = limit);
        let here = g.units().units_at(to).len();
        let mut accepted = 0;
        let mut refusal = None;
        for _ in 0..limit + 3 {
            let events = walker(&mut g, unit, to);
            match events.iter().find_map(|e| match e {
                Event::Rejected(r) => Some(*r),
                _ => None,
            }) {
                None => accepted += 1,
                Some(r) => {
                    refusal = Some(r);
                    break;
                }
            }
        }
        assert_eq!(accepted + here, limit, "limit {limit}");
        assert_eq!(
            refusal,
            Some(Rejection::Move(MoveError::StackFull)),
            "limit {limit}"
        );
    }
}

/// `max_city_size`: whatever the setting, a city that starts at its edge never grows past it over the
/// turns, and the world still holds together (the checker has the same rule).
#[test]
fn a_city_stops_growing_at_its_setting() {
    for setting in f4x_def::settings::MAX_CITY_SIZE_RANGE {
        let (_, mut g) = worlds().remove(0);
        g.scenario_change_settings(|s| s.rules.max_city_size = setting);
        let cities: Vec<_> = g.economy().cities().into_iter().map(|c| c.0).collect();
        for &c in &cities {
            g.scenario_set_citizens(c, u32::from(setting) - 1);
        }
        for _ in 0..15 {
            crate::support::busy_round(&mut g, 0);
            for &c in &cities {
                let size = g.economy().city_economy(c).map(|e| e.figures);
                assert!(
                    size.is_none_or(|n| n <= u32::from(setting)),
                    "setting {setting}: a city of {size:?}"
                );
            }
        }
        let bad = f4x_core::check::world_problems(&g);
        assert!(bad.is_empty(), "setting {setting}: {bad:#?}");
    }
}
