//! AI players: they look at the game the way a player does and answer with `Command`s sent
//! through `Game::apply`. Nothing here reaches into the game's internals.
//!
//! One [`Ai`] plays one wizard. [`Ai::play_turn`] does that wizard's whole turn (research,
//! spells, city production, settlers, the army) and leaves the turn's boundaries to `f4x-master`,
//! which runs the rounds and the turn of each seat in order; [`AiSeat`] and [`NeutralSeat`] are
//! the controllers it calls, and [`play_round`] plays one round for a list of AIs.
//!
//! Tests: in the modules (each decision has its own file and tests); `tests/it/` has the rules of
//! the ships and the slow whole-game behaviour tests (ignored by default).

mod alchemy;
mod assault;
pub mod battle;
mod colony;
mod controller;
mod craft;
mod diplomacy;
mod economy;
mod expand;
mod garrison;
mod military;
mod monster;
mod naval;
mod objective;
mod offer;
mod persist;
mod phase;
mod plan_clock;
mod race;
mod raze;
mod scout;
mod shortfall;
mod site;
mod spellhelper;
mod statecraft;
#[cfg(test)]
mod testworld;
mod trade;
mod view;
mod wizardry;

use std::cell::RefCell;
use std::collections::HashMap;

use f4x_core::game::serial::Serial;
use f4x_core::game::{Command, Event, Game};
use f4x_core::map::Position;
use f4x_core::player::view::PlayerView;
use f4x_core::player::PlayerId;

/// A command the game refused, with where in the AI it was sent from (for finding out why the AI
/// sends orders the game will not take).
#[derive(Debug, Clone)]
pub struct Refusal {
    pub turn: u32,
    pub player: PlayerId,
    /// `file:line` of the `send` call.
    pub source: String,
    pub command: String,
    pub reason: String,
}

/// One computer-played wizard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ai {
    pub player: PlayerId,
    /// Where each settler is heading to found a city.
    #[serde(serialize_with = "persist::sorted")]
    settler_targets: HashMap<Serial, Position>,
    /// The enemy city the army is gathering to attack, once it has chosen one.
    campaign: Option<Position>,
    /// Where each settler stood last turn, to notice one that is stuck.
    #[serde(serialize_with = "persist::sorted")]
    settler_last: HashMap<Serial, Position>,
    /// Sites the AI gave up on.
    bad_sites: Vec<Position>,
    /// The units set to find the rest of the world, and where each is heading.
    scouts: Vec<Serial>,
    #[serde(serialize_with = "persist::sorted")]
    scout_targets: HashMap<Serial, Position>,
    /// Where each scout stood last turn and for how many turns it has not moved.
    #[serde(serialize_with = "persist::sorted")]
    scout_last: HashMap<Serial, (Position, u32)>,
    /// Targets scouts could not get to.
    bad_scout_targets: Vec<Position>,
    /// The units on the march against `march_target`, and the turn they set out.
    marching: Vec<Serial>,
    march_started: u32,
    march_target: Option<Position>,
    /// The landing the AI is working towards when the enemy lives on another land mass, the ship
    /// on a voyage, and ports or targets that did not work out.
    naval: Option<naval::Plan>,
    /// The ships on a voyage with a landing, each with the units it carries.
    #[serde(default)]
    ferries: Vec<naval::Ferry>,
    naval_bad: Vec<Position>,
    /// Turns a loaded ship has waited for a stronger landing party.
    naval_wait: u32,
    /// Looking for the rest of the world by ship: the city that builds it, whether a ship is out
    /// scouting, and whether that is over.
    sea_port: Option<Position>,
    sea_scouting: bool,
    sea_done: bool,
    /// The fighters a sea scout carries to put ashore on land nobody holds (`scout.rs`).
    sea_party: Vec<Serial>,
    /// The sea scouting was ended by the plan clock (`plan_clock.rs`), not by finding nothing:
    /// it is tried again when the failure lists are forgiven.
    #[serde(default)]
    sea_gave_up: bool,
    /// The open tower the planar scout is bound for, to be set aside when it does not get there.
    #[serde(default)]
    planar_tower: Option<Position>,
    /// The scout on its way to an open tower, to change plane (`scout.rs`).
    planar: Option<Serial>,
    /// The open tower an army crosses by to reach a rival's city on the other plane (`military.rs`).
    #[serde(default)]
    crossing: Option<Position>,
    /// Settlers by sea (`colony.rs`): the voyage planned, the ship and settler on it and the turn
    /// they set out, landings that failed, and the turn the search is made again.
    colony: Option<colony::Colony>,
    colonists: Option<(Serial, Serial)>,
    colony_since: u32,
    colony_bad: Vec<Position>,
    colony_retry: u32,
    /// Own cities with a stack of a wizard at war near and too few defenders, this turn.
    threatened: Vec<Position>,
    /// The strength last seen in each enemy city and the turn it was seen.
    #[serde(serialize_with = "persist::sorted")]
    city_memory: HashMap<Position, (f32, u32)>,
    /// Enemy cities no march found a way to, and when the last was added.
    unreachable: Vec<Position>,
    unreachable_since: u32,
    /// No reachable square shows anything new: scouting by land is over.
    frontier_closed: bool,
    /// Whether refused commands are kept (see [`Ai::take_refusals`]) and the ones kept so far.
    /// What was last seen of each rival casting the Spell of Mastery (`race.rs`).
    #[serde(serialize_with = "persist::sorted")]
    mastery_seen: HashMap<PlayerId, race::Sighting>,
    /// The AI means to win the race for the Spell of Mastery, or has declared war on the wizard
    /// it cannot beat to it.
    racing: bool,
    war_on: Option<PlayerId>,
    /// The wizard this one went to war on by temper (`diplomacy::quarrel`).
    quarrel: Option<PlayerId>,
    /// What the statecraft (`statecraft.rs`, task 510) keeps between turns.
    #[serde(default)]
    statecraft: statecraft::State,
    #[serde(skip)]
    record_refusals: bool,
    #[serde(skip)]
    refusals: RefCell<Vec<Refusal>>,
    /// The units out on a raid of a tower, node or encounter site (`site.rs`).
    raiders: Vec<Serial>,
    /// The site each raider is set on, so a party keeps to it as it walks (`site.rs`).
    #[serde(default, serialize_with = "persist::sorted")]
    raid_targets: HashMap<Serial, Position>,
    /// When each plan began, for the time limit on all of them (`plan_clock.rs`).
    #[serde(default)]
    clock: plan_clock::PlanClock,
    /// Players who fight their own defence (a human): an attack on them is kept for
    /// [`Ai::take_attacks`] instead of being resolved.
    #[serde(skip)]
    defended_by: Vec<PlayerId>,
    #[serde(skip)]
    attacks: Vec<Attack>,
}

/// An attack on a player who defends in person: the attacking stack and the stack hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attack {
    pub attackers: Vec<hecs::Entity>,
    pub defenders: Vec<hecs::Entity>,
}

impl Ai {
    /// The AI's plans as text (RON on one line), to be kept in a save.
    pub fn to_text(&self) -> Result<String, ron::Error> {
        ron::ser::to_string(self)
    }

    /// An AI from the text `to_text` wrote.
    pub fn from_text(text: &str) -> Result<Self, ron::error::SpannedError> {
        battle::install();
        ron::de::from_str(text)
    }

    /// Put the plans of every AI in `ais` into `game`, to be saved with it.
    pub fn remember(game: &mut Game, ais: &[Ai]) {
        let plans = ais
            .iter()
            .filter_map(|ai| Some((ai.player, ai.to_text().ok()?)))
            .collect();
        game.set_plans(plans);
    }

    /// Give each AI in `ais` the plans `game` carries for its player. An AI the game has no plans
    /// for, or plans that cannot be read, starts afresh. Who defends in person (a setting of the
    /// seats, not a plan) is kept.
    pub fn recall(game: &Game, ais: &mut [Ai]) {
        for ai in ais {
            let saved = game
                .plans()
                .iter()
                .find(|(player, _)| *player == ai.player)
                .and_then(|(_, text)| Ai::from_text(text).ok());
            if let Some(mut saved) = saved {
                saved.defended_by = std::mem::take(&mut ai.defended_by);
                *ai = saved;
            }
        }
    }

    /// Whether the AI keeps out of this player's way, for being much stronger and near.
    pub(crate) fn is_appeasing(&self, other: PlayerId) -> bool {
        self.statecraft.appeased.contains(&other)
    }

    pub fn new(player: PlayerId) -> Self {
        battle::install();
        Self {
            player,
            settler_targets: HashMap::new(),
            campaign: None,
            settler_last: HashMap::new(),
            bad_sites: Vec::new(),
            scouts: Vec::new(),
            scout_targets: HashMap::new(),
            scout_last: HashMap::new(),
            bad_scout_targets: Vec::new(),
            frontier_closed: false,
            marching: Vec::new(),
            march_started: 0,
            march_target: None,
            naval: None,
            ferries: Vec::new(),
            naval_bad: Vec::new(),
            naval_wait: 0,
            sea_port: None,
            sea_scouting: false,
            sea_done: false,
            sea_party: Vec::new(),
            sea_gave_up: false,
            planar_tower: None,
            planar: None,
            crossing: None,
            colony: None,
            colonists: None,
            colony_since: 0,
            colony_bad: Vec::new(),
            colony_retry: 0,
            city_memory: HashMap::new(),
            threatened: Vec::new(),
            unreachable: Vec::new(),
            unreachable_since: 0,
            mastery_seen: HashMap::new(),
            racing: false,
            war_on: None,
            quarrel: None,
            statecraft: Default::default(),
            record_refusals: false,
            refusals: RefCell::new(Vec::new()),
            raiders: Vec::new(),
            raid_targets: HashMap::new(),
            clock: plan_clock::PlanClock::default(),
            defended_by: Vec::new(),
            attacks: Vec::new(),
        }
    }

    /// These players fight the battles against them themselves: the AI does not auto-resolve an
    /// attack on them but keeps it, for the caller to play out (see [`Ai::take_attacks`]).
    pub fn leave_defence_to(&mut self, players: &[PlayerId]) {
        self.defended_by = players.to_vec();
    }

    /// The attacks this turn on players who defend in person, oldest first.
    pub fn take_attacks(&mut self) -> Vec<Attack> {
        std::mem::take(&mut self.attacks)
    }

    /// Keep every command the game refuses, for [`Ai::take_refusals`].
    pub fn record_refusals(&mut self, on: bool) {
        self.record_refusals = on;
    }

    /// The refused commands kept since the last call.
    pub fn take_refusals(&self) -> Vec<Refusal> {
        std::mem::take(&mut *self.refusals.borrow_mut())
    }

    /// The enemy city this AI is campaigning against, if any (for tools that watch it).
    pub fn campaign(&self) -> Option<Position> {
        self.campaign
    }

    /// How many units are out scouting and where each is heading.
    pub fn scout_targets(&self) -> Vec<(u64, Position)> {
        self.scout_targets.iter().map(|(s, p)| (s.0, *p)).collect()
    }

    /// The landing the AI is preparing against a city on another land mass, as (target, port),
    /// and whether a ship is on a voyage.
    pub fn landing(&self) -> Option<(Position, Position, bool)> {
        self.naval
            .map(|p| (p.target, p.port, !self.ferries.is_empty()))
    }

    /// Whether scouting by land is over: nothing the AI can walk to shows anything new.
    pub fn frontier_closed(&self) -> bool {
        self.frontier_closed
    }

    /// What this wizard has to say to the human at the start of the human's turn: offers,
    /// warnings of war and requests (`diplomacy::approach_human`). The client calls it for each
    /// rival after the turn has changed, so the proposals wait for the human's answer in that turn.
    pub fn approach_human(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        diplomacy::approach_human(self, game, &mut events);
        events
    }

    /// Play this wizard's turn; returns the events the AI's own commands caused.
    pub fn play_turn(&mut self, game: &mut Game) -> Vec<Event> {
        // Every change in the game during the turn is a command this wizard sends, so what it
        // sees is counted once and patched (`Game::keep_for_turn`).
        game.keep_for_turn(self.player);
        let events = self.play_kept(game);
        game.drop_kept();
        events
    }

    fn play_kept(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        {
            let view = PlayerView::new(game, self.player);
            // A player plays when it has a wizard or is the neutral player of a race (task 522).
            let plays = view.wizard().is_some() || game.race_of_player(self.player).is_some();
            if view.game_over() || !plays || view.is_defeated() {
                return events;
            }
        }
        let dropped = self.drop_stale_plans(game.turn);
        if dropped.sea {
            colony::release_scouts(self, game, &mut events);
        }
        if !dropped.release.is_empty() {
            let units: Vec<hecs::Entity> = {
                let view = PlayerView::new(game, self.player);
                view.my_units()
                    .into_iter()
                    .filter(|&u| dropped.release.contains(&view.serial_of(u)))
                    .collect()
            };
            if !units.is_empty() {
                self.order(game, Command::ClearOrders { units }, &mut events);
            }
        }
        diplomacy::answer(self, game, &mut events);
        diplomacy::quarrel(self, game, &mut events);
        diplomacy::negotiate(self, game, &mut events);
        statecraft::run(self, game, &mut events);
        offer::answer(self, game, &mut events);
        offer::equip(self, game, &mut events);
        game.balance_farmers(self.player);
        economy::taxes(self, game, &mut events);
        alchemy::run(self, game, &mut events);
        race::run(self, game, &mut events);
        diplomacy::mastery_scare(self, game, &mut events);
        wizardry::run(self, game, &mut events);
        // While another wizard keeps Time Stop going, none of this wizard's units can act
        // (`AIMOVE.c`, `AIDUDES.c`): its armies, settlers and ships wait.
        if !game
            .time_stopper()
            .is_some_and(|keeper| keeper != self.player)
        {
            garrison::disperse(self, game, &mut events);
            expand::settlers(self, game, &mut events);
            scout::scout(self, game, &mut events);
            site::run(self, game, &mut events);
            military::army(self, game, &mut events);
            naval::run(self, game, &mut events);
            colony::run(self, game, &mut events);
        }
        raze::run(self, game, &mut events);
        economy::cities(self, game, &mut events);
        shortfall::run(self, game, &mut events);
        events
    }

    /// Keep a failure that is no refused command (a party that cannot be sent anywhere) in the
    /// refused-order log, with the line that asks.
    #[track_caller]
    pub(crate) fn note_failure(&self, game: &Game, what: String, reason: String) {
        if !self.record_refusals {
            return;
        }
        let at = std::panic::Location::caller();
        self.refusals.borrow_mut().push(Refusal {
            turn: game.turn,
            player: self.player,
            source: format!("{}:{}", at.file(), at.line()),
            command: what,
            reason,
        });
    }

    /// Whether `count` more units fit on the square (the limit of a square, `StackFull`): the check
    /// to make before an order to walk there, so that a full square is not asked again and again.
    pub(crate) fn room_at(game: &Game, to: Position, count: usize) -> bool {
        game.units().units_at(to).len() + count <= game.max_tile()
    }

    /// Send a command and keep its events.
    #[track_caller]
    fn send(
        &self,
        game: &mut Game,
        command: Command,
        events: &mut Vec<Event>,
    ) -> Result<(), SendError> {
        self.submit(game, command, events, true)
    }

    /// Send a command whose refusal changes nothing in the plan: the refusal is in the refused-order
    /// log (as for [`Ai::send`]), and the next turn's plan starts from where the units really
    /// are. For a command whose success the caller needs to know, use `send`.
    #[track_caller]
    fn order(&self, game: &mut Game, command: Command, events: &mut Vec<Event>) {
        let _ = self.submit(game, command, events, true);
    }

    /// Send a command that the game may refuse as the AI expects (it cannot know, say, that a site
    /// has no guards left): its refusal is not a bug, so the refused-order log does not keep it.
    #[track_caller]
    fn probe(
        &self,
        game: &mut Game,
        command: Command,
        events: &mut Vec<Event>,
    ) -> Result<(), SendError> {
        self.submit(game, command, events, false)
    }

    #[track_caller]
    fn submit(
        &self,
        game: &mut Game,
        command: Command,
        events: &mut Vec<Event>,
        keep_refusal: bool,
    ) -> Result<(), SendError> {
        if game.game_over.is_some() {
            // The other wizard's turn ended the game: nothing more can be ordered.
            return Err(SendError::GameOver);
        }
        let at = std::panic::Location::caller();
        let kept = (self.record_refusals && keep_refusal).then(|| format!("{command:?}"));
        let out = game.apply(command);
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
                // A standing order that stopped on a refused step is a refusal too.
                if let Event::OrderFailed { why, player, .. } = e {
                    if *player == self.player {
                        self.refusals.borrow_mut().push(Refusal {
                            turn: game.turn,
                            player: self.player,
                            source: format!("{}:{}", at.file(), at.line()),
                            command: command.clone(),
                            reason: format!("OrderFailed {why:?}"),
                        });
                    }
                }
                if let Event::Rejected(reason) = e {
                    self.refusals.borrow_mut().push(Refusal {
                        turn: game.turn,
                        player: self.player,
                        source: format!("{}:{}", at.file(), at.line()),
                        command: command.clone(),
                        reason: format!("{reason:?}"),
                    });
                }
            }
        }
        events.extend(out);
        refused.map_or(Ok(()), Err)
    }
}

/// Why a command the AI sent did not go through: the game was over already, a rule refused it
/// (the first refusal of its events), or a standing order was taken and then stopped on a refused
/// step at once (a full square, no room on the ship): the units stay where they are. The caller decides what that means for its plan: the next
/// candidate, a failure list, a wait, or [`Ai::note_failure`]; a caller that wants only yes or
/// no uses `.is_ok()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendError {
    GameOver,
    Rejected(f4x_core::game::Rejection),
    Stopped(f4x_core::unit::movement::MoveError),
}

impl SendError {
    /// The square the units were sent to has no room for them (a full stack, no room on the
    /// ship): sending more there this turn only adds refusals.
    pub(crate) fn is_no_room(self) -> bool {
        use f4x_core::game::Rejection;
        use f4x_core::unit::movement::MoveError;
        matches!(
            self,
            Self::Stopped(MoveError::StackFull | MoveError::NoRoom)
                | Self::Rejected(Rejection::Move(MoveError::StackFull | MoveError::NoRoom))
        )
    }
}

/// The wizard answers the proposals made to it now (a human who has put a proposal waits for
/// the answer on the diplomacy screen instead of until the turn ends). Returns the events.
pub fn answer_proposals(game: &mut Game, player: PlayerId) -> Vec<Event> {
    let mut events = Vec::new();
    diplomacy::answer(&Ai::new(player), game, &mut events);
    events
}

pub use offer::answer_resales;

pub use controller::{play_round, AiSeat, NeutralSeat};

/// A tuning number that an experiment may set from the environment (`AIK_<NAME>`).
pub(crate) fn knob(name: &str, default: f32) -> f32 {
    std::env::var(format!("AIK_{name}"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod send_tests {
    use super::*;
    use crate::testworld::{at, game, spawn, COAST_CITY, P0, P1};
    use f4x_core::unit::movement::MoveError;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_command_that_goes_through_is_ok_and_a_refused_one_says_which_rule() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 1);
        let mut events = Vec::new();
        let go = Command::GoTo {
            units: units.clone(),
            to: at(5, 9),
        };
        assert_eq!(Ai::new(P0).send(&mut g, go.clone(), &mut events), Ok(()));
        // Another wizard's units are not ours to order.
        let refused = Ai::new(P1).send(&mut g, go, &mut events);
        assert!(
            matches!(refused, Err(SendError::Rejected(_))),
            "{refused:?}"
        );
    }

    #[test]
    fn an_order_that_stops_on_its_first_step_is_an_error_with_the_reason() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 1);
        // The square next to it is full of its own units.
        spawn(&mut g, P0, at(5, 9), 113, f4x_core::unit::MAX_TILE_LIMIT);
        let mut events = Vec::new();
        let sent = Ai::new(P0).send(
            &mut g,
            Command::GoTo {
                units,
                to: at(5, 9),
            },
            &mut events,
        );
        assert_eq!(sent, Err(SendError::Stopped(MoveError::StackFull)));
        Ok(())
    }

    #[test]
    fn only_a_full_square_or_ship_counts_as_no_room() {
        use f4x_core::game::Rejection;
        assert!(SendError::Stopped(MoveError::StackFull).is_no_room());
        assert!(SendError::Stopped(MoveError::NoRoom).is_no_room());
        assert!(SendError::Rejected(Rejection::Move(MoveError::StackFull)).is_no_room());
        assert!(!SendError::Stopped(MoveError::Impassable).is_no_room());
        assert!(!SendError::GameOver.is_no_room());
    }

    #[test]
    fn nothing_can_be_sent_once_the_game_is_over() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.game_over = Some(f4x_core::game::endgame::GameOver {
            winner: P0,
            kind: f4x_core::game::endgame::EndKind::SpellOfMastery,
            turn: 1,
        });
        let mut events = Vec::new();
        assert_eq!(
            Ai::new(P0).send(&mut g, Command::EndTurn, &mut events),
            Err(SendError::GameOver)
        );
    }
}
