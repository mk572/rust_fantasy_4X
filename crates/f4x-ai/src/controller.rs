//! The AI as controllers for `f4x-master`: a wizard's [`Ai`] plays its turn at once when it
//! comes ([`AiSeat`]), and so does [`NeutralAi`] in the turn of each player without a wizard.
//!
//! Tests: in tests/it.

use crate::{Ai, Attack, Diplomats};
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::Event;
use f4x_master::Game;
use f4x_master::{Controller, Master, Pass, Plans, Seat, Status, Until};
use std::collections::{BTreeMap, HashMap};

/// The seat of one wizard's AI.
pub struct AiSeat<'a>(pub &'a mut Ai);

impl Controller for AiSeat<'_> {
    fn play(&mut self, game: &mut Game, _: PlayerId, events: &mut Vec<Event>) -> Status {
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

    fn notice(&mut self, game: &Game, events: &[Event]) {
        self.0.notice(game, events);
    }
}

/// The computer play of every player without a wizard, each in a turn of its own: the neutral
/// player of each race plays as any AI does (task 522), with the same [`Ai`] and the same turn,
/// and the roaming monsters in theirs. It keeps one [`Ai`] for each race player, made when the
/// player first plays or when a loaded game carries plans for it. Attacks on the players in `defended_by` are not resolved but kept for the host (see
/// [`NeutralAi::take_attacks`]).
#[derive(Default)]
pub struct NeutralAi {
    defended_by: Vec<PlayerId>,
    attacks: Vec<Attack>,
    ais: BTreeMap<PlayerId, Ai>,
    record_refusals: bool,
    /// The game's table of memories, which the race players join when they are made.
    diplomats: Diplomats,
}

impl NeutralAi {
    /// Computer play whose attacks on these players are left to them to fight.
    pub fn defended_by(players: &[PlayerId]) -> Self {
        Self {
            defended_by: players.to_vec(),
            ..Self::default()
        }
    }

    /// The race players join this table of memories (`Ai::join`) when they are made.
    pub fn share(&mut self, diplomats: &Diplomats) {
        self.diplomats = diplomats.clone();
        for ai in self.ais.values_mut() {
            ai.join(diplomats);
        }
    }

    /// The attacks on defended players since the last call, oldest first.
    pub fn take_attacks(&mut self) -> Vec<Attack> {
        std::mem::take(&mut self.attacks)
    }

    /// Keep every command the game refuses, for [`NeutralAi::take_refusals`].
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

    /// Give the race players of `game` the plans it was saved with (see [`Ai::recall`]). A race
    /// player with plans gets its AI now, so that it hears what happens before its turn and its
    /// plans are saved again with the game.
    pub fn recall(&mut self, game: &Game, plans: &[(PlayerId, String)]) {
        for &(id, _) in plans {
            if game.race_of_player(id).is_some() {
                self.ai_of(id);
            }
        }
        for ai in self.ais.values_mut() {
            Ai::recall(plans, std::slice::from_mut(ai));
        }
    }

    /// What the race players have in mind, as text to keep in a save, with their players.
    pub fn plans(&self) -> Plans {
        self.ais
            .iter()
            .filter_map(|(&p, ai)| Some((p, ai.to_text().ok()?)))
            .collect()
    }

    /// What the neutral players have to say to the human at the start of the human's turn
    /// (`Ai::approach_human`): offers, warnings of war, requests.
    pub fn approach_human(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<PlayerId> = self.ais.keys().copied().collect();
        for id in ids {
            if game.race_of_player(id).is_none() {
                continue;
            }
            if let Some(ai) = self.ais.get_mut(&id) {
                events.extend(ai.approach_human(game));
            }
        }
        events
    }

    /// The AI of a race player, made now if it has none.
    fn ai_of(&mut self, id: PlayerId) -> &mut Ai {
        let (defended_by, record) = (self.defended_by.clone(), self.record_refusals);
        let diplomats = self.diplomats.clone();
        self.ais.entry(id).or_insert_with(|| {
            let mut ai = Ai::new(id);
            ai.join(&diplomats);
            ai.leave_defence_to(&defended_by);
            ai.record_refusals(record);
            ai
        })
    }

    /// Play the turn of `player`: a race player's, or the monsters'.
    pub fn play_turn(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) {
        crate::battle::install();
        if player == MONSTERS {
            let (more, attacks) = crate::monster::roam(game, &self.defended_by);
            events.extend(more);
            self.attacks.extend(attacks);
        } else if game.race_of_player(player).is_some() {
            let ai = self.ai_of(player);
            events.extend(ai.play_turn(game));
            let attacks = ai.take_attacks();
            self.attacks.extend(attacks);
        }
    }
}

impl Controller for NeutralAi {
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status {
        self.play_turn(game, player, events);
        Status::Done
    }

    fn plans(&self) -> Vec<(PlayerId, String)> {
        NeutralAi::plans(self)
    }

    fn notice(&mut self, game: &Game, events: &[Event]) {
        for ai in self.ais.values_mut() {
            ai.notice(game, events);
        }
    }
}

/// Play the rest of the current round for these AIs and begin the next one. Every wizard has a
/// seat: the AI's own, or one that passes for a wizard without one (a human the caller does not
/// drive); [`NeutralAi`] plays the other players. Returns all the events.
pub fn play_round(game: &mut Game, ais: &mut [Ai]) -> Vec<Event> {
    crate::battle::install();
    let diplomats = Diplomats::new();
    for ai in ais.iter_mut() {
        ai.join(&diplomats);
    }
    let mut by_player: HashMap<PlayerId, &mut Ai> =
        ais.iter_mut().map(|ai| (ai.player, ai)).collect();
    let mut seats: Vec<Seat> = game
        .player_ids()
        .into_iter()
        .filter(|&p| !game.is_neutral_player(p))
        .map(|p| match by_player.remove(&p) {
            Some(ai) => Seat::new(p, Box::new(AiSeat(ai))),
            None => Seat::new(p, Box::new(Pass)),
        })
        .collect();
    let mut neutrals = NeutralAi::default();
    neutrals.share(&diplomats);
    seats.push(Seat::non_wizards(Box::new(neutrals)));
    Master::new(seats).run(game, Until::Rounds(1)).events
}
