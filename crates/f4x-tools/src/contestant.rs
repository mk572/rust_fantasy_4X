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
}

macro_rules! contestant {
    ($krate:ident) => {
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
        }
    };
}

contestant!(f4x_ai);
#[cfg(feature = "reference")]
contestant!(f4x_ai_ref);

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

/// The independent cities and roaming monsters play their turn, by the given version.
pub fn play_neutrals(version: Version, game: &mut Game) -> Vec<Event> {
    match version {
        Version::New => f4x_ai::play_neutrals(game),
        #[cfg(feature = "reference")]
        Version::Reference => f4x_ai_ref::play_neutrals(game),
        #[cfg(not(feature = "reference"))]
        Version::Reference => without_reference(),
    }
}

/// A contestant at its seat: it plays when the master calls, and its events are kept apart
/// from the master's (the soak reports each contestant's turn on its own).
struct ContestantSeat<'a> {
    contestant: &'a mut dyn Contestant,
    turns: &'a RefCell<Vec<(PlayerId, Vec<Event>)>>,
}

impl Controller for ContestantSeat<'_> {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], _: &mut Vec<Event>) -> Status {
        let events = self.contestant.play_turn(game);
        self.turns
            .borrow_mut()
            .push((self.contestant.player(), events));
        Status::Done
    }
}

/// The neutral seat of one version of the AI.
struct NeutralSeat(Version);

impl Controller for NeutralSeat {
    fn play(&mut self, game: &mut Game, _: &[PlayerId], events: &mut Vec<Event>) -> Status {
        events.extend(play_neutrals(self.0, game));
        Status::Done
    }
}

/// The rest of the current round is played by the master: each contestant in seat order (the
/// order of `players`), then the neutrals, then the round ends and the next one begins. Returns
/// the events of each contestant's turn, in order, and then those of the neutrals and the round
/// boundaries. A wizard without a contestant passes.
pub fn play_round(
    game: &mut Game,
    players: &mut [Box<dyn Contestant>],
    neutrals: Version,
) -> (Vec<(PlayerId, Vec<Event>)>, Vec<Event>) {
    let turns = RefCell::new(Vec::new());
    let mut seats: Vec<Seat> = Vec::new();
    for p in players.iter_mut() {
        let player = p.player();
        seats.push(Seat::new(
            vec![player],
            Box::new(ContestantSeat {
                contestant: p.as_mut(),
                turns: &turns,
            }),
        ));
    }
    seats.push(Seat::neutrals(Box::new(NeutralSeat(neutrals))));
    let rest = Master::new(seats).run(game, Until::Rounds(1)).events;
    (turns.into_inner(), rest)
}
