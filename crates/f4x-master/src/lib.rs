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

    /// What the controller has in mind, as text to keep in a save, with the player each is for
    /// (the plans of the computer players it plays). The master collects these at every checkpoint
    /// ([`Master::checkpoints`]) and gives them to the game ([`Game::set_plans`]). The default
    /// is nothing to keep.
    fn plans(&self) -> Vec<(PlayerId, String)> {
        Vec::new()
    }
}

/// A controller that plays nothing and finishes at once: for a seat the host does not drive
/// (a human in a test or a tool that issues no commands).
pub struct Pass;

impl Controller for Pass {
    fn play(&mut self, _: &mut Game, _: &[PlayerId], _: &mut Vec<Event>) -> Status {
        Status::Done
    }
}

/// A seat at the table: the players it plays for and the controller that plays them.
pub struct Seat<'a> {
    players: Vec<PlayerId>,
    neutral: bool,
    controller: Box<dyn Controller + 'a>,
}

impl<'a> Seat<'a> {
    /// A seat for the players in the order given (one wizard, or the neutral players).
    pub fn new(players: Vec<PlayerId>, controller: Box<dyn Controller + 'a>) -> Self {
        Self {
            players,
            neutral: false,
            controller,
        }
    }

    /// The neutral seat: it plays every independent city's player and the monsters' owner, as
    /// they are when its turn comes (cities turn neutral during a game). It always takes a turn.
    pub fn neutrals(controller: Box<dyn Controller + 'a>) -> Self {
        Self {
            players: Vec::new(),
            neutral: true,
            controller,
        }
    }

    /// Whether this is the neutral seat.
    pub fn is_neutral(&self) -> bool {
        self.neutral
    }

    /// The players this seat plays (none listed for the neutral seat; see
    /// [`Master::players_of`]).
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

impl State {
    /// The state as text, for [`Game::set_flow`].
    pub fn to_text(self) -> String {
        // A plain enum in a struct cannot fail to serialize.
        ron::to_string(&self).unwrap_or_default()
    }

    /// The state from the text [`State::to_text`] made.
    pub fn from_text(text: &str) -> Result<Self, FlowError> {
        ron::from_str(text).map_err(|e| FlowError::Unreadable(e.to_string()))
    }
}

/// Why a saved flow cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    /// The text is not a master state.
    Unreadable(String),
    /// The state names a seat the table does not have (the game was saved at another table).
    NoSuchSeat { seat: usize, seats: usize },
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::Unreadable(why) => write!(f, "the saved round state is unreadable: {why}"),
            FlowError::NoSuchSeat { seat, seats } => {
                write!(
                    f,
                    "the saved round state is at seat {seat}, the table has {seats}"
                )
            }
        }
    }
}

impl std::error::Error for FlowError {}

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
    /// The round is about to begin and the master was asked to stop there
    /// ([`Master::checkpoints`]): the game can be saved now, and the next step starts the round.
    RoundStart,
}

/// How long [`Master::run`] goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Until {
    /// Until a controller waits for input, or the game ends.
    Waiting,
    /// Until this many more rounds have begun (the round counter has advanced and the round-start
    /// work is done), or a controller waits, or the game ends. `Rounds(1)` from a fresh master
    /// plays out the current round and begins the next, as the old `EndTurn` did.
    Rounds(u32),
}

/// Runs the rounds.
pub struct Master<'a> {
    seats: Vec<Seat<'a>>,
    state: State,
    checkpoints: bool,
}

impl<'a> Master<'a> {
    /// A master for a game whose current round has already begun (a new game, whose set-up did
    /// the opening, or a game loaded without a saved state): the first seat plays first.
    pub fn new(seats: Vec<Seat<'a>>) -> Self {
        Self {
            seats,
            state: State {
                phase: Phase::TurnStart(0),
            },
            checkpoints: false,
        }
    }

    /// A master that goes on from a saved state.
    pub fn resume(seats: Vec<Seat<'a>>, state: State) -> Self {
        Self {
            seats,
            state,
            checkpoints: false,
        }
    }

    /// A master that goes on from where the game was saved (`Game::flow`, which the master
    /// writes after every step). A game without a flow was saved on the first seat's turn: the
    /// human's, the seat the client plays and saves on. The state must fit the table.
    pub fn from_game(seats: Vec<Seat<'a>>, game: &Game) -> Result<Self, FlowError> {
        let state = match game.flow() {
            Some(text) => State::from_text(text)?,
            None => State {
                phase: Phase::Acting(0),
            },
        };
        let seat = match state.phase {
            Phase::TurnStart(i) | Phase::Acting(i) | Phase::TurnEnd(i) => Some(i),
            _ => None,
        };
        if let Some(seat) = seat.filter(|&i| i >= seats.len()) {
            return Err(FlowError::NoSuchSeat {
                seat,
                seats: seats.len(),
            });
        }
        Ok(Self::resume(seats, state))
    }

    /// Stop at the start of every round ([`Stopped::RoundStart`]) so the host can save the game
    /// there: the one moment when no seat is half way through its turn.
    pub fn checkpoints(mut self) -> Self {
        self.checkpoints = true;
        self
    }

    /// The state to save with the game.
    pub fn state(&self) -> State {
        self.state
    }

    /// The seats, in playing order.
    pub fn seats(&self) -> &[Seat<'a>] {
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
        let players = self.players_of(game, seat);
        if !players.contains(&actor) {
            return Err(Refused::NotYourTurn {
                active: players.first().copied().unwrap_or(actor),
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

    /// The players a seat plays now: its own, or for the neutral seat every independent city's
    /// player and the monsters' owner that exist.
    pub fn players_of(&self, game: &Game, seat: usize) -> Vec<PlayerId> {
        if self.seats[seat].neutral {
            game.turn_players()
                .into_iter()
                .filter(|&p| game.is_neutral_player(p))
                .collect()
        } else {
            self.seats[seat].players.clone()
        }
    }

    /// Whether the seat can take a turn: some player of it is still in the game.
    fn eligible(&self, game: &Game, seat: usize) -> bool {
        self.seats[seat].neutral
            || self.seats[seat]
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
                events.extend(game.apply(Command::StartRound));
                self.state.phase = match self.next_eligible(game, 0) {
                    Some(i) => Phase::TurnStart(i),
                    None => Phase::Over,
                };
            }
            Phase::TurnStart(i) => {
                for p in self.players_of(game, i) {
                    events.extend(game.apply(Command::StartPlayerTurn { player: p }));
                }
                self.state.phase = Phase::Acting(i);
            }
            Phase::Acting(i) => {
                let players = self.players_of(game, i);
                match self.seats[i].controller.play(game, &players, &mut events) {
                    Status::Done => self.state.phase = Phase::TurnEnd(i),
                    Status::Waiting => stopped = Stopped::Waiting(i),
                }
            }
            Phase::TurnEnd(i) => {
                for p in self.players_of(game, i) {
                    events.extend(game.apply(Command::EndPlayerTurn { player: p }));
                }
                // A player may have been defeated during the turn; the next one to play is the
                // next seat that is still in the game.
                self.state.phase = match self.next_eligible(game, i + 1) {
                    Some(next) => Phase::TurnStart(next),
                    None => Phase::RoundEnd,
                };
            }
            Phase::RoundEnd => {
                events.extend(game.apply(Command::EndRound));
                self.state.phase = if game.game_over.is_some() {
                    Phase::Over
                } else {
                    Phase::RoundStart
                };
                if self.checkpoints && self.state.phase == Phase::RoundStart {
                    stopped = Stopped::RoundStart;
                    let plans: Vec<_> = self
                        .seats
                        .iter()
                        .flat_map(|s| s.controller.plans())
                        .collect();
                    if !plans.is_empty() {
                        game.set_plans(plans);
                    }
                }
            }
        }
        if self.state.phase == Phase::Over {
            stopped = Stopped::Over;
        }
        // Whatever saves the game now saves where it is in its round.
        game.set_flow(Some(self.state.to_text()));
        Progress { events, stopped }
    }

    /// Step until the stop condition, collecting the events.
    pub fn run(&mut self, game: &mut Game, until: Until) -> Progress {
        let mut events = Vec::new();
        let mut begun = 0;
        loop {
            let starting = self.state.phase == Phase::RoundStart;
            let p = self.step(game);
            events.extend(p.events);
            if starting {
                begun += 1;
            }
            match p.stopped {
                Stopped::Ready => {}
                stopped => return Progress { events, stopped },
            }
            if matches!(until, Until::Rounds(n) if begun >= n) {
                return Progress {
                    events,
                    stopped: Stopped::Ready,
                };
            }
        }
    }
}
