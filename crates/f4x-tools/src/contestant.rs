//! The scripted strategic AI in its two versions: the live crate (`f4x-ai`, being worked on) and
//! the frozen reference (`f4x-ai-ref`, made by `scripts/fork_ai.py`). The soak plays either, or
//! both in one game, through the small surface the two share.
//!
//! Tests: none of its own, because it only picks between the two AI crates; `metrics` plays it in a test.

use f4x_core::player::PlayerId;
use f4x_core::world::{Event, World};
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
    fn play_turn(&mut self, world: &mut World) -> Vec<Event>;
    fn record_refusals(&mut self, on: bool);
    fn take_refusals(&self) -> Vec<Refused>;
    /// The AI's own state for the soak's detail lines: the tail of the player's line.
    fn describe(&self) -> String;
    /// The diplomacy system the wizard plays and its state, for the detail lines (empty for a version
    /// without systems).
    fn diplomacy_note(&self, _world: &World) -> String {
        String::new()
    }
    /// The turns a situation called for a kind of spell the wizard did not know, by the kind; none
    /// for a version without the count.
    fn unmet_spell_wants(&self) -> Vec<(&'static str, u32)> {
        Vec::new()
    }
    /// The plans the AI's time limit has dropped, by kind (task 515); none for a version without it.
    fn plans_dropped(&self) -> Vec<(&'static str, u32)> {
        Vec::new()
    }
    /// Why settlers' plans have ended without a city, by reason (task 515); none for a version
    /// without it.
    fn settler_failures(&self) -> Vec<(&'static str, u32)> {
        Vec::new()
    }
    /// What the AI has in mind, as text for a save (`World::set_plans`); `None` for a version that
    /// cannot write it (a reference forked before plans existed).
    fn plans(&self) -> Option<String> {
        None
    }
    /// Take back the plans a loaded game carries for this player (`World::plans`); a version that
    /// cannot read them starts afresh.
    fn recall(&mut self, _world: &World) {}
    /// Join the game's table of memories (`f4x_ai::Diplomats`), and hear the events of the turns, as
    /// the live AI does; the reference copy has neither.
    fn join(&mut self, _diplomats: &f4x_ai::Diplomats) {}
    fn notice(&mut self, _world: &World, _events: &[Event]) {}
}

macro_rules! contestant {
    ($krate:ident $(, memory $memory:ident)? $(, plans $plans:ident)? $(, dropped $dropped:ident)? $(, settlers $settlers:ident)? $(, unmet $unmet:ident)?) => {
        impl Contestant for $krate::Ai {
            fn player(&self) -> PlayerId {
                self.player
            }
            fn play_turn(&mut self, world: &mut World) -> Vec<Event> {
                $krate::Ai::play_turn(self, world)
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
                fn unmet_spell_wants(&self) -> Vec<(&'static str, u32)> {
                    self.$unmet()
                }
            )?
            $(
                fn settler_failures(&self) -> Vec<(&'static str, u32)> {
                    self.$settlers()
                }
            )?
            $(
                fn join(&mut self, diplomats: &f4x_ai::Diplomats) {
                    $krate::Ai::$memory(self, diplomats);
                }
                fn notice(&mut self, world: &World, events: &[Event]) {
                    $krate::Ai::notice(self, world, events);
                }
                fn diplomacy_note(&self, world: &World) -> String {
                    $krate::Ai::diplomacy_note(self, world)
                }
            )?
            $(
                fn plans(&self) -> Option<String> {
                    self.$plans().ok()
                }
                fn recall(&mut self, world: &World) {
                    $krate::Ai::recall(world, std::slice::from_mut(self));
                }
            )?
        }
    };
}

contestant!(f4x_ai, memory join, plans to_text, dropped plans_dropped, settlers settler_failures, unmet unmet_spell_wants);
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

/// The neutral players and the roaming monsters, played by one version of the AI. Each version's
/// `NeutralSeat` keeps one `Ai` for each neutral race player across the rounds, so a game keeps one
/// of these for as long as it runs. Only the seat of the chosen version plays.
pub struct Neutrals {
    version: Version,
    new: f4x_ai::NeutralSeat,
    #[cfg(feature = "reference")]
    reference: f4x_ai_ref::NeutralSeat,
}

impl Neutrals {
    pub fn new(version: Version) -> Self {
        Self {
            version,
            new: f4x_ai::NeutralSeat::default(),
            #[cfg(feature = "reference")]
            reference: f4x_ai_ref::NeutralSeat::default(),
        }
    }

    /// Keep the commands the game refuses, as the wizards' AIs do.
    pub fn record_refusals(&mut self, on: bool) {
        match self.version {
            Version::New => self.new.record_refusals(on),
            #[cfg(feature = "reference")]
            Version::Reference => self.reference.record_refusals(on),
            #[cfg(not(feature = "reference"))]
            Version::Reference => without_reference(),
        }
    }

    /// The refused commands since the last call.
    pub fn take_refusals(&self) -> Vec<Refused> {
        match self.version {
            Version::New => self
                .new
                .take_refusals()
                .into_iter()
                .map(|r| Refused {
                    turn: r.turn,
                    player: r.player,
                    source: r.source,
                    command: r.command,
                    reason: r.reason,
                })
                .collect(),
            #[cfg(feature = "reference")]
            Version::Reference => self
                .reference
                .take_refusals()
                .into_iter()
                .map(|r| Refused {
                    turn: r.turn,
                    player: r.player,
                    source: r.source,
                    command: r.command,
                    reason: r.reason,
                })
                .collect(),
            #[cfg(not(feature = "reference"))]
            Version::Reference => without_reference(),
        }
    }

    /// What the neutral players have in mind, as text for a save (`World::set_plans`).
    pub fn plans(&self) -> Vec<(PlayerId, String)> {
        match self.version {
            Version::New => self.new.plans(),
            #[cfg(feature = "reference")]
            Version::Reference => self.reference.plans(),
            #[cfg(not(feature = "reference"))]
            Version::Reference => without_reference(),
        }
    }

    /// The race players join the game's table of memories.
    pub fn share(&mut self, diplomats: &f4x_ai::Diplomats) {
        if self.version == Version::New {
            self.new.share(diplomats);
        }
    }

    fn notice(&mut self, world: &World, events: &[Event]) {
        if self.version == Version::New {
            Controller::notice(&mut self.new, world, events);
        }
    }

    fn play(&mut self, world: &mut World, players: &[PlayerId], events: &mut Vec<Event>) {
        match self.version {
            Version::New => self.new.play_turn(world, players, events),
            #[cfg(feature = "reference")]
            Version::Reference => self.reference.play_turn(world, players, events),
            #[cfg(not(feature = "reference"))]
            Version::Reference => without_reference(),
        }
    }
}

thread_local! {
    /// Whose turn this thread is playing: the seed, the turn and the player, for a panic's report.
    static PLAYING: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Where this thread is in a game, as `seed S turn T player P`; empty outside a turn.
pub fn playing() -> String {
    PLAYING.with(|p| p.borrow().clone())
}

/// A contestant at its seat: it plays when the master calls, and its events are kept apart
/// from the master's (the soak reports each contestant's turn on its own).
struct ContestantSeat<'a> {
    contestant: &'a mut dyn Contestant,
    turns: &'a RefCell<Vec<(PlayerId, Vec<Event>)>>,
    times: &'a RefCell<RoundTimes>,
}

impl Controller for ContestantSeat<'_> {
    fn play(&mut self, world: &mut World, _: &[PlayerId], _: &mut Vec<Event>) -> Status {
        let started = std::time::Instant::now();
        PLAYING.with(|p| {
            *p.borrow_mut() = format!(
                "seed {} turn {} player {}",
                world.seed(),
                world.turn,
                self.contestant.player().0
            );
        });
        let events = self.contestant.play_turn(world);
        self.times
            .borrow_mut()
            .players
            .push((self.contestant.player(), started.elapsed()));
        self.turns
            .borrow_mut()
            .push((self.contestant.player(), events));
        Status::Done
    }

    fn notice(&mut self, world: &World, events: &[Event]) {
        self.contestant.notice(world, events);
    }
}

/// The neutral seat of one version of the AI.
struct NeutralSeat<'a>(&'a mut Neutrals, &'a RefCell<RoundTimes>);

impl Controller for NeutralSeat<'_> {
    fn play(&mut self, world: &mut World, players: &[PlayerId], events: &mut Vec<Event>) -> Status {
        let started = std::time::Instant::now();
        self.0.play(world, players, events);
        self.1.borrow_mut().neutrals += started.elapsed();
        Status::Done
    }

    fn notice(&mut self, world: &World, events: &[Event]) {
        self.0.notice(world, events);
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
    world: &mut World,
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
    let rest = Master::new(seats).run(world, Until::Rounds(1)).events;
    let mut times = times.into_inner();
    times.total = started.elapsed();
    (turns.into_inner(), rest, times)
}
