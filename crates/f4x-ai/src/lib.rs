//! AI players: they look at the game the way a player does and answer with `Command`s sent
//! through `Game::apply`. Nothing here reaches into the game's internals.
//!
//! One [`Ai`] plays one wizard. [`Ai::play_turn`] does that wizard's whole turn (research,
//! spells, city production, settlers, the army) and leaves ending the turn to the caller,
//! because a turn ends for every wizard at once; [`play_round`] does that for a list of AIs.
//! The first version is deliberately simple: it expands, builds, defends and attacks cities it
//! has found, and is meant to be good enough to play whole games, not to play them well
//! (tasks 64 to 67 add personalities, spell use and real campaign planning).

mod diplomacy;
mod economy;
mod expand;
mod military;
mod naval;
mod neutral;
mod objective;
mod offer;
mod race;
mod raze;
mod scout;
mod shortfall;
mod site;
#[cfg(test)]
mod testworld;
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
#[derive(Debug, Clone)]
pub struct Ai {
    pub player: PlayerId,
    /// Where each settler is heading to found a city.
    settler_targets: HashMap<Serial, Position>,
    /// The enemy city the army is gathering to attack, once it has chosen one.
    campaign: Option<Position>,
    /// Where each settler stood last turn, to notice one that is stuck.
    settler_last: HashMap<Serial, Position>,
    /// Sites the AI gave up on.
    bad_sites: Vec<Position>,
    /// The units set to find the rest of the world, and where each is heading.
    scouts: Vec<Serial>,
    scout_targets: HashMap<Serial, Position>,
    /// Where each scout stood last turn and for how many turns it has not moved.
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
    ferry: Option<naval::Ferry>,
    naval_bad: Vec<Position>,
    /// Turns a loaded ship has waited for a stronger landing party.
    naval_wait: u32,
    /// Looking for the rest of the world by ship: the city that builds it, whether a ship is out
    /// scouting, and whether that is over.
    sea_port: Option<Position>,
    sea_scouting: bool,
    sea_done: bool,
    /// The strength last seen in each enemy city and the turn it was seen.
    city_memory: HashMap<Position, (f32, u32)>,
    /// Enemy cities no march found a way to, and when the last was added.
    unreachable: Vec<Position>,
    unreachable_since: u32,
    /// No reachable square shows anything new: scouting by land is over.
    frontier_closed: bool,
    /// Whether refused commands are kept (see [`Ai::take_refusals`]) and the ones kept so far.
    /// What was last seen of each rival casting the Spell of Mastery (`race.rs`).
    mastery_seen: HashMap<PlayerId, race::Sighting>,
    /// The AI means to win the race for the Spell of Mastery, or has declared war on the wizard
    /// it cannot beat to it.
    racing: bool,
    war_on: Option<PlayerId>,
    /// The wizard this one went to war on by temper (`diplomacy::quarrel`).
    quarrel: Option<PlayerId>,
    record_refusals: bool,
    refusals: RefCell<Vec<Refusal>>,
    /// The units out on a raid of a tower, node or encounter site (`site.rs`).
    raiders: Vec<Serial>,
    /// Players who fight their own defence (a human): an attack on them is kept for
    /// [`Ai::take_attacks`] instead of being resolved.
    defended_by: Vec<PlayerId>,
    attacks: Vec<Attack>,
}

/// An attack on a player who defends in person: the attacking stack and the stack hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attack {
    pub attackers: Vec<hecs::Entity>,
    pub defenders: Vec<hecs::Entity>,
}

impl Ai {
    pub fn new(player: PlayerId) -> Self {
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
            ferry: None,
            naval_bad: Vec::new(),
            naval_wait: 0,
            sea_port: None,
            sea_scouting: false,
            sea_done: false,
            city_memory: HashMap::new(),
            unreachable: Vec::new(),
            unreachable_since: 0,
            mastery_seen: HashMap::new(),
            racing: false,
            war_on: None,
            quarrel: None,
            record_refusals: false,
            refusals: RefCell::new(Vec::new()),
            raiders: Vec::new(),
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
        self.naval.map(|p| (p.target, p.port, self.ferry.is_some()))
    }

    /// Whether scouting by land is over: nothing the AI can walk to shows anything new.
    pub fn frontier_closed(&self) -> bool {
        self.frontier_closed
    }

    /// Play this wizard's turn; returns the events the AI's own commands caused.
    pub fn play_turn(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        {
            let view = PlayerView::new(game, self.player);
            if view.game_over() || view.wizard().is_none() || view.is_defeated() {
                return events;
            }
        }
        diplomacy::answer(self, game, &mut events);
        diplomacy::quarrel(self, game, &mut events);
        offer::answer(self, game, &mut events);
        offer::equip(self, game, &mut events);
        game.balance_farmers(self.player);
        economy::taxes(self, game, &mut events);
        race::run(self, game, &mut events);
        diplomacy::mastery_scare(self, game, &mut events);
        wizardry::run(self, game, &mut events);
        // While another wizard keeps Time Stop going, none of this wizard's units can act
        // (`AIMOVE.c`, `AIDUDES.c`): its armies, settlers and ships wait.
        if !game
            .time_stopper()
            .is_some_and(|keeper| keeper != self.player)
        {
            expand::settlers(self, game, &mut events);
            scout::scout(self, game, &mut events);
            site::run(self, game, &mut events);
            military::army(self, game, &mut events);
            naval::run(self, game, &mut events);
        }
        raze::run(self, game, &mut events);
        economy::cities(self, game, &mut events);
        shortfall::run(self, game, &mut events);
        events
    }

    /// Send a command and keep its events.
    #[track_caller]
    fn send(&self, game: &mut Game, command: Command, events: &mut Vec<Event>) -> bool {
        if game.game_over.is_some() {
            // The other wizard's turn ended the game: nothing more can be ordered.
            return false;
        }
        let at = std::panic::Location::caller();
        let kept = self.record_refusals.then(|| format!("{command:?}"));
        let out = game.apply(command);
        let ok = !out.iter().any(|e| matches!(e, Event::Rejected(_)));
        if let Some(command) = kept {
            for e in &out {
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
        ok
    }
}

/// The wizard answers the proposals made to it now (a human who has put a proposal waits for
/// the answer on the diplomacy screen instead of until the turn ends). Returns the events.
pub fn answer_proposals(game: &mut Game, player: PlayerId) -> Vec<Event> {
    let mut events = Vec::new();
    diplomacy::answer(&Ai::new(player), game, &mut events);
    events
}

pub use neutral::play_neutrals;
pub use offer::answer_resales;

/// Let every AI play its turn, then end the turn for everyone. Returns all the events.
pub fn play_round(game: &mut Game, ais: &mut [Ai]) -> Vec<Event> {
    let mut events = Vec::new();
    for ai in ais.iter_mut() {
        events.extend(ai.play_turn(game));
    }
    events.extend(play_neutrals(game));
    if game.game_over.is_none() {
        events.extend(game.apply(Command::EndTurn));
    }
    events
}

/// A tuning number that an experiment may set from the environment (`AIK_<NAME>`).
pub(crate) fn knob(name: &str, default: f32) -> f32 {
    std::env::var(format!("AIK_{name}"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
