//! A square can hold only `max_tile` units, but a summoning, a hire, a finished unit or a freed
//! prisoner may add one to a square that is full. The computer wizards avoid it (the AI keeps
//! its cities below the limit and walks a unit out of the summoning circle first); the human
//! wizard is asked what to do: pick a unit on the square, then move it to a free neighbouring
//! square or disband it, until the square is back at the limit. The answers are the ordinary
//! `MoveStack` and `DismissUnit` commands; what the game adds here is the question, which squares
//! are over the limit and where a unit could go.
//!
//! Tests: in crowding/tests.rs, on the shipped content.

use std::collections::HashMap;

use crate::game::Game;
use crate::map::Position;
use crate::player::PlayerId;
use crate::unit::movement::Entry;

impl Game {
    /// Whether the units `player` gains on a full square are let in (and the square is over
    /// the limit until the player has answered): for the human wizard.
    pub fn crowds_allowed(&self, player: PlayerId) -> bool {
        self.human() == Some(player)
    }

    /// The squares that hold more of `player`'s units than the game allows, in map order.
    pub fn crowded_squares(&self, player: PlayerId) -> Vec<Position> {
        let mut count: HashMap<Position, usize> = HashMap::new();
        for u in self.units().player_units(player) {
            if let Some(p) = self.units().unit_position(u) {
                *count.entry(p).or_default() += 1;
            }
        }
        let mut full: Vec<Position> = count
            .into_iter()
            .filter(|&(_, n)| n > self.max_tile())
            .map(|(p, _)| p)
            .collect();
        full.sort_by_key(|p| (p.plane as u8, p.y, p.x));
        full
    }

    /// The squares next to the unit it could step to now to make room: it has moves left, the
    /// terrain lets it in, nothing hostile stands there and the square is not full itself.
    pub fn crowd_moves(&self, unit: hecs::Entity) -> Vec<Position> {
        let Some(from) = self.units().unit_position(unit) else {
            return Vec::new();
        };
        if self.units().unit_moves_left(unit) == 0 {
            return Vec::new();
        }
        crate::unit::pathing::neighbours(self, from)
            .into_iter()
            .filter(|&to| matches!(self.economy().plan_step(&[unit], to), Ok((_, Entry::Free))))
            .collect()
    }
}

#[cfg(test)]
mod tests;
