//! City gates. Planar travel by tower and the Planar Seal are in `unit/movement.rs` and
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
//!
//! Tests: in tests/it/city_gate.rs, on the shipped content.

use crate::city::economy::Economy;
use crate::game::{Event, Game};
use crate::map::Position;
use crate::player::PlayerId;
use crate::unit::movement::MoveError;
use crate::unit::MovesLeft;

/// The hook of the gate spells: the effect is read from the city's enchantments, so casting
/// does nothing beyond enchanting the city. A spell carries the hook in `spells.ron`, with the
/// kind of gate as its argument.
pub const GATE: &str = "gate";
pub const EARTH_GATE: &str = "earth";
pub const ASTRAL_GATE: &str = "astral";

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
        if self.units().units_at(to).len() + units.len() > self.max_tile() {
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
    /// Whether `owner` keeps a gate spell of the given kind (`EARTH_GATE` or `ASTRAL_GATE`, the
    /// argument of the `gate` hook) on the city standing at `pos`.
    pub fn has_gate(&self, pos: Position, owner: PlayerId, gate: &str) -> bool {
        let Some(city) = self.city_at(pos) else {
            return false;
        };
        crate::spell::instance::Spells::new(self.world)
            .city_enchantments(city)
            .iter()
            .any(|&(spell, o)| {
                o == owner
                    && self
                        .content
                        .rules
                        .spells
                        .get(spell)
                        .hook_args(GATE)
                        .is_some_and(|a| a == [gate])
            })
            && self.city_owner(city) == Some(owner)
    }
}
