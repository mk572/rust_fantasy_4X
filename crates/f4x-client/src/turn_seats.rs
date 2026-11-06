//! The client's seats for `f4x-master`: the human's seat waits until the player ends the turn;
//! a rival's seat plays at once and then waits while an attack on the human is fought, so the
//! next rival plays on the board the battle left.
//!
//! Tests: none of its own; `tests/` plays turns through the client's `end_turns`.

use crate::attack_prompt::PendingAttack;
use f4x_ai::{Ai, Attack};
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Status};
use std::cell::RefCell;
use std::collections::VecDeque;

/// What persists between the client's calls into the master, besides the master's own state.
#[derive(Default)]
pub(crate) struct Seating {
    /// The player pressed End Turn: the human seat finishes on its next call.
    pub human_done: bool,
    /// The rival (or neutral) seat that is acting has played and now waits for the defences.
    pub played: bool,
}

/// The human: waiting until the turn is ended.
pub(crate) struct HumanSeat<'a> {
    pub seating: &'a RefCell<Seating>,
}

impl Controller for HumanSeat<'_> {
    fn play(&mut self, _: &mut Game, _: &[PlayerId], _: &mut Vec<Event>) -> Status {
        if std::mem::take(&mut self.seating.borrow_mut().human_done) {
            Status::Done
        } else {
            Status::Waiting
        }
    }
}

fn queue(defences: &RefCell<VecDeque<PendingAttack>>, attacks: Vec<Attack>) {
    for attack in attacks {
        defences.borrow_mut().push_back(PendingAttack {
            attackers: attack.attackers,
            defenders: attack.defenders,
            defending: true,
            treaty_asked: true,
        });
    }
}

/// Whether the seat is done: it has played and no attack on the human waits any more.
fn finished(seating: &RefCell<Seating>, defences: &RefCell<VecDeque<PendingAttack>>) -> Status {
    if defences.borrow().is_empty() {
        seating.borrow_mut().played = false;
        Status::Done
    } else {
        Status::Waiting
    }
}

/// A rival wizard.
pub(crate) struct RivalSeat<'a> {
    pub ai: &'a mut Ai,
    pub seating: &'a RefCell<Seating>,
    pub defences: &'a RefCell<VecDeque<PendingAttack>>,
}

impl Controller for RivalSeat<'_> {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], events: &mut Vec<Event>) -> Status {
        if !self.seating.borrow().played {
            events.extend(self.ai.play_turn(game));
            queue(self.defences, self.ai.take_attacks());
            self.seating.borrow_mut().played = true;
        }
        finished(self.seating, self.defences)
    }
}

/// The independent cities and the monsters.
pub(crate) struct NeutralSeat<'a> {
    pub inner: &'a mut f4x_ai::NeutralSeat,
    pub seating: &'a RefCell<Seating>,
    pub defences: &'a RefCell<VecDeque<PendingAttack>>,
}

impl Controller for NeutralSeat<'_> {
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        if !self.seating.borrow().played {
            self.inner.play(game, players, events);
            queue(self.defences, self.inner.take_attacks());
            self.seating.borrow_mut().played = true;
        }
        finished(self.seating, self.defences)
    }
}
