//! The master: it owns the flow of a game, one round after another and in each round one turn
//! per player (task 477, `docs/turn-structure.md`).
//!
//! ```text
//! round start   (once: the round counter, world events)
//!   for each seat, in order:
//!     turn start   (moves refilled, offers, standing orders)
//!     the seat's controller plays (a human may take many calls; an AI plays at once)
//!     turn end     (the player's cities, treasury, magic, healing)
//! round end     (once: durations and world-wide work)
//! ```
//!
//! A **seat** is who plays: one wizard (a human or an AI), or the neutral seat that plays all
//! the independent cities and the monsters, last in the round. The master knows no concrete AI
//! and no front end: a [`Controller`] is given by the host (the client's human, `f4x-ai`'s
//! wizards and neutrals). The simulation rules stay in `f4x-core`; the master decides *when*
//! each of its lifecycle operations runs, exactly once, and keeps a small [`State`] that is
//! saved with the game so that a load resumes without repeating or skipping a step.
//!
//! Tests: in tests/it.

use f4x_core::game::{Command, Event, Game};
use f4x_core::player::PlayerId;
use serde::{Deserialize, Serialize};

/// What a controller reports after it was asked to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The seat's turn is over.
    Done,
    /// The seat has more to do and needs more input (a human); ask again later.
    Waiting,
}

/// Plays the turns of a seat. The master calls [`Controller::play`] until it says `Done`; a
/// human controller answers `Waiting` until the person ends the turn, and never blocks.
pub trait Controller {
    /// Play or go on playing the turn of the seat's `players` (one wizard, or all the neutral
    /// players). Events are appended to `events`.
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status;
}

/// A seat at the table: the players it plays for and the controller that plays them.
pub struct Seat {
    players: Vec<PlayerId>,
    controller: Box<dyn Controller>,
}

impl Seat {
    /// A seat for the players in the order given (one wizard, or the neutral players).
    pub fn new(players: Vec<PlayerId>, controller: Box<dyn Controller>) -> Self {
        Self {
            players,
            controller,
        }
    }

    /// The players this seat plays.
    pub fn players(&self) -> &[PlayerId] {
        &self.players
    }
}

/// Where in the round the game is. Saved with the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// The round is about to begin.
    RoundStart,
    /// The seat's turn is about to begin.
    TurnStart(usize),
    /// The seat is playing.
    Acting(usize),
    /// The seat's turn has been played and is about to end.
    TurnEnd(usize),
    /// Every seat has played; the round is about to end.
    RoundEnd,
    /// The game is over (a winner, or no seat can play).
    Over,
}

/// The master's saved state: with the game's own round counter (`Game::turn`) it says exactly
/// where to go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub phase: Phase,
}

/// Why a command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Nobody is acting (between turns, or the game is over).
    NobodyActing,
    /// `actor` is not the seat that is acting now.
    NotYourTurn { active: PlayerId, actor: PlayerId },
    /// `actor` sent a command that acts for another player (that player's unit or city).
    ForAnotherPlayer { actor: PlayerId, named: PlayerId },
    /// Ending the turn is the controller's job, not a command; the lifecycle runs it.
    UseLifecycle,
}

/// What a step or a run came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub events: Vec<Event>,
    pub stopped: Stopped,
}

/// Why the master stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// One step was made and the master could go on.
    Ready,
    /// A controller waits for input; this is its seat.
    Waiting(usize),
    /// The game is over.
    Over,
}

/// How long [`Master::run`] goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Until {
    /// Until a controller waits for input, or the game ends.
    Waiting,
    /// Until this many rounds have ended (or a controller waits, or the game ends).
    Rounds(u32),
}

/// Runs the rounds.
pub struct Master {
    seats: Vec<Seat>,
    state: State,
}

impl Master {
    /// A master for a game whose current round has already begun (a new game, whose set-up did
    /// the opening, or a game loaded without a saved state): the first seat plays first.
    pub fn new(seats: Vec<Seat>) -> Self {
        Self {
            seats,
            state: State {
                phase: Phase::TurnStart(0),
            },
        }
    }

    /// A master that goes on from a saved state.
    pub fn resume(seats: Vec<Seat>, state: State) -> Self {
        Self { seats, state }
    }

    /// The state to save with the game.
    pub fn state(&self) -> State {
        self.state
    }

    /// The seats, in playing order.
    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// The seat that is acting now.
    pub fn active_seat(&self) -> Option<usize> {
        match self.state.phase {
            Phase::Acting(i) => Some(i),
            _ => None,
        }
    }

    /// The first player of the seat that is acting now.
    pub fn active_player(&self) -> Option<PlayerId> {
        self.active_seat()
            .and_then(|i| self.seats[i].players.first().copied())
    }

    /// Whether the game is over.
    pub fn is_over(&self) -> bool {
        self.state.phase == Phase::Over
    }

    /// Apply a command for `actor`: refused unless it is the actor's turn and the command acts
    /// for the actor (a command naming another player's unit or city is refused).
    pub fn apply(
        &self,
        game: &mut Game,
        actor: PlayerId,
        command: Command,
    ) -> Result<Vec<Event>, Refused> {
        if command == Command::EndTurn {
            return Err(Refused::UseLifecycle);
        }
        let seat = self.active_seat().ok_or(Refused::NobodyActing)?;
        let players = &self.seats[seat].players;
        if !players.contains(&actor) {
            return Err(Refused::NotYourTurn {
                active: players[0],
                actor,
            });
        }
        if let Some(named) = game.command_actor(&command) {
            if !players.contains(&named) {
                return Err(Refused::ForAnotherPlayer { actor, named });
            }
        }
        Ok(game.apply(command))
    }

    /// Whether the seat can take a turn: some player of it is still in the game.
    fn eligible(&self, game: &Game, seat: usize) -> bool {
        self.seats[seat]
            .players
            .iter()
            .any(|&p| game.player(p).is_some_and(|pl| !pl.is_defeated()))
    }

    /// The first seat from `from` that can take a turn.
    fn next_eligible(&self, game: &Game, from: usize) -> Option<usize> {
        (from..self.seats.len()).find(|&i| self.eligible(game, i))
    }

    /// Make one step: one boundary operation, or one call to a controller.
    pub fn step(&mut self, game: &mut Game) -> Progress {
        let mut events = Vec::new();
        let mut stopped = Stopped::Ready;
        if game.game_over.is_some() {
            self.state.phase = Phase::Over;
        }
        match self.state.phase {
            Phase::Over => stopped = Stopped::Over,
            Phase::RoundStart => {
                events.extend(game.start_round());
                self.state.phase = match self.next_eligible(game, 0) {
                    Some(i) => Phase::TurnStart(i),
                    None => Phase::Over,
                };
            }
            Phase::TurnStart(i) => {
                for &p in &self.seats[i].players {
                    events.extend(game.start_player_turn(p));
                }
                self.state.phase = Phase::Acting(i);
            }
            Phase::Acting(i) => {
                let players = self.seats[i].players.clone();
                match self.seats[i].controller.play(game, &players, &mut events) {
                    Status::Done => self.state.phase = Phase::TurnEnd(i),
                    Status::Waiting => stopped = Stopped::Waiting(i),
                }
            }
            Phase::TurnEnd(i) => {
                for &p in &self.seats[i].players {
                    events.extend(game.end_player_turn(p));
                }
                // A player may have been defeated during the turn; the next one to play is the
                // next seat that is still in the game.
                self.state.phase = match self.next_eligible(game, i + 1) {
                    Some(next) => Phase::TurnStart(next),
                    None => Phase::RoundEnd,
                };
            }
            Phase::RoundEnd => {
                events.extend(game.end_round());
                self.state.phase = if game.game_over.is_some() {
                    Phase::Over
                } else {
                    Phase::RoundStart
                };
            }
        }
        if self.state.phase == Phase::Over {
            stopped = Stopped::Over;
        }
        Progress { events, stopped }
    }

    /// Step until the stop condition, collecting the events.
    pub fn run(&mut self, game: &mut Game, until: Until) -> Progress {
        let mut events = Vec::new();
        let mut rounds = 0;
        loop {
            let ending = self.state.phase == Phase::RoundEnd;
            let p = self.step(game);
            events.extend(p.events);
            if ending && self.state.phase != Phase::RoundEnd {
                rounds += 1;
            }
            match p.stopped {
                Stopped::Ready => {}
                stopped => return Progress { events, stopped },
            }
            if let Until::Rounds(n) = until {
                if rounds >= n {
                    return Progress {
                        events,
                        stopped: Stopped::Ready,
                    };
                }
            }
        }
    }
}
