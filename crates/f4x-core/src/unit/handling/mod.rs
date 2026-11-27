//! Unit handling: a copy of the computer player's routines (`f4x-ai`) that keep a unit at its
//! job. Each module is the AI's file of the same name with only the changes needed to run in core:
//! - the routines run for a [`Handler`] (the AI's `Ai`) against a [`Host`] (the AI's `Game`);
//! - what the AI decides by its strategy is asked of a [`Judgement`];
//! - the war jobs (muster, march, raid, perimeter, strikes, landings) and production (engineers,
//!   ships, recruits) are not here;
//! - `view` holds the few helpers the routines took from other AI modules.
//!
//! Nothing in the game calls it yet.
//!
//! Tests: in each module (the AI's tests of the copied routines), and at the end of this file full
//! turns played through [`Handler::play`]; the test world is `testworld.rs`.

pub mod bridge;
pub mod dispatch;
pub mod escort;
pub mod expand;
pub mod meld;
pub mod pickup;
pub mod reinforce;
pub mod road;
pub mod roles;
pub mod scout;
pub mod view;

#[cfg(test)]
pub(crate) mod testworld;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::map::Position;
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use crate::unit::movement::MoveError;
use crate::world::rng::GameRng;
use crate::world::serial::Serial;
use crate::world::{Command, Event, Rejection, World};

/// The game the routines run in: they read the world and send it commands.
pub trait Host: std::ops::Deref<Target = World> {
    fn command(&mut self, command: Command) -> Vec<Event>;

    /// The round being played.
    fn round(&self) -> u32;
}

/// A world and its dice, as a [`Host`].
pub struct Live<'a> {
    pub world: &'a mut World,
    pub rng: &'a mut GameRng,
    pub round: u32,
}

impl std::ops::Deref for Live<'_> {
    type Target = World;

    fn deref(&self) -> &World {
        self.world
    }
}

impl Host for Live<'_> {
    fn command(&mut self, command: Command) -> Vec<Event> {
        self.world.apply(command, self.rng)
    }

    fn round(&self) -> u32 {
        self.round
    }
}

/// Why a settler lost its site, or has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlerFate {
    NoSite,
    SiteTaken,
    SiteUnusable,
    WayBlocked,
}

/// What is decided by whoever the units are handled for, not by the handling: a computer player
/// answers from its strategy. The answers here are those of an owner with no strategy.
pub trait Judgement {
    /// The soldiers standing in the city at `at` that are its garrison.
    fn garrison(
        &self,
        _view: &PlayerView,
        _at: Position,
        _soldiers: &[hecs::Entity],
        _round: u32,
    ) -> Vec<hecs::Entity> {
        Vec::new()
    }

    /// The own cities under threat, each with the strength it lacks.
    fn threats(&self, _view: &PlayerView) -> Vec<(Position, f32)> {
        Vec::new()
    }

    /// How many scouts are wanted: with fewer out, a unit is recruited.
    fn scouts_wanted(&self, _view: &PlayerView, _round: u32) -> usize {
        0
    }

    /// Whether the owner is still building up: it then crosses to another plane only for ground
    /// seen to be unexplored there.
    fn building_up(&self, _view: &PlayerView, _round: u32) -> bool {
        false
    }

    /// Whether a way, on foot or by ship, still leads to a settler's site.
    fn site_reachable(&self, _view: &PlayerView, _site: Position, _round: u32) -> bool {
        true
    }

    /// A settler lost its site, or has none.
    fn settler_lost(&self, _fate: SettlerFate) {}
}

/// An owner with no strategy.
pub struct NoJudgement;

impl Judgement for NoJudgement {}

/// A command of the handling that the game refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub turn: u32,
    pub command: String,
    pub reason: String,
}

pub const DETAIL_SEPARATOR: &str = " | ";

pub(crate) fn with_detail(reason: String, detail: Option<String>) -> String {
    match detail {
        Some(d) => format!("{reason}{DETAIL_SEPARATOR}{d}"),
        None => reason,
    }
}

/// Why a command did not go through: a rule refused it, or a standing order was taken and then
/// stopped on a refused step at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    Rejected(Rejection),
    Stopped(MoveError),
}

impl SendError {
    /// The square the units were sent to has no room for them.
    pub fn is_no_room(self) -> bool {
        matches!(
            self,
            Self::Stopped(MoveError::StackFull | MoveError::NoRoom)
                | Self::Rejected(Rejection::Move(MoveError::StackFull | MoveError::NoRoom))
        )
    }
}

/// The units of one player under handling.
pub struct Handler {
    pub player: PlayerId,
    pub judge: Rc<dyn Judgement>,
    pub roles: roles::Registry,
    /// Nothing reachable shows anything new: the scouting is over.
    pub frontier_closed: bool,
    /// Squares scouting gave up on.
    pub bad_scout_targets: Vec<Position>,
    /// The tower the planar scout is bound for.
    pub planar_tower: Option<Position>,
    /// A ship is wanted for scouting, and whether that search is over.
    pub sea_scouting: bool,
    pub sea_done: bool,
    /// Towns the scouts look at before others.
    pub recon: Vec<Position>,
    /// Where engineers build toward once the own cities are linked.
    pub campaign: Option<Position>,
    /// The number of cities when `road_checked` was begun, and the cities with no link to build.
    pub road_cities: usize,
    pub road_checked: Vec<Serial>,
    pub missions_reset: u32,
    /// Keep the refused commands for [`Handler::take_refusals`].
    pub record_refusals: bool,
    refusals: RefCell<Vec<Refusal>>,
    /// Walks stopped for lack of room, sent again at the end of the turn.
    blocked: RefCell<Vec<Command>>,
    retrying: Cell<bool>,
}

impl Handler {
    pub fn new(player: PlayerId) -> Self {
        Self::judged(player, Rc::new(NoJudgement))
    }

    pub fn judged(player: PlayerId, judge: Rc<dyn Judgement>) -> Self {
        Self {
            player,
            judge,
            roles: roles::Registry::default(),
            frontier_closed: false,
            bad_scout_targets: Vec::new(),
            planar_tower: None,
            sea_scouting: false,
            sea_done: false,
            recon: Vec::new(),
            campaign: None,
            road_cities: 0,
            road_checked: Vec::new(),
            missions_reset: 0,
            record_refusals: false,
            refusals: RefCell::default(),
            blocked: RefCell::default(),
            retrying: Cell::new(false),
        }
    }

    /// One turn of the handling. While another player keeps Time Stop going, no unit can act.
    pub fn play(&mut self, game: &mut dyn Host) -> Vec<Event> {
        let round = game.round();
        let mut events = Vec::new();
        // Units built or hired since the last turn await orders from the first moment.
        roles::register(self, game, round);
        let me = self.player;
        if !game.time_stopper().is_some_and(|keeper| keeper != me) {
            dispatch::run(self, game, &mut events);
        }
        events
    }

    /// Keep every command the game refuses, for [`Handler::take_refusals`].
    pub fn record_refusals(&mut self, on: bool) {
        self.record_refusals = on;
    }

    /// The squares the scouts are bound for.
    pub fn scout_targets(&self) -> Vec<Position> {
        self.roles.targets(roles::Role::Scout)
    }

    /// The refused commands kept since the last call.
    pub fn take_refusals(&self) -> Vec<Refusal> {
        std::mem::take(&mut *self.refusals.borrow_mut())
    }

    /// Keep a failure that is no refused command in the refused-order log.
    pub(crate) fn note_failure(&self, game: &dyn Host, command: String, reason: String) {
        if self.record_refusals {
            self.refusals.borrow_mut().push(Refusal {
                turn: game.round(),
                command,
                reason,
            });
        }
    }

    /// Whether `count` more units fit on the square.
    pub(crate) fn room_at(world: &World, to: Position, count: usize) -> bool {
        world.economy().army_at(to).len() + count <= world.max_tile()
    }

    /// Send a command and keep its events.
    pub(crate) fn send(
        &self,
        game: &mut dyn Host,
        command: Command,
        events: &mut Vec<Event>,
    ) -> Result<(), SendError> {
        self.submit(game, command, events, true)
    }

    /// Send a command whose refusal changes nothing in the plan.
    pub(crate) fn order(&self, game: &mut dyn Host, command: Command, events: &mut Vec<Event>) {
        let _ = self.submit(game, command, events, true);
    }

    /// Send a command that the game may refuse as expected: its refusal is not kept.
    pub(crate) fn probe(
        &self,
        game: &mut dyn Host,
        command: Command,
        events: &mut Vec<Event>,
    ) -> Result<(), SendError> {
        self.submit(game, command, events, false)
    }

    fn submit(
        &self,
        game: &mut dyn Host,
        command: Command,
        events: &mut Vec<Event>,
        keep_refusal: bool,
    ) -> Result<(), SendError> {
        let kept = (self.record_refusals && keep_refusal).then(|| format!("{command:?}"));
        let walk_command = matches!(command, Command::GoTo { .. } | Command::MoveStack { .. })
            .then(|| command.clone());
        let out = game.command(command);
        // A refusal first; else an order that was taken and stopped on its first step.
        let refused = out
            .iter()
            .find_map(|e| match e {
                Event::Rejected(why) => Some(SendError::Rejected(*why)),
                _ => None,
            })
            .or_else(|| {
                out.iter().find_map(|e| match e {
                    Event::OrderFailed { player, why, .. } if *player == self.player => {
                        Some(SendError::Stopped(*why))
                    }
                    _ => None,
                })
            });
        if let Some(command) = kept {
            for e in &out {
                let reason = match e {
                    Event::OrderFailed { why, player, .. } if *player == self.player => {
                        format!("OrderFailed {why:?}")
                    }
                    Event::Rejected(Rejection::NoPath(n)) => format!("NoPath({})", n.name()),
                    Event::Rejected(other) => format!("{other:?}"),
                    _ => continue,
                };
                self.refusals.borrow_mut().push(Refusal {
                    turn: game.round(),
                    command: command.clone(),
                    reason,
                });
            }
        }
        events.extend(out);
        if let (Some(why), Some(walk), false) = (refused, walk_command, self.retrying.get()) {
            if why.is_no_room() {
                self.blocked.borrow_mut().push(walk);
            }
        }
        refused.map_or(Ok(()), Err)
    }

    /// Send again the walking orders the game stopped for lack of room, now that the other jobs
    /// have moved their units, as long as one more gets through.
    pub(crate) fn retry_blocked(&self, game: &mut dyn Host, events: &mut Vec<Event>) {
        let mut waiting = std::mem::take(&mut *self.blocked.borrow_mut());
        self.retrying.set(true);
        while !waiting.is_empty() {
            let mut still = Vec::new();
            let mut sent = false;
            for command in waiting {
                match self.submit(game, command.clone(), events, false) {
                    Err(why) if why.is_no_room() => still.push(command),
                    Ok(()) => sent = true,
                    Err(_) => {}
                }
            }
            if !sent {
                break;
            }
            waiting = still;
        }
        self.retrying.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::roles::Role;
    use super::testworld::{
        at, game, give_site, handler, site_over_the_sea, spawn, Game, COAST_CITY, ENEMY_CITY, P0,
    };
    use super::*;

    /// Play whole turns until `done`, for 80 turns at most.
    fn play_until(w: &mut Game, h: &mut Handler, done: impl Fn(&Game, &Handler) -> bool) {
        for _ in 0..80 {
            if done(w, h) {
                return;
            }
            w.balance_farmers(P0);
            h.play(w);
            w.play_round();
        }
        panic!("not done after 80 turns: {:?}", h.roles);
    }

    fn home() -> Position {
        at(COAST_CITY.0, COAST_CITY.1)
    }

    /// A square of the far island's shore.
    fn far_shore(w: &World) -> Position {
        let view = PlayerView::new(w, P0);
        view::landing(&view, at(ENEMY_CITY.0, ENEMY_CITY.1))
            .expect("a shore on the far island")
            .0
    }

    /// Soldiers across the sea, told to reinforce a city, are fetched by a ship and arrive.
    #[test]
    fn soldiers_sent_to_a_city_over_the_sea_are_fetched_and_arrive() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let shore = far_shore(&w);
        let soldiers = spawn(&mut w, P0, shore, 104 /* high_men_spearmen */, 2);
        spawn(&mut w, P0, home(), 36 /* galley */, 1);
        let mut h = handler(P0);
        roles::register(&mut h, &w, 1);
        for &u in &soldiers {
            let s = PlayerView::new(&w, P0).serial_of(u);
            h.roles
                .claim(s, Role::Reinforce, 1, Some(home()))
                .expect("a free soldier");
        }
        play_until(&mut w, &mut h, |w, _| {
            soldiers
                .iter()
                .all(|&u| w.units().unit_position(u) == Some(home()))
        });
    }

    /// A settler given a site over the sea is carried there and founds its city.
    #[test]
    fn a_settler_given_a_site_over_the_sea_founds_its_city_there() {
        let mut w = game(COAST_CITY, 8 /* high_men */);
        w.reveal_map(P0);
        let site = site_over_the_sea(&w);
        let settler = spawn(&mut w, P0, home(), 111 /* high_men_settlers */, 1)[0];
        spawn(&mut w, P0, home(), 36 /* galley */, 1);
        let mut h = handler(P0);
        give_site(&mut h, &w, settler, site);
        play_until(&mut w, &mut h, |w, _| w.economy().city_at(site).is_some());
    }
}
