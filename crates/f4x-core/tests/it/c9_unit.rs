//! Class 9, the unit: what a player sees when a unit walks, flies, swims or sails, runs out of moves, is
//! refilled at the turn and is hurt and healed. The costs are worked out here from the content (the
//! terrain table, the movement type of the unit) and compared with what the game charges.

use crate::support::{around, worlds};
use f4x_core::map::terrain::TerrainClass;
use f4x_core::map::Position;
use f4x_core::rules::unit_flag::UnitFlag;
use f4x_core::rules::UnitId;
use f4x_core::unit::movement::MoveError;
use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, Event, Rejection, World};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::MovementType;

fn refusal(events: &[Event]) -> Option<Rejection> {
    events.iter().find_map(|e| match e {
        Event::Rejected(r) => Some(*r),
        _ => None,
    })
}

/// Whether a unit's movement depends on anything but its movement type and the ground: the abilities that
/// change a step (flying, water walking, forest and mountain walking, pathfinding, no body).
fn plain(w: &World, unit: hecs::Entity) -> bool {
    // A flier flies by its movement type; the flag only repeats it.
    let flies = w
        .units()
        .unit_def(unit)
        .is_some_and(|d| d.movement == MovementType::Flying);
    let changes_a_step = |f: UnitFlag| {
        matches!(
            f,
            UnitFlag::WaterWalking
                | UnitFlag::Pathfinding
                | UnitFlag::Forester
                | UnitFlag::Mountaineer
                | UnitFlag::NonCorporeal
                | UnitFlag::WindWalking
        ) || (f == UnitFlag::Flies && !flies)
    };
    ![
        UnitFlag::Flies,
        UnitFlag::WaterWalking,
        UnitFlag::Pathfinding,
        UnitFlag::Forester,
        UnitFlag::Mountaineer,
        UnitFlag::NonCorporeal,
        UnitFlag::WindWalking,
    ]
    .iter()
    .any(|&f| changes_a_step(f) && w.units().unit_has_flag(unit, f))
}

/// What a step onto `to` costs a plain unit of `movement`, from the content; `Err` when it may not enter.
fn cost(
    w: &World,
    movement: MovementType,
    from: Position,
    to: Position,
) -> Result<HalfInt, MoveError> {
    let map = w.map().expect("a map");
    let terrain = w
        .content()
        .terrains
        .get(
            map.plane(to.plane)
                .get(to.x as i32, to.y as i32)
                .expect("a square"),
        )
        .expect("a terrain");
    let water = terrain.class == TerrainClass::Ocean;
    match movement {
        MovementType::Flying => return Ok(HalfInt::from_int(1)),
        MovementType::Swimming if water => return Ok(HalfInt::from_int(1)),
        // A ship: open to water, and to land only in a city of its owner (the squares tried have none).
        MovementType::Sailing if water => return Ok(HalfInt::from_int(1)),
        MovementType::Sailing => return Err(MoveError::Impassable),
        _ => {}
    }
    if water || !terrain.walkable {
        return Err(MoveError::Impassable);
    }
    let both_roads = map.plane(from.plane).has_road(from.x as i32, from.y as i32)
        && map.plane(to.plane).has_road(to.x as i32, to.y as i32);
    if both_roads {
        // A road halves the step; on Myrror every road is enchanted and the step is free.
        return Ok(if to.plane == f4x_core::map::Plane::Myrror {
            HalfInt::ZERO
        } else {
            HalfInt::HALF
        });
    }
    Ok(HalfInt::from_int(i32::from(terrain.move_cost).max(1)))
}

/// A new plain unit of the movement type at `at`, the first the content has.
fn make_plain(
    w: &mut Table,
    owner: f4x_core::player::PlayerId,
    at: Position,
    movement: MovementType,
) -> Option<hecs::Entity> {
    let defs: Vec<UnitId> = w
        .content()
        .rules
        .units
        .defs()
        .iter()
        .filter(|d| d.movement == movement)
        .map(|d| UnitId::new(d.num))
        .collect();
    for unit in defs {
        w.apply(Command::CreateUnit {
            owner,
            pos: at,
            unit,
        });
        let u = *w.units().all_units().last().expect("a unit");
        if plain(w, u) && w.units().unit_moves_left(u) > HalfInt::ZERO {
            return Some(u);
        }
        w.scenario_remove_unit(u);
    }
    None
}

/// An empty square next to `from` that a move can be tried on: no army, no lair, no city, no other feature.
fn open_neighbours(w: &World, from: Position) -> Vec<Position> {
    around(w, from)
        .into_iter()
        .filter(|&to| {
            w.units().units_at(to).is_empty()
                && w.economy().city_at(to).is_none()
                && w.economy().feature_entity_at(to).is_none()
        })
        .collect()
}

#[test]
fn a_step_costs_what_the_terrain_and_the_movement_of_the_unit_say_or_is_refused() {
    let mut tried = 0;
    let mut refused = 0;
    let mut kinds = std::collections::BTreeSet::new();
    for (name, mut w) in worlds() {
        assert!(
            !w.settings().basic.diagonal_extra_cost,
            "the matrix assumes the plain cost"
        );
        let wizard = w
            .player_ids()
            .into_iter()
            .find(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
            .expect("a wizard");
        let from = w.economy().cities_of(wizard)[0].1;
        let defs: Vec<_> = w
            .content()
            .rules
            .units
            .defs()
            .iter()
            .map(|d| (UnitId::new(d.num), d.movement))
            .collect();
        for (unit, movement) in defs {
            for to in open_neighbours(&w, from) {
                w.apply(Command::CreateUnit {
                    owner: wizard,
                    pos: from,
                    unit,
                });
                let u = *w.units().all_units().last().expect("the new unit");
                if !plain(&w, u) || w.units().unit_moves_left(u) == HalfInt::ZERO {
                    w.scenario_remove_unit(u);
                    continue;
                }
                let left = w.units().unit_moves_left(u);
                let events = w.apply(Command::MoveStack { units: vec![u], to });
                tried += 1;
                match (cost(&w, movement, from, to), refusal(&events)) {
                    (Ok(c), None) => {
                        assert_eq!(
                            w.units().unit_position(u),
                            Some(to),
                            "{name}: {unit:?} to {to:?}"
                        );
                        assert_eq!(
                            w.units().unit_moves_left(u),
                            left.saturating_sub(c),
                            "{name}: {unit:?} ({movement:?}) to {to:?}: the cost"
                        );
                        kinds.insert(format!("{movement:?}"));
                    }
                    (Err(why), Some(Rejection::Move(got))) => {
                        refused += 1;
                        assert_eq!(got, why, "{name}: {unit:?} to {to:?}");
                        assert_eq!(w.units().unit_position(u), Some(from));
                        assert_eq!(w.units().unit_moves_left(u), left);
                    }
                    (want, got) => panic!(
                        "{name}: {unit:?} ({movement:?}) to {to:?}: wanted {want:?}, got {got:?}"
                    ),
                }
                w.scenario_remove_unit(u);
            }
        }
    }
    assert!(
        tried > 500 && refused > 100,
        "{tried} moves, {refused} refused"
    );
    for kind in ["Walking", "Flying", "Swimming"] {
        assert!(kinds.contains(kind), "{kind} never moved: {kinds:?}");
    }
}

#[test]
fn a_unit_without_moves_cannot_step_and_a_new_turn_gives_them_back() {
    for (name, mut w) in worlds().into_iter().step_by(2) {
        let wizard = w
            .player_ids()
            .into_iter()
            .find(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
            .expect("a wizard");
        let units = w.units().player_units(wizard);
        let u = *units.first().expect("a unit");
        let at = w.units().unit_position(u).expect("on the map");
        let max = w.units().unit_max_moves(u);
        assert!(max > HalfInt::ZERO, "{name}: a unit with no moves at all");
        w.scenario_set_moves_left(u, HalfInt::ZERO);
        let to = around(&w, at).into_iter().next().expect("a neighbour");
        assert_eq!(
            refusal(&w.apply(Command::MoveStack { units: vec![u], to })),
            Some(Rejection::Move(MoveError::NoMovesLeft)),
            "{name}"
        );
        assert_eq!(w.units().unit_position(u), Some(at));
        w.start_player_turn(wizard, 2);
        assert_eq!(
            w.units().unit_moves_left(u),
            max,
            "{name}: the moves were not given back"
        );
    }
}

#[test]
fn a_stack_moves_together_and_each_unit_pays_its_own_cost() {
    let mut mixed = 0;
    for (name, mut w) in worlds() {
        let wizard = w
            .player_ids()
            .into_iter()
            .find(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
            .expect("a wizard");
        let from = w.economy().cities_of(wizard)[0].1;
        // A walker and a flier together, onto a square that costs the walker more than one move or less.
        for to in open_neighbours(&w, from) {
            let Ok(walk) = cost(&w, MovementType::Walking, from, to) else {
                continue;
            };
            let (Some(a), Some(b)) = (
                make_plain(&mut w, wizard, from, MovementType::Walking),
                make_plain(&mut w, wizard, from, MovementType::Flying),
            ) else {
                continue;
            };
            let (la, lb) = (w.units().unit_moves_left(a), w.units().unit_moves_left(b));
            let events = w.apply(Command::MoveStack {
                units: vec![a, b],
                to,
            });
            assert_eq!(refusal(&events), None, "{name}");
            assert_eq!(w.units().unit_position(a), Some(to));
            assert_eq!(w.units().unit_position(b), Some(to));
            assert_eq!(
                w.units().unit_moves_left(a),
                la.saturating_sub(walk),
                "{name}: the walker"
            );
            assert_eq!(
                w.units().unit_moves_left(b),
                lb.saturating_sub(HalfInt::from_int(1)),
                "{name}: the flier"
            );
            mixed += 1;
            break;
        }
    }
    assert!(mixed >= 2, "{mixed} mixed stacks were moved");
}

/// The moves a new walking unit spends on a move from `from` to `to`; `None` when it does not get there.
fn spent(
    w: &mut Table,
    owner: f4x_core::player::PlayerId,
    from: Position,
    to: Position,
) -> Option<HalfInt> {
    let u = make_plain(w, owner, from, MovementType::Walking)?;
    let left = w.units().unit_moves_left(u);
    w.apply(Command::MoveStack { units: vec![u], to });
    let got = (w.units().unit_position(u) == Some(to))
        .then(|| left.saturating_sub(w.units().unit_moves_left(u)));
    w.scenario_remove_unit(u);
    got
}

#[test]
fn a_step_between_two_river_squares_costs_half_a_move_with_the_river_roads_setting() {
    let mut checked = 0;
    for (name, mut w) in worlds() {
        let wizard = w
            .player_ids()
            .into_iter()
            .find(|&p| !w.is_neutral_player(p) && w.wizard(p).is_some())
            .expect("a wizard");
        let city = w.economy().cities_of(wizard)[0].1;
        for first in open_neighbours(&w, city) {
            // Three squares in a line: the city's neighbour and the one beyond it.
            let beyond = Position {
                x: (2 * first.x + w.map().expect("a map").width() - city.x)
                    % w.map().expect("a map").width(),
                y: (2 * first.y).wrapping_sub(city.y),
                plane: city.plane,
            };
            if beyond.y >= w.map().expect("a map").height()
                || !open_neighbours(&w, first).contains(&beyond)
            {
                continue;
            }
            let (a, b) = (first, beyond);
            for p in [city, a, b] {
                w.scenario_set_road(p, false);
            }
            // Only plain land walks: a river square with the road gone costs its terrain.
            let Some(off) = spent(&mut w, wizard, a, b) else {
                continue;
            };
            w.cheat_set_river(a, true);
            w.cheat_set_river(b, true);
            let still_off = spent(&mut w, wizard, a, b).expect("a move");
            assert_eq!(still_off, off, "{name}: a river changes nothing by itself");
            w.scenario_change_settings(|s| s.rules.river_roads = true);
            let on = spent(&mut w, wizard, a, b).expect("a move");
            assert!(
                on == HalfInt::HALF.min(off),
                "{name}: {a:?} to {b:?} along a river: {on:?} (off: {off:?})"
            );
            // One end off the river: the plain cost again.
            w.cheat_set_river(b, false);
            assert_eq!(
                spent(&mut w, wizard, a, b),
                Some(off),
                "{name}: one bank only"
            );
            checked += 1;
            break;
        }
    }
    assert!(
        checked >= 2,
        "only {checked} worlds had a line of open land"
    );
}
