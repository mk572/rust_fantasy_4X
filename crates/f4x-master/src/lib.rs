//! The master: it owns the flow of a game, one round after another and in each round one turn
//! per player (task 477, `docs/turn-structure.md`).
//!
//! ```text
//! round start   (once: the round counter, world events)
//!   for each player, in the order of the turns (`turn_players`):
//!     turn start   (moves refilled, offers, standing orders)
//!     the player's controller plays (a human may take many calls; an AI plays at once)
//!     turn end     (the player's cities, treasury, magic, healing)
//! round end     (once: durations and world-wide work)
//! ```
//!
//! A turn that has been played does not end while a proposal made in it waits for an answer: the
//! master asks the controller of whoever it waits for ([`Controller::react`]).
//!
//! Every player has a turn of its own: the wizards, then the independent race players, then the
//! monsters' player. A **seat** says who plays: one wizard's (a human or an AI), or the one that
//! plays for every player without a wizard, each in that player's turn. The master knows no
//! concrete AI and no front end: a [`Controller`] is given by the host (the client's human,
//! `f4x-ai`'s AIs). The simulation rules stay in `f4x-core`; the master decides *when* each step
//! of the turn order is made (`game/step.rs`), exactly once, and keeps a small [`State`] that is
//! saved with the game so that a load resumes without repeating or skipping a step.
//!
//! Tests: in tests/it.

mod convert;
mod game;

pub use game::end::{self, GameOver};
pub use game::reaction::{self, Reaction};
pub use game::replay::{self, Entry, Replay, ReplayError};
pub use game::save::{
    self, load_game, save_game, summarize_save, GameState, Loaded, Plans, Restored, SaveFile,
    SaveSummary,
};
pub use game::step::{self, turn_order, turn_players, Step};
pub use game::undo::{self, UndoError};
pub use game::Game;

use f4x_core::diplomacy::Proposal;
use f4x_core::player::{PlayerId, MONSTERS};
use f4x_core::world::{Command, Event, World};
use serde::{Deserialize, Serialize};

/// What a controller reports after it was asked to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The player's turn is over.
    Done,
    /// The player has more to do and needs more input (a human); ask again later.
    Waiting,
}

/// Plays the turns of a seat. The master calls [`Controller::play`] until it says `Done`; a
/// human controller answers `Waiting` until the person ends the turn, and never blocks.
pub trait Controller {
    /// Play or go on playing the turn of `player`. Events are appended to `events`.
    fn play(&mut self, game: &mut Game, player: PlayerId, events: &mut Vec<Event>) -> Status;

    /// Something of the turn being played waits for `player`: a proposal made to it. The
    /// controller answers it with commands, or leaves it. `Waiting` holds the turn until the
    /// controller is asked again (a human who has not answered); `Done` lets the turn go on,
    /// answered or not. Events are appended to `events`. The default leaves it.
    fn react(&mut self, _game: &mut Game, _player: PlayerId, _events: &mut Vec<Event>) -> Status {
        Status::Done
    }

    /// What the controller has in mind, as text to keep in a save, with the player each is for
    /// (the plans of the computer players it plays). The host asks for these when it saves the
    /// game ([`Master::plans`]). The default is nothing to keep.
    fn plans(&self) -> Vec<(PlayerId, String)> {
        Vec::new()
    }

    /// What has just happened in the game: the events of the step the master made, whoever
    /// played it. A computer player keeps what concerns it (it fought a battle on another
    /// player's turn, a city of it was taken) as a human's screen shows them. The default ignores
    /// them.
    fn notice(&mut self, _game: &Game, _events: &[Event]) {}
}

/// A controller that plays nothing and finishes at once: for a seat the host does not drive
/// (a human in a test or a tool that issues no commands).
pub struct Pass;

impl Controller for Pass {
    fn play(&mut self, _: &mut Game, _: PlayerId, _: &mut Vec<Event>) -> Status {
        Status::Done
    }
}

/// A seat at the table: the controller, and whose turns it plays.
pub struct Seat<'a> {
    /// The wizard's player, or `None` for the seat of the players without a wizard.
    player: Option<PlayerId>,
    controller: Box<dyn Controller + 'a>,
}

impl<'a> Seat<'a> {
    /// The seat of one wizard.
    pub fn new(player: PlayerId, controller: Box<dyn Controller + 'a>) -> Self {
        Self {
            player: Some(player),
            controller,
        }
    }

    /// The seat that plays for every player without a wizard (the independent race players and
    /// the monsters'), as they come and go during a game: it is asked once in the turn of each.
    pub fn non_wizards(controller: Box<dyn Controller + 'a>) -> Self {
        Self {
            player: None,
            controller,
        }
    }

    /// The wizard whose seat this is; `None` for the seat of the players without one.
    pub fn player(&self) -> Option<PlayerId> {
        self.player
    }
}

/// Where in the round the game is. Saved with the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// The round is about to begin.
    RoundStart,
    /// The player's turn is about to begin.
    TurnStart(PlayerId),
    /// The player is playing.
    Acting(PlayerId),
    /// The player's turn has been played and is about to end: it waits here while a proposal of
    /// the turn is unanswered.
    TurnEnd(PlayerId),
    /// Every player has played; the round is about to end.
    RoundEnd,
    /// The game is over (a winner, or no player can play).
    Over,
}

/// The master's place in the round: with the game's round counter it says exactly where to go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub phase: Phase,
}

/// Why a saved state cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    /// The state names a player no seat plays (the game was saved at another table).
    NoSeat { player: PlayerId },
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::NoSeat { player } => {
                write!(
                    f,
                    "the saved round state is at player {}, who has no seat here",
                    player.0
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
    /// `actor` is not the player that is acting now.
    NotYourTurn { active: PlayerId, actor: PlayerId },
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
    /// A controller waits for input; this is the player it plays: the one whose turn it is
    /// ([`Phase::Acting`]), or one a turn that has been played waits for ([`Phase::TurnEnd`]).
    Waiting(PlayerId),
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
    /// plays out the current round and begins the next.
    Rounds(u32),
}

/// The proposals a turn of `active` is not over with: those it made and those made to it.
fn requests(game: &Game, active: PlayerId) -> Vec<Proposal> {
    let open = game.diplomacy.proposals().iter();
    open.filter(|p| p.from == active || p.to == active)
        .copied()
        .collect()
}

/// The wizard that plays first of those seated; the monsters' player at a table without one.
fn first_wizard(seats: &[Seat]) -> PlayerId {
    seats
        .iter()
        .filter_map(Seat::player)
        .min_by_key(|&p| turn_order(p))
        .unwrap_or(MONSTERS)
}

/// Runs the rounds.
pub struct Master<'a> {
    seats: Vec<Seat<'a>>,
    state: State,
    checkpoints: bool,
}

impl<'a> Master<'a> {
    /// A master for a game whose current round has already begun (a new game, whose set-up did
    /// the opening, or a game loaded without a saved state): the first wizard's turn is next.
    pub fn new(seats: Vec<Seat<'a>>) -> Self {
        let phase = Phase::TurnStart(first_wizard(&seats));
        Self::resume(seats, State { phase })
    }

    /// A master that goes on from a saved state.
    pub fn resume(seats: Vec<Seat<'a>>, state: State) -> Self {
        Self {
            seats,
            state,
            checkpoints: false,
        }
    }

    /// A master that goes on from where the game was saved ([`Game::phase`], which the master
    /// writes after every step). A game without a phase was saved on the first wizard's turn:
    /// the human's, the seat the client plays and saves on. The player in the state must have a
    /// seat.
    pub fn from_game(seats: Vec<Seat<'a>>, game: &Game) -> Result<Self, FlowError> {
        let phase = game
            .phase()
            .unwrap_or_else(|| Phase::Acting(first_wizard(&seats)));
        let master = Self::resume(seats, State { phase });
        if let Phase::TurnStart(player) | Phase::Acting(player) | Phase::TurnEnd(player) = phase {
            if master.seat_of(game, player).is_none() {
                return Err(FlowError::NoSeat { player });
            }
        }
        Ok(master)
    }

    /// Stop at the start of every round ([`Stopped::RoundStart`]) so the host can save the game
    /// there: the one moment when no seat is half way through its turn.
    pub fn checkpoints(mut self) -> Self {
        self.checkpoints = true;
        self
    }

    /// What the controllers have in mind, to be saved with the game.
    pub fn plans(&self) -> Plans {
        self.seats
            .iter()
            .flat_map(|s| s.controller.plans())
            .collect()
    }

    /// The state to save with the game.
    pub fn state(&self) -> State {
        self.state
    }

    /// The seats, in playing order.
    pub fn seats(&self) -> &[Seat<'a>] {
        &self.seats
    }

    /// The player whose turn is being played now.
    pub fn active_player(&self) -> Option<PlayerId> {
        match self.state.phase {
            Phase::Acting(player) => Some(player),
            _ => None,
        }
    }

    /// Whether the game is over.
    pub fn is_over(&self) -> bool {
        self.state.phase == Phase::Over
    }

    /// Apply a command for `actor`: refused here unless it is the actor's turn; the game then
    /// refuses what is not the actor's to give ([`Game::apply`]).
    pub fn apply(
        &self,
        game: &mut Game,
        actor: PlayerId,
        command: Command,
    ) -> Result<Vec<Event>, Refused> {
        let active = self.active_player().ok_or(Refused::NobodyActing)?;
        if active != actor {
            return Err(Refused::NotYourTurn { active, actor });
        }
        Ok(game.apply(actor, command))
    }

    /// The index of the seat that plays `player`'s turns: the wizard's own, or for a player
    /// without a wizard the seat of those.
    fn seat_of(&self, world: &World, player: PlayerId) -> Option<usize> {
        let own = self.seats.iter().position(|s| s.player == Some(player));
        own.or_else(|| {
            let shared = || self.seats.iter().position(|s| s.player.is_none());
            (!world.is_wizard(player)).then(shared).flatten()
        })
    }

    /// Whether `player` plays when its place comes: it takes its turn ([`Game::takes_turn`]) and
    /// is seated.
    fn eligible(&self, game: &Game, player: PlayerId) -> bool {
        game.takes_turn(player) && self.seat_of(game, player).is_some()
    }

    /// What comes after the turn of `last` (`None`: at the start of the round): the start of the
    /// turn of the next player in the game's order that can play, or the end of the round.
    fn next_turn(&self, game: &Game, last: Option<PlayerId>) -> Phase {
        let order = game.turn_players();
        let from = last.map_or(0, |last| {
            let place = order.iter().position(|&p| p == last);
            place.map_or(order.len(), |i| i + 1)
        });
        order[from..]
            .iter()
            .copied()
            .find(|&p| self.eligible(game, p))
            .map_or(Phase::RoundEnd, Phase::TurnStart)
    }

    /// Ask whoever a proposal of the turn of `active` waits for to answer it
    /// ([`Controller::react`]); an answer in return is asked for too. Returns the player the
    /// turn waits for.
    fn held_by(
        &mut self,
        game: &mut Game,
        active: PlayerId,
        events: &mut Vec<Event>,
    ) -> Option<PlayerId> {
        loop {
            let open = requests(game, active);
            let mut asked: Vec<PlayerId> = open.iter().map(|p| p.to).collect();
            asked.sort_by_key(|&p| turn_order(p));
            asked.dedup();
            for player in asked {
                let Some(seat) = self.seat_of(game, player) else {
                    continue;
                };
                if self.seats[seat].controller.react(game, player, events) == Status::Waiting {
                    return Some(player);
                }
            }
            if requests(game, active) == open {
                return None;
            }
        }
    }

    /// Make one step: one boundary operation, or one call to a controller.
    pub fn step(&mut self, game: &mut Game) -> Progress {
        let mut events = Vec::new();
        let mut stopped = Stopped::Ready;
        if game.game_over().is_some() {
            self.state.phase = Phase::Over;
        }
        match self.state.phase {
            Phase::Over => stopped = Stopped::Over,
            Phase::RoundStart => {
                events.extend(game.start_round());
                self.state.phase = match self.next_turn(game, None) {
                    Phase::RoundEnd => Phase::Over,
                    first => first,
                };
            }
            Phase::TurnStart(player) => {
                if self.eligible(game, player) {
                    events.extend(game.start_player_turn(player));
                    self.state.phase = Phase::Acting(player);
                } else {
                    self.state.phase = self.next_turn(game, Some(player));
                }
            }
            Phase::Acting(player) => {
                let seat = self.seat_of(game, player);
                let status = seat.map_or(Status::Done, |i| {
                    self.seats[i].controller.play(game, player, &mut events)
                });
                match status {
                    Status::Done => self.state.phase = Phase::TurnEnd(player),
                    Status::Waiting => stopped = Stopped::Waiting(player),
                }
            }
            Phase::TurnEnd(player) => match self.held_by(game, player, &mut events) {
                Some(other) => stopped = Stopped::Waiting(other),
                // An answer may have ended the game.
                None if game.game_over().is_some() => self.state.phase = Phase::Over,
                None => {
                    events.extend(game.end_player_turn(player));
                    self.state.phase = self.next_turn(game, Some(player));
                }
            },
            Phase::RoundEnd => {
                events.extend(game.end_round());
                self.state.phase = if game.game_over().is_some() {
                    Phase::Over
                } else {
                    Phase::RoundStart
                };
                if self.checkpoints && self.state.phase == Phase::RoundStart {
                    stopped = Stopped::RoundStart;
                }
            }
        }
        if self.state.phase == Phase::Over {
            stopped = Stopped::Over;
        }
        // Whatever saves the game now saves where it is in its round.
        game.set_phase(self.state.phase);
        self.tell(game, &events);
        Progress { events, stopped }
    }

    /// Tell every seat what happened. A host that changes the game outside the master (a human's
    /// commands on the client) tells it the events they caused with this.
    pub fn tell(&mut self, game: &Game, events: &[Event]) {
        if events.is_empty() {
            return;
        }
        for seat in &mut self.seats {
            seat.controller.notice(game, events);
        }
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
