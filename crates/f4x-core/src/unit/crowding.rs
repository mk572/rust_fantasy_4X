//! A square can hold only `max_tile` units, but a summoning, a hire, a finished unit or a freed
//! prisoner may add one to a square that is full. The computer wizards avoid it (the AI keeps
//! its cities below the limit and walks a unit out of the summoning circle first); the human
//! wizard is asked what to do: pick a unit on the square, then move it to a free neighbouring
//! square or disband it, until the square is back at the limit. The answers are the ordinary
//! `MoveStack` and `DismissUnit` commands; what the game adds here is the question, which squares
//! are over the limit and where a unit could go.
//!
//! A unit that is about to appear on a full square (a finished unit, a summoning, hired
//! mercenaries) first asks for room: [`Game::make_room`] moves one of the owner's units on the
//! square to a free square next to it. Who picks the unit: the human answers the question above
//! (the square may go over the limit meanwhile); for a computer wizard the game asks the chooser
//! the AI crate installs ([`set_crowd_chooser`], as it does for the battle AI), and without one
//! the cheapest unit that can leave goes. So the question never goes unanswered.
//!
//! Tests: in crowding/tests.rs, on the shipped content.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::game::Game;
use crate::map::Position;
use crate::player::PlayerId;
use crate::unit::movement::Entry;

/// A unit that could step off a full square and the free squares next to it it could step to.
pub type Leaver = (hecs::Entity, Vec<Position>);

/// The scripted choice, for a computer wizard, of the unit to move off a full square and the
/// square it goes to: given the game, the owner, the square and the units that can leave it,
/// the pair to move. An answer that is not one of the offered pairs is ignored.
pub type CrowdChooser =
    fn(&Game, PlayerId, Position, &[Leaver]) -> Option<(hecs::Entity, Position)>;

static CROWD_CHOOSER: OnceLock<CrowdChooser> = OnceLock::new();

/// Name the scripted chooser: `f4x_ai::install` calls this once. Which unit makes room is a
/// decision, so it lives in the AI crate; the game only asks.
pub fn set_crowd_chooser(chooser: CrowdChooser) {
    let _ = CROWD_CHOOSER.set(chooser);
}

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

    /// The units of `player` on `at` that could step off it now, each with the squares it could
    /// step to (see [`Game::crowd_moves`]).
    fn leavers(&self, at: Position, player: PlayerId) -> Vec<Leaver> {
        self.units()
            .units_at(at)
            .into_iter()
            .filter(|&u| self.units().unit_owner(u) == Some(player))
            .map(|u| (u, self.crowd_moves(u)))
            .filter(|(_, to)| !to.is_empty())
            .collect()
    }

    /// Whether `needed` more units of `player` can stand on `at` now: they fit, the square may
    /// go over the limit (the human is asked), or enough units of the player can step off it.
    pub fn can_make_room(&self, at: Position, player: PlayerId, needed: usize) -> bool {
        let held = self.units().units_at(at).len();
        held + needed <= self.max_tile()
            || self.crowds_allowed(player)
            || held + needed - self.max_tile() <= self.leavers(at, player).len()
    }

    /// Make room on `at` for `needed` units of `player`: while it is too full, one of the
    /// player's units on it steps to a free square next to it. The player's chooser picks the
    /// unit and the square ([`set_crowd_chooser`]); without one, or when its answer is not one
    /// of the offered pairs, the cheapest unit steps to the first free square. Does nothing for
    /// the human, who is asked instead. Returns whether there is room now.
    pub fn make_room(&mut self, at: Position, player: PlayerId, needed: usize) -> bool {
        if self.crowds_allowed(player) {
            return true;
        }
        let mut stuck: Vec<hecs::Entity> = Vec::new();
        loop {
            if self.units().units_at(at).len() + needed <= self.max_tile() {
                return true;
            }
            let mut leavers = self.leavers(at, player);
            leavers.retain(|(u, _)| !stuck.contains(u));
            let chosen = CROWD_CHOOSER
                .get()
                .and_then(|choose| choose(self, player, at, &leavers))
                .filter(|(u, to)| leavers.iter().any(|(l, free)| l == u && free.contains(to)))
                .or_else(|| self.cheapest(&leavers));
            let Some((unit, to)) = chosen else {
                return false;
            };
            if self.move_stack(&[unit], to).is_err() {
                stuck.push(unit);
            }
        }
    }

    /// The cheapest of these units to build (the newest first among equals) and the first square
    /// it could step to.
    fn cheapest(&self, leavers: &[Leaver]) -> Option<(hecs::Entity, Position)> {
        let (unit, to) = leavers.iter().min_by_key(|(u, _)| {
            let cost = self.units().unit_def(*u).map_or(0, |d| d.cost);
            (cost, std::cmp::Reverse(self.units().serial_of(*u)))
        })?;
        Some((*unit, *to.first()?))
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
