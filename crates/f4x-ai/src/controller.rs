//! The AI as controllers for `f4x-master`: a wizard's [`Ai`] plays its turn at once when its
//! seat comes, and the neutral seat plays the neutral player of each race and the monsters.
//!
//! Tests: in tests/it.

use crate::{Ai, Attack};
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Master, Pass, Seat, Status, Until};
use std::collections::{BTreeMap, HashMap};

/// The seat of one wizard's AI.
pub struct AiSeat<'a>(pub &'a mut Ai);

impl Controller for AiSeat<'_> {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], events: &mut Vec<Event>) -> Status {
        events.extend(self.0.play_turn(game));
        Status::Done
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        self.0
            .to_text()
            .ok()
            .map(|t| (self.0.player, t))
            .into_iter()
            .collect()
    }
}

/// The neutral seat (task 522): the neutral player of each race plays as any AI does, with the
/// same [`Ai`] and the same turn, and the roaming monsters play after them. The seat keeps one
/// [`Ai`] for each race player, made when the player first plays (with the plans a saved game
/// carries for it) and dropped when the player is gone. Attacks on the players in `defended_by`
/// are not resolved but kept for the host (see [`NeutralSeat::take_attacks`]).
#[derive(Default)]
pub struct NeutralSeat {
    defended_by: Vec<PlayerId>,
    attacks: Vec<Attack>,
    ais: BTreeMap<PlayerId, Ai>,
    record_refusals: bool,
}

impl NeutralSeat {
    /// A neutral seat whose attacks on these players are left to them to fight.
    pub fn defended_by(players: &[PlayerId]) -> Self {
        Self {
            defended_by: players.to_vec(),
            ..Self::default()
        }
    }

    /// The attacks on defended players since the last call, oldest first.
    pub fn take_attacks(&mut self) -> Vec<Attack> {
        std::mem::take(&mut self.attacks)
    }

    /// Keep every command the game refuses, for [`NeutralSeat::take_refusals`].
    pub fn record_refusals(&mut self, on: bool) {
        self.record_refusals = on;
        for ai in self.ais.values_mut() {
            ai.record_refusals(on);
        }
    }

    /// The refused commands of the race players since the last call.
    pub fn take_refusals(&self) -> Vec<crate::Refusal> {
        self.ais.values().flat_map(Ai::take_refusals).collect()
    }

    /// What the race players have in mind, as text to keep in a save, with their players.
    pub fn plans(&self) -> Vec<(PlayerId, String)> {
        self.ais
            .iter()
            .filter_map(|(&p, ai)| Some((p, ai.to_text().ok()?)))
            .collect()
    }

    /// The AI of a race player, made now if it has none.
    fn ai_of(&mut self, game: &Game, id: PlayerId) -> &mut Ai {
        let (defended_by, record) = (self.defended_by.clone(), self.record_refusals);
        self.ais.entry(id).or_insert_with(|| {
            let mut ai = Ai::new(id);
            ai.leave_defence_to(&defended_by);
            Ai::recall(game, std::slice::from_mut(&mut ai));
            ai.record_refusals(record);
            ai
        })
    }

    /// Play the turn of every race player in `players`, then the monsters'.
    pub fn play_turn(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) {
        crate::battle::install();
        self.ais.retain(|&id, _| game.race_of_player(id).is_some());
        for &id in players {
            if game.race_of_player(id).is_none() || !game.has_player(id) {
                continue;
            }
            let ai = self.ai_of(game, id);
            events.extend(ai.play_turn(game));
            let attacks = ai.take_attacks();
            self.attacks.extend(attacks);
        }
        let (more, attacks) = crate::monster::roam(game, &self.defended_by);
        events.extend(more);
        self.attacks.extend(attacks);
    }
}

impl Controller for NeutralSeat {
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        self.play_turn(game, players, events);
        Status::Done
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        NeutralSeat::plans(self)
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
