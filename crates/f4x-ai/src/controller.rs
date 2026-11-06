//! The AI as controllers for `f4x-master`: a wizard's [`Ai`] plays its turn at once when its
//! seat comes, and the neutral seat plays the independent cities and the monsters.
//!
//! Tests: in tests/it.

use crate::{play_neutrals_for, Ai, Attack};
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Master, Pass, Seat, Status, Until};
use std::collections::HashMap;

/// The seat of one wizard's AI.
pub struct AiSeat<'a>(pub &'a mut Ai);

impl Controller for AiSeat<'_> {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], events: &mut Vec<Event>) -> Status {
        events.extend(self.0.play_turn(game));
        Status::Done
    }
}

/// The neutral seat: independent cities and roaming monsters play. Attacks on the players in
/// `defended_by` are not resolved but kept for the host (see [`NeutralSeat::take_attacks`]).
#[derive(Default)]
pub struct NeutralSeat {
    defended_by: Vec<PlayerId>,
    attacks: Vec<Attack>,
}

impl NeutralSeat {
    /// A neutral seat whose attacks on these players are left to them to fight.
    pub fn defended_by(players: &[PlayerId]) -> Self {
        Self {
            defended_by: players.to_vec(),
            attacks: Vec::new(),
        }
    }

    /// The attacks on defended players since the last call, oldest first.
    pub fn take_attacks(&mut self) -> Vec<Attack> {
        std::mem::take(&mut self.attacks)
    }
}

impl Controller for NeutralSeat {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], events: &mut Vec<Event>) -> Status {
        let (more, attacks) = play_neutrals_for(game, &self.defended_by);
        events.extend(more);
        self.attacks.extend(attacks);
        Status::Done
    }
}

/// Play the rest of the current round for these AIs and begin the next one. Every wizard has a
/// seat in the order of their ids: the AI's own, or a seat that passes for a player without one
/// (a human the caller does not drive); the neutral seat comes last. Returns all the events.
pub fn play_round(game: &mut Game, ais: &mut [Ai]) -> Vec<Event> {
    crate::battle::install();
    let mut by_player: HashMap<PlayerId, &mut Ai> =
        ais.iter_mut().map(|ai| (ai.player, ai)).collect();
    let mut seats: Vec<Seat> = game
        .player_ids()
        .into_iter()
        .filter(|&p| !game.is_neutral_player(p))
        .map(|p| match by_player.remove(&p) {
            Some(ai) => Seat::new(vec![p], Box::new(AiSeat(ai))),
            None => Seat::new(vec![p], Box::new(Pass)),
        })
        .collect();
    seats.push(Seat::neutrals(Box::new(NeutralSeat::default())));
    Master::new(seats).run(game, Until::Rounds(1)).events
}
