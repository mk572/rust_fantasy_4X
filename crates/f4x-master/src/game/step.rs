//! The turn order: the steps that advance the game, and who takes part in them.
//!
//! A step is not a command: no player sends it. Each is written down when the game is recording,
//! ends the undo history, and puts the master's saved place in the round out of date. What a step
//! does to the world is the world's (`f4x-core`, `turn.rs`).
//!
//! Tests: in tests/it.

use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::{Event, Rejection, World};
use serde::{Deserialize, Serialize};

use super::end::{judge, GameOver};
use super::Game;
use crate::Phase;

/// One step of the turn order, or the whole change of rounds at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// The round begins.
    StartRound,
    /// The turn of a player begins.
    StartTurn(PlayerId),
    /// The turn of a player ends.
    EndTurn(PlayerId),
    /// The round ends.
    EndRound,
    /// [`Game::play_round`].
    PlayRound,
}

/// Every player of `world` that takes part in the turn order, in the fixed order: the wizards
/// and the independent cities' players by id, then the monsters' owner last.
pub fn turn_players(world: &World) -> Vec<PlayerId> {
    let mut players = world.player_ids();
    if world.player(MONSTERS).is_some() {
        players.push(MONSTERS);
    }
    players
}

impl Game {
    /// The round the game is in, the first being 1.
    pub fn round(&self) -> u32 {
        self.state.round
    }

    /// Where the master was in the round, as it last wrote it: `None` when nothing has, or a step
    /// has been made since.
    pub fn phase(&self) -> Option<Phase> {
        self.state.phase
    }

    /// Record where the master is in the round, to be saved with the game.
    pub(crate) fn set_phase(&mut self, phase: Phase) {
        self.state.phase = Some(phase);
    }

    /// [`turn_players`] of the game's world.
    pub fn turn_players(&self) -> Vec<PlayerId> {
        turn_players(&self.world)
    }

    /// The round begins: once, before any player acts.
    pub fn start_round(&mut self) -> Vec<Event> {
        self.take(Step::StartRound)
    }

    /// The turn of `player` begins.
    pub fn start_player_turn(&mut self, player: PlayerId) -> Vec<Event> {
        self.take(Step::StartTurn(player))
    }

    /// The turn of `player` ends.
    pub fn end_player_turn(&mut self, player: PlayerId) -> Vec<Event> {
        self.take(Step::EndTurn(player))
    }

    /// The round ends: once, after the last player.
    pub fn end_round(&mut self) -> Vec<Event> {
        self.take(Step::EndRound)
    }

    /// The change of rounds for everyone at once, with nobody playing: every player ends their
    /// turn, the round ends and the next begins, and every player starts theirs. For tests and
    /// tools; a game that is played goes through the master, one seat after another.
    pub fn play_round(&mut self) -> Vec<Event> {
        self.take(Step::PlayRound)
    }

    /// Make a step. A game that has ended, or a player the game does not have, refuses it, and
    /// a refused step changes nothing and is not recorded.
    pub(super) fn take(&mut self, step: Step) -> Vec<Event> {
        if self.state.game_over.is_some() {
            return vec![Event::Rejected(Rejection::GameOver)];
        }
        if let Step::StartTurn(player) | Step::EndTurn(player) = step {
            if self.world.player(player).is_none() {
                return vec![Event::Rejected(Rejection::NoSuchPlayer)];
            }
        }
        self.record_step(step);
        self.forget_history();
        self.state.phase = None;
        let players = match step {
            Step::PlayRound => self.turn_players(),
            _ => Vec::new(),
        };
        let round = self.round();
        let next = match step {
            Step::StartRound | Step::PlayRound => round + 1,
            _ => round,
        };
        let over = &mut self.state.game_over;
        let mut events = self.world.advance(|world| match step {
            Step::StartRound => world.start_round(next),
            Step::StartTurn(player) => world.start_player_turn(player, round),
            Step::EndTurn(player) => world.end_player_turn(player),
            Step::EndRound => world.end_round(),
            Step::PlayRound => whole_round(world, &players, round, over),
        });
        self.state.round = next;
        events.extend(self.check_end());
        events
    }
}

/// [`Game::play_round`] on the world, which ends round `round` and begins the next. A game decided
/// by the end of the round ended in it.
fn whole_round(
    world: &mut World,
    players: &[PlayerId],
    round: u32,
    over: &mut Option<GameOver>,
) -> Vec<Event> {
    let mut events = Vec::new();
    for &p in players {
        events.extend(world.end_player_turn(p));
    }
    events.extend(world.end_round());
    events.extend(judge(world, round, over));
    events.extend(world.start_round(round + 1));
    for &p in players {
        events.extend(world.start_player_turn(p, round + 1));
    }
    events
}
