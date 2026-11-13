//! The scripted strategic AI in its two versions: the live crate (`f4x-ai`, being worked on) and
//! the frozen reference (`f4x-ai-ref`, made by `scripts/fork_ai.py`). The soak plays either, or
//! both in one game, through the small surface the two share.
//!
//! Tests: none of its own, because it only picks between the two AI crates; `metrics` plays it in a test.

use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use f4x_master::{Controller, Master, Seat, Status, Until};
use std::cell::RefCell;

/// Which copy of the AI plays a wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// `f4x-ai`, the AI under work.
    New,
    /// `f4x-ai-ref`, the frozen copy to measure against.
    Reference,
}

/// A command the game refused, as the soak's refused-order log keeps it.
pub struct Refused {
    pub turn: u32,
    pub player: PlayerId,
    pub source: String,
    pub command: String,
    pub reason: String,
}

/// What the soak needs from an AI player, whichever crate it comes from.
pub trait Contestant {
    fn player(&self) -> PlayerId;
    fn play_turn(&mut self, game: &mut Game) -> Vec<Event>;
    fn record_refusals(&mut self, on: bool);
    fn take_refusals(&self) -> Vec<Refused>;
    /// The AI's own state for the soak's detail lines: the tail of the player's line.
    fn describe(&self) -> String;
    /// The plans the AI's time limit has dropped, by kind (task 515); none for a version without it.
    fn plans_dropped(&self) -> Vec<(&'static str, u32)> {
        Vec::new()
    }
    /// What the AI has in mind, as text for a save (`Game::set_plans`); `None` for a version that
    /// cannot write it (a reference forked before plans existed).
    fn plans(&self) -> Option<String> {
        None
    }
    /// Take back the plans a loaded game carries for this player (`Game::plans`); a version that
    /// cannot read them starts afresh.
    fn recall(&mut self, _game: &Game) {}
}

macro_rules! contestant {
    ($krate:ident $(, plans $plans:ident)? $(, dropped $dropped:ident)?) => {
        impl Contestant for $krate::Ai {
            fn player(&self) -> PlayerId {
                self.player
            }
            fn play_turn(&mut self, game: &mut Game) -> Vec<Event> {
                $krate::Ai::play_turn(self, game)
            }
            fn record_refusals(&mut self, on: bool) {
                $krate::Ai::record_refusals(self, on);
            }
            fn take_refusals(&self) -> Vec<Refused> {
                $krate::Ai::take_refusals(self)
                    .into_iter()
                    .map(|r| Refused {
                        turn: r.turn,
                        player: r.player,
                        source: r.source,
                        command: r.command,
                        reason: r.reason,
                    })
                    .collect()
            }
            fn describe(&self) -> String {
                format!(
                    "{}, scouts {:?}, campaign {:?}, landing {:?}",
                    if self.frontier_closed() {
                        " (frontier closed)"
                    } else {
                        ""
                    },
                    self.scout_targets()
                        .into_iter()
                        .map(|(_, p)| (p.x, p.y))
                        .collect::<Vec<_>>(),
                    self.campaign().map(|c| (c.x, c.y)),
                    self.landing()
                        .map(|(t, port, f)| ((t.x, t.y), (port.x, port.y), f))
                )
            }
            $(
                fn plans_dropped(&self) -> Vec<(&'static str, u32)> {
                    self.$dropped()
                }
            )?
            $(
                fn plans(&self) -> Option<String> {
                    self.$plans().ok()
                }
                fn recall(&mut self, game: &Game) {
                    $krate::Ai::recall(game, std::slice::from_mut(self));
                }
            )?
        }
    };
}

contestant!(f4x_ai, plans to_text, dropped plans_dropped);
#[cfg(feature = "reference")]
contestant!(f4x_ai_ref, plans to_text);

/// The reference AI is not built in: say how to get it, rather than play something else.
#[cfg(not(feature = "reference"))]
fn without_reference() -> ! {
    panic!(
        "the reference AI is not built in; run this through `python3 scripts/ai_battle.py` \
         (or build f4x-tools with `--features reference`)"
    )
}

/// A player of the given version.
pub fn make(version: Version, player: PlayerId) -> Box<dyn Contestant> {
    match version {
        Version::New => Box::new(f4x_ai::Ai::new(player)),
        #[cfg(feature = "reference")]
        Version::Reference => Box::new(f4x_ai_ref::Ai::new(player)),
        #[cfg(not(feature = "reference"))]
        Version::Reference => without_reference(),
    }
}

/// The neutral players and the roaming monsters, played by one version of the AI. The new AI
/// keeps one `Ai` for each neutral race player across the rounds, so a game keeps one of these
/// for as long as it runs (the reference AI plays its own stateless neutrals).
pub struct Neutrals {
    version: Version,
    new: f4x_ai::NeutralSeat,
}

impl Neutrals {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            new: f4x_ai::NeutralSeat::default(),
        }
    }

    /// Keep the commands the game refuses, as the wizards' AIs do.
    pub fn record_refusals(&mut self, on: bool) {
        self.new.record_refusals(on);
    }

    /// The refused commands since the last call.
    pub fn take_refusals(&self) -> Vec<Refused> {
        self.new
            .take_refusals()
            .into_iter()
            .map(|r| Refused {
                turn: r.turn,
                player: r.player,
                source: r.source,
                command: r.command,
                reason: r.reason,
            })
            .collect()
    }

    /// What the neutral players have in mind, as text for a save (`Game::set_plans`).
    pub fn plans(&self) -> Vec<(PlayerId, String)> {
        self.new.plans()
    }

    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) {
        match self.version {
            Version::New => self.new.play_turn(game, players, events),
            #[cfg(feature = "reference")]
            Version::Reference => events.extend(f4x_ai_ref::play_neutrals(game)),
            #[cfg(not(feature = "reference"))]
            Version::Reference => without_reference(),
        }
    }
}

/// A contestant at its seat: it plays when the master calls, and its events are kept apart
/// from the master's (the soak reports each contestant's turn on its own).
struct ContestantSeat<'a> {
    contestant: &'a mut dyn Contestant,
    turns: &'a RefCell<Vec<(PlayerId, Vec<Event>)>>,
    times: &'a RefCell<RoundTimes>,
}

impl Controller for ContestantSeat<'_> {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], _: &mut Vec<Event>) -> Status {
        let started = std::time::Instant::now();
        let events = self.contestant.play_turn(game);
        self.times
            .borrow_mut()
            .players
            .push((self.contestant.player(), started.elapsed()));
        self.turns
            .borrow_mut()
            .push((self.contestant.player(), events));
        Status::Done
    }
}

/// The neutral seat of one version of the AI.
struct NeutralSeat<'a>(&'a mut Neutrals, &'a RefCell<RoundTimes>);

impl Controller for NeutralSeat<'_> {
    fn play(&mut self, game: &mut Game, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        let started = std::time::Instant::now();
        self.0.play(game, players, events);
        self.1.borrow_mut().neutrals += started.elapsed();
        Status::Done
    }
}

/// The rest of the current round is played by the master: each contestant in seat order (the
/// order of `players`), then the neutrals, then the round ends and the next one begins. Returns
/// the events of each contestant's turn, in order, and then those of the neutrals and the round
/// boundaries. A wizard without a contestant passes. Also returns the time each seat took.
/// The events of each contestant's turn, in seat order.
pub type PlayerTurns = Vec<(PlayerId, Vec<Event>)>;

/// Where the time of one round went (the soak's `--every` line adds these up).
#[derive(Debug, Default, Clone)]
pub struct RoundTimes {
    /// Each contestant's own turn (its AI step and its battles).
    pub players: Vec<(PlayerId, std::time::Duration)>,
    /// The neutral seat's turn.
    pub neutrals: std::time::Duration,
    /// The whole round, `run` to `run`.
    pub total: std::time::Duration,
}

impl RoundTimes {
    /// What no seat played: the turn changes, the other players' turn ends, the master itself.
    pub fn rest(&self) -> std::time::Duration {
        let seats: std::time::Duration = self
            .players
            .iter()
            .map(|(_, d)| *d)
            .sum::<std::time::Duration>()
            + self.neutrals;
        self.total.saturating_sub(seats)
    }
}

/// [`play_round`] with the time each seat took.
pub fn play_round_timed(
    game: &mut Game,
    players: &mut [Box<dyn Contestant>],
    neutrals: &mut Neutrals,
) -> (PlayerTurns, Vec<Event>, RoundTimes) {
    let turns = RefCell::new(Vec::new());
    let times = RefCell::new(RoundTimes::default());
    let started = std::time::Instant::now();
    let mut seats: Vec<Seat> = Vec::new();
    for p in players.iter_mut() {
        let player = p.player();
        seats.push(Seat::new(
            vec![player],
            Box::new(ContestantSeat {
                contestant: p.as_mut(),
                turns: &turns,
                times: &times,
            }),
        ));
    }
    seats.push(Seat::neutrals(Box::new(NeutralSeat(neutrals, &times))));
    let rest = Master::new(seats).run(game, Until::Rounds(1)).events;
    let mut times = times.into_inner();
    times.total = started.elapsed();
    (turns.into_inner(), rest, times)
}
