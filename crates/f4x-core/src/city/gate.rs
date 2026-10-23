//! City gates (task 12). Planar travel by tower and the Planar Seal are in `unit/movement.rs` and
//! `spell/global_enchantment.rs`.
//!
//! - **Earth Gate** (a city enchantment): a stack standing in a city with an Earth Gate may
//!   teleport to another city of the same wizard, on the same plane, that also has one, for one
//!   move (the original's `MainScr.c` checks the same plane, though its help says "any two
//!   cities").
//! - **Astral Gate**: a stack in a city with an Astral Gate may change plane there, as in a
//!   tower. The help adds that it does not work while the other plane holds a city on the same
//!   square; stationed enemies there block it like any other plane shift.
//!
//! Both belong to the wizard who cast them: a gate that outlived its caster's ownership of the
//! city does nothing for the city's new owner.

use crate::city::economy::Economy;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::unit::movement::MoveError;
use crate::unit::{MovesLeft, MAX_STACK};

/// The hook of the gate spells: the effect is read from the city's enchantments, so casting
/// does nothing beyond enchanting the city.
pub const CITY_GATE: &str = "city_gate";
pub const EARTH_GATE: &str = "earth_gate";
pub const ASTRAL_GATE: &str = "astral_gate";

/// One full move in half-moves.
const FULL_MOVE: u32 = 2;

impl Game {
    /// Whether the stack could teleport through Earth Gates to the city at `to`.
    pub fn can_use_earth_gate(
        &self,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<(), MoveError> {
        let (from, owner) = self.economy().check_stack(units)?;
        if self.economy().city_at(to).is_some_and(|c| {
            units
                .iter()
                .any(|&u| self.economy().ward_bars_creature(c, u))
        }) {
            return Err(MoveError::Warded);
        }
        if from == to
            || from.plane != to.plane
            || !self.economy().has_gate(from, owner, EARTH_GATE)
            || !self.economy().has_gate(to, owner, EARTH_GATE)
        {
            return Err(MoveError::NoGate);
        }
        if units.iter().any(|&u| self.units().unit_moves_left(u) == 0) {
            return Err(MoveError::NoMovesLeft);
        }
        if self.units().units_at(to).len() + units.len() > MAX_STACK {
            return Err(MoveError::StackFull);
        }
        Ok(())
    }

    /// Teleport the stack to the Earth Gate city at `to`, costing one move.
    pub fn use_earth_gate(
        &mut self,
        units: &[hecs::Entity],
        to: Position,
    ) -> Result<Vec<Event>, MoveError> {
        self.can_use_earth_gate(units, to)?;
        let (from, owner) = self.economy().check_stack(units)?;
        for &u in units {
            let left = self.units().unit_moves_left(u).saturating_sub(FULL_MOVE);
            let _ = self.world.insert_one(u, MovesLeft(left));
            let _ = self.world.insert_one(u, to);
        }
        self.explore(owner, to, crate::map::fog::sight_for(2));
        Ok(vec![Event::UnitsMoved {
            units: units.to_vec(),
            from,
            to,
        }])
    }
}

impl Economy<'_> {
    /// Whether `owner` keeps the named gate spell on the city standing at `pos`.
    pub fn has_gate(&self, pos: Position, owner: PlayerId, gate: &str) -> bool {
        let Some(city) = self.city_at(pos) else {
            return false;
        };
        let Some(spell) = self.content.rules.spells.id_of(gate) else {
            return false;
        };
        crate::spell::instance::Spells::new(self.world)
            .city_enchantments(city)
            .contains(&(spell, owner))
            && self.city_owner(city) == Some(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testutil::content_with_magic;
    use crate::map::{Grid, Map, Plane};
    use crate::rules::exec::Target;

    const P0: PlayerId = PlayerId(0);
    const P1: PlayerId = PlayerId(1);

    fn at(plane: Plane, x: u32, y: u32) -> Position {
        Position { plane, x, y }
    }

    fn world() -> Game {
        let c = content_with_magic();
        let grass = c.terrains.id_of("grassland").unwrap();
        let map = Map::new(Grid::new(12, 6, grass), Grid::new(12, 6, grass));
        let mut g = Game::new(1);
        g.content = c;
        g.map = Some(map);
        for id in [P0, P1] {
            g.add_player(id, "W");
        }
        g
    }

    fn city(g: &mut Game, owner: PlayerId, pos: Position, gate: Option<(&str, PlayerId)>) {
        let c = g.spawn_city(owner, pos, "C".into(), None, 3);
        if let Some((name, caster)) = gate {
            let spell = g.content.rules.spells.id_of(name).unwrap();
            g.attach_enchantment(Target::City(c), spell, caster);
        }
    }

    fn walker(g: &mut Game, owner: PlayerId, pos: Position) -> hecs::Entity {
        let id = g.content.rules.units.id_of("walker").unwrap();
        g.spawn_unit(owner, pos, id)
    }

    #[test]
    fn an_earth_gate_teleports_between_two_gate_cities_for_one_move() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        let b = at(Plane::Arcanus, 9, 4);
        city(&mut g, P0, a, Some((EARTH_GATE, P0)));
        city(&mut g, P0, b, Some((EARTH_GATE, P0)));
        let w = walker(&mut g, P0, a);
        let events = g.apply(crate::game::Command::EarthGate {
            units: vec![w],
            to: b,
        });
        assert!(
            matches!(events[..], [Event::UnitsMoved { .. }]),
            "{events:?}"
        );
        assert_eq!(g.units().unit_position(w), Some(b));
        assert_eq!(g.units().unit_moves_left(w), 2, "costs one move of two");
    }

    #[test]
    fn the_gate_needs_both_ends_the_same_owner_and_plane() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        let no_gate = at(Plane::Arcanus, 9, 4);
        let rival = at(Plane::Arcanus, 5, 4);
        let other_plane = at(Plane::Myrror, 9, 1);
        city(&mut g, P0, a, Some((EARTH_GATE, P0)));
        city(&mut g, P0, no_gate, None);
        city(&mut g, P1, rival, Some((EARTH_GATE, P1)));
        city(&mut g, P0, other_plane, Some((EARTH_GATE, P0)));
        let w = walker(&mut g, P0, a);
        for to in [no_gate, rival, other_plane, a, at(Plane::Arcanus, 3, 3)] {
            assert_eq!(
                g.can_use_earth_gate(&[w], to),
                Err(MoveError::NoGate),
                "{to:?}"
            );
        }
        // The stack must start in a gate city.
        let outside = walker(&mut g, P0, at(Plane::Arcanus, 3, 3));
        assert_eq!(
            g.can_use_earth_gate(&[outside], other_plane),
            Err(MoveError::NoGate)
        );
    }

    #[test]
    fn a_gate_cast_by_someone_else_does_nothing() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        let b = at(Plane::Arcanus, 9, 4);
        // The city changed hands: the old owner's enchantment is not the new owner's gate.
        city(&mut g, P0, a, Some((EARTH_GATE, P1)));
        city(&mut g, P0, b, Some((EARTH_GATE, P0)));
        let w = walker(&mut g, P0, a);
        assert_eq!(g.can_use_earth_gate(&[w], b), Err(MoveError::NoGate));
    }

    #[test]
    fn a_gate_needs_moves_and_room() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        let b = at(Plane::Arcanus, 9, 4);
        city(&mut g, P0, a, Some((EARTH_GATE, P0)));
        city(&mut g, P0, b, Some((EARTH_GATE, P0)));
        let w = walker(&mut g, P0, a);
        let _ = g.world.insert_one(w, crate::unit::MovesLeft(0));
        assert_eq!(g.can_use_earth_gate(&[w], b), Err(MoveError::NoMovesLeft));
        let w2 = walker(&mut g, P0, a);
        for _ in 0..MAX_STACK {
            walker(&mut g, P0, b);
        }
        assert_eq!(g.can_use_earth_gate(&[w2], b), Err(MoveError::StackFull));
    }

    #[test]
    fn an_astral_gate_changes_plane_unless_a_city_is_on_the_other_side() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        city(&mut g, P0, a, Some((ASTRAL_GATE, P0)));
        let w = walker(&mut g, P0, a);
        assert_eq!(g.shift_plane(&[w]), Ok(at(Plane::Myrror, 1, 1)));
        // Back through the same square: no gate on the Myrror side (no city), so no way.
        assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));

        let b = at(Plane::Arcanus, 6, 3);
        city(&mut g, P0, b, Some((ASTRAL_GATE, P0)));
        city(&mut g, P1, at(Plane::Myrror, 6, 3), None);
        let v = walker(&mut g, P0, b);
        assert_eq!(g.shift_plane(&[v]), Err(MoveError::CannotShiftPlane));
    }

    #[test]
    fn an_astral_gate_is_closed_by_the_planar_seal_and_by_stationed_enemies() {
        let mut g = world();
        let a = at(Plane::Arcanus, 1, 1);
        city(&mut g, P0, a, Some((ASTRAL_GATE, P0)));
        let w = walker(&mut g, P0, a);
        let guard = walker(&mut g, P1, at(Plane::Myrror, 1, 1));
        assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));
        g.despawn_with_spells(guard);
        assert!(g.economy().can_shift_plane(&[w]).is_ok());
        let seal = g.content.rules.spells.id_of("planar_seal").unwrap();
        g.attach_enchantment(Target::Player(P1), seal, P1);
        assert_eq!(
            g.economy().can_shift_plane(&[w]),
            Err(MoveError::CannotShiftPlane)
        );
    }
}
