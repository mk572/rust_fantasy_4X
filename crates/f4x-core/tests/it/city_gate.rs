//! Earth Gate and Astral Gate on the shipped rules (`city/gate.rs`).

use f4x_core::city::gate::{ASTRAL_GATE, EARTH_GATE};
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::{Plane, Position};
use f4x_core::player::PlayerId;
use f4x_core::rules::exec::Target;
use f4x_core::testdir::TestResult;
use f4x_core::unit::movement::MoveError;

use crate::world::{at, flat_game, shipped, P0, P1};

fn world() -> Result<Game, &'static str> {
    flat_game(1, 12, 6, &[P0, P1])
}

/// A city of `owner`, with a gate cast by `caster` on it when one is named.
fn city(
    g: &mut Game,
    owner: PlayerId,
    pos: Position,
    gate: Option<(&str, PlayerId)>,
) -> TestResult {
    let city = g.scenario_found_city(owner, pos, "City", None, 3);
    if let Some((name, caster)) = gate {
        let spell = shipped()
            .rules
            .spells
            .id_of(name)
            .ok_or("the gate spell is not in the content")?;
        g.attach_enchantment(Target::City(city), spell, caster);
    }
    Ok(())
}

fn walker(g: &mut Game, owner: PlayerId, pos: Position) -> Result<hecs::Entity, &'static str> {
    let id = shipped()
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    Ok(g.spawn_unit(owner, pos, id))
}

#[test]
fn an_earth_gate_teleports_between_two_gate_cities_for_one_move() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    let b = at(Plane::Arcanus, 9, 4);
    city(&mut g, P0, a, Some((EARTH_GATE, P0)))?;
    city(&mut g, P0, b, Some((EARTH_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    let before = g.units().unit_moves_left(w);
    let events = g.apply(Command::EarthGate {
        units: vec![w],
        to: b,
    });
    assert!(
        matches!(events[..], [Event::UnitsMoved { .. }]),
        "{events:?}"
    );
    assert_eq!(g.units().unit_position(w), Some(b));
    assert_eq!(
        g.units().unit_moves_left(w),
        before - 2,
        "costs one move (two half-moves)"
    );
    Ok(())
}

#[test]
fn the_gate_needs_both_ends_the_same_owner_and_plane() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    let no_gate = at(Plane::Arcanus, 9, 4);
    let rival = at(Plane::Arcanus, 5, 4);
    let other_plane = at(Plane::Myrror, 9, 1);
    city(&mut g, P0, a, Some((EARTH_GATE, P0)))?;
    city(&mut g, P0, no_gate, None)?;
    city(&mut g, P1, rival, Some((EARTH_GATE, P1)))?;
    city(&mut g, P0, other_plane, Some((EARTH_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    for to in [no_gate, rival, other_plane, a, at(Plane::Arcanus, 3, 3)] {
        assert_eq!(
            g.can_use_earth_gate(&[w], to),
            Err(MoveError::NoGate),
            "{to:?}"
        );
    }
    // The stack must start in a gate city.
    let outside = walker(&mut g, P0, at(Plane::Arcanus, 3, 3))?;
    assert_eq!(
        g.can_use_earth_gate(&[outside], other_plane),
        Err(MoveError::NoGate)
    );
    Ok(())
}

#[test]
fn a_gate_cast_by_someone_else_does_nothing() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    let b = at(Plane::Arcanus, 9, 4);
    // The city changed hands: the old owner's enchantment is not the new owner's gate.
    city(&mut g, P0, a, Some((EARTH_GATE, P1)))?;
    city(&mut g, P0, b, Some((EARTH_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    assert_eq!(g.can_use_earth_gate(&[w], b), Err(MoveError::NoGate));
    Ok(())
}

#[test]
fn a_gate_needs_moves_and_room() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    let b = at(Plane::Arcanus, 9, 4);
    city(&mut g, P0, a, Some((EARTH_GATE, P0)))?;
    city(&mut g, P0, b, Some((EARTH_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    g.scenario_set_moves_left(w, 0);
    assert_eq!(g.can_use_earth_gate(&[w], b), Err(MoveError::NoMovesLeft));
    let w2 = walker(&mut g, P0, a)?;
    for _ in 0..g.max_tile() {
        walker(&mut g, P0, b)?;
    }
    assert_eq!(g.can_use_earth_gate(&[w2], b), Err(MoveError::StackFull));
    Ok(())
}

#[test]
fn an_astral_gate_changes_plane_unless_a_city_is_on_the_other_side() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    city(&mut g, P0, a, Some((ASTRAL_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    assert_eq!(g.shift_plane(&[w]), Ok(at(Plane::Myrror, 1, 1)));
    // Back through the same square: no gate on the Myrror side (no city), so no way.
    assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));

    let b = at(Plane::Arcanus, 6, 3);
    city(&mut g, P0, b, Some((ASTRAL_GATE, P0)))?;
    city(&mut g, P1, at(Plane::Myrror, 6, 3), None)?;
    let v = walker(&mut g, P0, b)?;
    assert_eq!(g.shift_plane(&[v]), Err(MoveError::CannotShiftPlane));
    Ok(())
}

#[test]
fn an_astral_gate_is_closed_by_the_planar_seal_and_by_stationed_enemies() -> TestResult {
    let mut g = world()?;
    let a = at(Plane::Arcanus, 1, 1);
    city(&mut g, P0, a, Some((ASTRAL_GATE, P0)))?;
    let w = walker(&mut g, P0, a)?;
    let guard = walker(&mut g, P1, at(Plane::Myrror, 1, 1))?;
    assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));
    g.scenario_remove_unit(guard);
    assert!(g.economy().can_shift_plane(&[w]).is_ok());
    let seal = shipped()
        .rules
        .spells
        .id_of("planar_seal")
        .ok_or("planar_seal is not in the content")?;
    g.attach_enchantment(Target::Player(P1), seal, P1);
    assert_eq!(
        g.economy().can_shift_plane(&[w]),
        Err(MoveError::CannotShiftPlane)
    );
    Ok(())
}
