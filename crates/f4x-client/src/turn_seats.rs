//! The client's seats for `f4x-master`: the human's seat waits until the player ends the turn,
//! and holds another player's turn while an attack or a proposal waits for the human's answer, or
//! the battle of an attack is fought; a computer player's turn is played at once.
//!
//! Tests: none of its own; `tests/` plays turns through the client's `end_turns`.

use f4x_ai::Ai;
use f4x_core::player::PlayerId;
use f4x_core::world::Event;
use f4x_master::Game;
use f4x_master::{Controller, Status};
use std::cell::RefCell;

/// What persists between the client's calls into the master, besides the master's own state.
#[derive(Default)]
pub(crate) struct Seating {
    /// The player pressed End Turn: the human seat finishes on its next call.
    pub human_done: bool,
    /// Nobody sits at the screen (`App::end_turns`): what waits for the human is left.
    pub absent: bool,
}

/// The human: waiting until the turn is ended.
pub(crate) struct HumanSeat<'a> {
    pub seating: &'a RefCell<Seating>,
}

impl Controller for HumanSeat<'_> {
    fn play(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        if std::mem::take(&mut self.seating.borrow_mut().human_done) {
            Status::Done
        } else {
            Status::Waiting
        }
    }

    /// The map asks the player (`App::resume_turn`, `App::queue_proposal`).
    fn react(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        if self.seating.borrow().absent {
            Status::Done
        } else {
            Status::Waiting
        }
    }
}

/// A rival wizard.
pub(crate) struct RivalSeat<'a> {
    pub ai: &'a mut Ai,
}

impl Controller for RivalSeat<'_> {
    fn play(&mut self, game: &mut Game, _: PlayerId, events: &mut Vec<Event>) -> Status {
        events.extend(self.ai.play_turn(game));
        Status::Done
    }

    fn react(&mut self, game: &mut Game, _: PlayerId, events: &mut Vec<Event>) -> Status {
        events.extend(self.ai.react(game));
        Status::Done
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        self.ai
            .to_text()
            .ok()
            .map(|t| (self.ai.player, t))
            .into_iter()
            .collect()
    }

    fn notice(&mut self, game: &Game, events: &[Event]) {
        self.ai.notice(game, events);
    }
}

/// The players without a wizard: the independent race players and the monsters.
pub(crate) struct NeutralTurns<'a>(pub &'a mut f4x_ai::NeutralAi);

impl Controller for NeutralTurns<'_> {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        self.0.play(game, player, events)
    }

    fn react(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        Controller::react(&mut *self.0, game, player, events)
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        self.0.plans()
    }

    fn notice(&mut self, game: &Game, events: &[Event]) {
        Controller::notice(&mut *self.0, game, events);
    }
}
