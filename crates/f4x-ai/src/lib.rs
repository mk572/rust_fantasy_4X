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
mod bridge;
mod budget;
mod builder;
mod city_site;
mod controller;
mod craft;
mod diplo_b;
mod diplo_b_stance;
mod diplo_b_talk;
mod diplomacy;
mod dispatch;
mod economy;
mod enchant_road;
mod escort;
mod expand;
mod forge;
mod garrison;
mod intent;
mod meld;
mod memory;
mod military;
mod monster;
mod naval;
mod objective;
mod offer;
mod opinion;
mod perimeter;
mod persist;
mod phase;
mod pickup;
mod plan_clock;
mod race;
mod raid;
mod raze;
mod recover;
mod reinforce;
mod road;
pub mod roles;
mod scout;
mod settler_fate;
mod shortfall;
mod site;
mod spellhelper;
mod statecraft;
mod strike;
#[cfg(test)]
mod testworld;
mod trade;
mod view;
mod war;
mod war_plan;
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

/// The refusal's reason and the facts of the failure, in one string: the soak's refused-order log
/// splits it at [`DETAIL_SEPARATOR`] into the reason and detail columns.
pub const DETAIL_SEPARATOR: &str = " | ";

fn with_detail(reason: String, detail: Option<String>) -> String {
    match detail {
        Some(d) => format!("{reason}{DETAIL_SEPARATOR}{d}"),
        None => reason,
    }
}

thread_local! {
    /// The last command any wizard of this thread sent, for the report of a panic.
    static LAST_ORDER: RefCell<Option<Command>> = const { RefCell::new(None) };
}

/// The last command sent on this thread (`Debug` form), to say which order a panic came with.
pub fn last_order() -> Option<String> {
    LAST_ORDER.with(|l| l.borrow().as_ref().map(|c| format!("{c:?}")))
}

/// What stopped a standing order on its step: the square it could not enter and what is there.
fn step_detail(game: &Game, event: &Event) -> Option<String> {
    let Event::OrderFailed { units, to, .. } = event else {
        return None;
    };
    let (x, y) = (to.x, to.y);
    Some(match game.blocker_at(units, *to) {
        Some(why) => format!("step ({x}, {y}): {why}"),
        None => format!("step ({x}, {y}): free now"),
    })
}

/// What a command that moves or fights is aimed at, for the facts of its refusal.
enum Aim {
    /// A stack sent to a square.
    Walk(Vec<hecs::Entity>, Position),
    /// A battle between two stacks.
    Fight(Vec<hecs::Entity>, Vec<hecs::Entity>),
}

/// One side of a battle for the log: how many units, whose, and where they stand.
fn side_detail(game: &Game, units: &[hecs::Entity]) -> String {
    if units.is_empty() {
        return "no units".to_string();
    }
    let mut owners: Vec<u32> = Vec::new();
    let mut squares: Vec<String> = Vec::new();
    for &u in units {
        let owner = game.units().unit_owner(u).map_or(u32::MAX, |p| p.0.into());
        if !owners.contains(&owner) {
            owners.push(owner);
        }
        let square = game
            .units()
            .unit_position(u)
            .map_or("nowhere".to_string(), |p| {
                format!("({}, {}) {:?}", p.x, p.y, p.plane)
            });
        if !squares.contains(&square) {
            squares.push(square);
        }
    }
    format!(
        "{} units of players {owners:?} on {}",
        units.len(),
        squares.join(" and ")
    )
}

/// The facts of a refused order: why a go-to found no way, what blocks the square of a refused
/// move, which units had no moves left, or how the sides of a refused battle stood.
fn rejection_detail(
    game: &Game,
    why: &f4x_core::game::Rejection,
    aim: Option<&Aim>,
) -> Option<String> {
    use f4x_core::game::Rejection;
    use f4x_core::unit::movement::MoveError;
    let walk = match aim {
        Some(Aim::Walk(units, to)) => Some((units, to)),
        _ => None,
    };
    match why {
        Rejection::BattleStart(_) => {
            let Some(Aim::Fight(attackers, defenders)) = aim else {
                return None;
            };
            Some(format!(
                "attackers: {}; defenders: {}",
                side_detail(game, attackers),
                side_detail(game, defenders)
            ))
        }
        Rejection::Move(MoveError::NoMovesLeft) => {
            let (units, to) = walk?;
            let spent: Vec<String> = units
                .iter()
                .filter(|&&u| game.units().unit_moves_left(u) == 0)
                .map(|&u| {
                    game.units()
                        .unit_def(u)
                        .map_or("?".to_string(), |d| d.num.to_string())
                })
                .collect();
            Some(format!(
                "to ({}, {}): {} of {} units have no moves left (kinds {}); a Wind Walker's allowance counts for the stack",
                to.x,
                to.y,
                spent.len(),
                units.len(),
                spent.join(",")
            ))
        }
        Rejection::NoPath(no_path) => {
            let (units, to) = walk?;
            // The kinds of unit in the stack (their content numbers): one that cannot go where the
            // others can is a common cause.
            let kinds: Vec<String> = units
                .iter()
                .map(|&u| {
                    game.units()
                        .unit_def(u)
                        .map_or("?".to_string(), |d| d.num.to_string())
                })
                .collect();
            Some(format!(
                "to ({}, {}): {no_path}; units {}",
                to.x,
                to.y,
                kinds.join(",")
            ))
        }
        Rejection::Move(MoveError::Impassable | MoveError::StackFull | MoveError::NoRoom) => {
            let (units, to) = walk?;
            let why = game.blocker_at(units, *to)?;
            Some(format!("to ({}, {}): {why}", to.x, to.y))
        }
        _ => None,
    }
}

/// One computer-played wizard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ai {
    pub player: PlayerId,
    /// The enemy city the army is gathering to attack, once it has chosen one.
    campaign: Option<Position>,
    /// The ranked places for the next cities (`city_site.rs`).
    #[serde(default)]
    sites: city_site::SiteBook,
    /// Why settlers' plans ended without a city, for the soak (`settler_fate.rs`).
    #[serde(default)]
    settler_fates: settler_fate::SettlerFates,
    /// Targets scouts could not get to.
    bad_scout_targets: Vec<Position>,
    /// The turn the units on the march against `march_target` set out.
    march_started: u32,
    march_target: Option<Position>,
    /// The landing the AI is working towards when the enemy lives on another land mass, the ship
    /// on a voyage, and ports or targets that did not work out.
    naval: Option<naval::Plan>,
    naval_bad: Vec<Position>,
    /// Turns a loaded ship has waited for a stronger landing party.
    naval_wait: u32,
    /// Looking for the rest of the world by ship: the city that builds it, whether a ship is out
    /// scouting, and whether that is over.
    sea_port: Option<Position>,
    sea_scouting: bool,
    sea_done: bool,
    /// The sea scouting was ended by the plan clock (`plan_clock.rs`), not by finding nothing:
    /// it is tried again when the failure lists are forgiven.
    #[serde(default)]
    sea_gave_up: bool,
    /// The open tower the planar scout is bound for, to be set aside when it does not get there.
    #[serde(default)]
    planar_tower: Option<Position>,
    /// The planes where a rival's city blocks the clearing of towers this turn (`raid.rs`).
    #[serde(skip)]
    tower_blocked: Vec<f4x_core::map::Plane>,
    /// The sites a raiding party lost its assault on, and the strength (with the wizard's spells)
    /// of the party that lost: the estimate of the guards was wrong for them (guards that weapons
    /// cannot hurt, say), so only a party much stronger goes again (`raid.rs`).
    #[serde(default)]
    site_losses: Vec<(Position, f32)>,
    /// The open tower an army crosses by to reach a rival's city on the other plane (`military.rs`).
    #[serde(default)]
    crossing: Option<Position>,
    /// Own cities the engineers found no road to build at, and how many cities there were then
    /// (`road.rs`).
    #[serde(default)]
    road_checked: Vec<f4x_core::game::serial::Serial>,
    #[serde(default)]
    road_cities: usize,
    /// Own cities with a stack of a wizard at war near and too few defenders, this turn.
    threatened: Vec<Position>,
    /// The strength last seen in each enemy city and the turn it was seen.
    #[serde(serialize_with = "persist::sorted")]
    city_memory: HashMap<Position, (f32, u32)>,
    /// The war plan (task 648): the goal of the war, the next target and the towns to look at.
    #[serde(default)]
    war_plan: war_plan::WarPlan,
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
    /// What it remembers of its dealings with the others (`memory.rs`, task 760); shared with the
    /// game's [`Diplomats`] table when the host joined it.
    #[serde(default, with = "memory::shared")]
    memory: memory::Shared,
    /// The game's table of memories, for answering a proposal made to another wizard.
    #[serde(skip)]
    diplomats: Diplomats,
    /// Units whose mission the dispatcher found over this turn (`dispatch::revalidate`); counted
    /// in the audit.
    #[serde(skip)]
    missions_reset: u32,
    #[serde(skip)]
    record_refusals: bool,
    #[serde(skip)]
    refusals: RefCell<Vec<Refusal>>,
    /// The walking orders this turn that the game stopped for lack of room on a square (a full
    /// stack): `dispatch::run` sends them again once the other jobs have moved their units
    /// (`Ai::retry_blocked`).
    #[serde(skip)]
    blocked: RefCell<Vec<Command>>,
    /// Set while the blocked orders are being sent again, so that one still blocked is not kept twice.
    #[serde(skip)]
    retrying: std::cell::Cell<bool>,
    /// The one place a unit's role lives (`roles.rs`). It holds each unit's role, its
    /// target and, for a unit that cannot walk there, what it needs (`Crossing`).
    #[serde(default)]
    roles: roles::Registry,
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
                // The table keeps the handle it has; the memory goes into it.
                let memory = saved.memory.get().clone();
                ai.memory.replace(memory);
                saved.memory = std::mem::take(&mut ai.memory);
                saved.diplomats = std::mem::take(&mut ai.diplomats);
                *ai = saved;
            }
        }
    }

    /// The wizard as it answers a proposal made to it: a new one with the memory the table holds
    /// of the real wizard.
    fn answering(player: PlayerId, diplomats: &Diplomats) -> Self {
        let mut ai = Ai::new(player);
        ai.memory.replace(diplomats.of(player));
        ai.diplomats = diplomats.clone();
        ai
    }

    /// The memories of every wizard of the game, this one's among them (a host that did not join a table
    /// has only this one's): what a trade needs to weigh the other side's answer too.
    pub(crate) fn minds(&self) -> Diplomats {
        self.diplomats.add_if_absent(self.player, &self.memory);
        self.diplomats.clone()
    }

    /// Join the game's table of memories: the wizard's memory is shared with it, so a proposal made to
    /// the wizard is answered by what it remembers (`memory.rs`).
    pub fn join(&mut self, diplomats: &Diplomats) {
        diplomats.add(self.player, &self.memory);
        self.diplomats = diplomats.clone();
    }

    /// The diplomacy system and, for B, the main enemy and what it has seen of the others, for the soak's
    /// detail lines.
    pub fn diplomacy_note(&self, game: &Game) -> String {
        let m = self.memory.get();
        let enemy = m.enemy.map_or("-".to_string(), |(p, since)| {
            format!("p{} since {since}", p.0)
        });
        let seen: Vec<String> = m
            .acts
            .iter()
            .map(|a| {
                format!(
                    "p{} razed {} wars {} broken {} ({})",
                    a.who.0,
                    a.razed,
                    a.wars,
                    a.broken,
                    m.ruthlessness(a.who, game.turn, diplo_b::Knobs::current().ruth_half)
                )
            })
            .collect();
        let weary: Vec<String> = m
            .dealings
            .iter()
            .filter(|d| game.treaty(self.player, d.with) == f4x_core::diplomacy::Treaty::War)
            .map(|d| format!("p{} {}", d.with.0, d.weary))
            .collect();
        format!(
            "diplomacy B, enemy {enemy}, weary [{}], seen [{}]",
            weary.join(", "),
            seen.join("; ")
        )
    }

    /// Tell the wizard what has happened (the events of a turn): the battles it fought and the
    /// cities it won or lost go into its memory. What a human would be told of, and no more.
    pub fn notice(&mut self, game: &Game, events: &[Event]) {
        self.memory.put().notice(self.player, game, events);
    }

    pub fn new(player: PlayerId) -> Self {
        battle::install();
        Self {
            player,
            campaign: None,
            sites: city_site::SiteBook::default(),
            settler_fates: settler_fate::SettlerFates::default(),
            bad_scout_targets: Vec::new(),
            frontier_closed: false,
            march_started: 0,
            march_target: None,
            naval: None,
            naval_bad: Vec::new(),
            naval_wait: 0,
            sea_port: None,
            sea_scouting: false,
            sea_done: false,
            sea_gave_up: false,
            planar_tower: None,
            tower_blocked: Vec::new(),
            crossing: None,
            site_losses: Vec::new(),
            road_checked: Vec::new(),
            road_cities: 0,
            city_memory: HashMap::new(),
            threatened: Vec::new(),
            war_plan: war_plan::WarPlan::default(),
            unreachable: Vec::new(),
            unreachable_since: 0,
            mastery_seen: HashMap::new(),
            racing: false,
            war_on: None,
            memory: Default::default(),
            diplomats: Diplomats::default(),
            missions_reset: 0,
            record_refusals: false,
            refusals: RefCell::new(Vec::new()),
            blocked: RefCell::new(Vec::new()),
            retrying: std::cell::Cell::new(false),
            roles: roles::Registry::default(),
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
        self.roles
            .targeted(roles::Role::Scout)
            .into_iter()
            .map(|(s, p)| (s.0, p))
            .collect()
    }

    /// The landing the AI is preparing against a city on another land mass, as (target, port),
    /// and whether a ship is on a voyage.
    pub fn landing(&self) -> Option<(Position, Position, bool)> {
        self.naval
            .map(|p| (p.target, p.port, !pickup::voyages(self).is_empty()))
    }

    /// Whether scouting by land is over: nothing the AI can walk to shows anything new.
    /// Why settlers' plans have ended without a city, by reason (`settler_fate.rs`), for the
    /// soak's summary.
    pub fn settler_failures(&self) -> Vec<(&'static str, u32)> {
        self.settler_fates.counts()
    }

    pub fn frontier_closed(&self) -> bool {
        self.frontier_closed
    }

    /// What this wizard has to say to the human at the start of the human's turn: offers,
    /// warnings of war and requests (`diplomacy::approach_human`). The client calls it for each
    /// rival after the turn has changed, so the proposals wait for the human's answer in that turn.
    pub fn approach_human(&mut self, game: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        diplo_b_talk::approach_human(self, game, &mut events);
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
        // Units built or hired since the last turn await orders from the first moment.
        roles::register(self, game);
        let dropped = self.drop_stale_plans(game.turn);
        if dropped.sea {
            naval::release_scouts(self, game, &mut events);
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
        self.memory.put().look(game, self.player);
        diplomacy::answer(self, game, &mut events);
        diplo_b_talk::negotiate(self, game, &mut events);
        statecraft::run(self, game, &mut events);
        offer::answer(self, game, &mut events);
        offer::equip(self, game, &mut events);
        game.balance_farmers(self.player);
        economy::taxes(self, game, &mut events);
        alchemy::run(self, game, &mut events);
        race::run(self, game, &mut events);
        self.tower_blocked = site::blocked_planes(
            &PlayerView::new(game, self.player),
            &self.unreachable,
            self.naval.map(|p| p.target),
        );
        wizardry::run(self, game, &mut events);
        // While another wizard keeps Time Stop going, none of this wizard's units can act
        // (`AIMOVE.c`, `AIDUDES.c`): its armies, settlers and ships wait.
        if !game
            .time_stopper()
            .is_some_and(|keeper| keeper != self.player)
        {
            dispatch::run(self, game, &mut events);
        }
        raze::run(self, game, &mut events);
        economy::cities(self, game, &mut events);
        road::build_engineer(self, game, &mut events);
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
    /// to make before an order to walk there, so that a full square is not asked again and again. A
    /// tower is one square on both planes: the units on its other side count too (`army_at`).
    pub(crate) fn room_at(game: &Game, to: Position, count: usize) -> bool {
        game.economy().army_at(to).len() + count <= game.max_tile()
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
        LAST_ORDER.with(|l| *l.borrow_mut() = Some(command.clone()));
        let target = match &command {
            Command::GoTo { units, to } | Command::MoveStack { units, to } => {
                Some(Aim::Walk(units.clone(), *to))
            }
            Command::AutoResolve {
                attackers,
                defenders,
                ..
            }
            | Command::BeginBattle {
                attackers,
                defenders,
            } => Some(Aim::Fight(attackers.clone(), defenders.clone())),
            _ => None,
        };
        let walk_command = matches!(command, Command::GoTo { .. } | Command::MoveStack { .. })
            .then(|| command.clone());
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
                            reason: with_detail(
                                format!("OrderFailed {why:?}"),
                                step_detail(game, e),
                            ),
                        });
                    }
                }
                if let Event::Rejected(reason) = e {
                    self.refusals.borrow_mut().push(Refusal {
                        turn: game.turn,
                        player: self.player,
                        source: format!("{}:{}", at.file(), at.line()),
                        command: command.clone(),
                        reason: with_detail(
                            match reason {
                                f4x_core::game::Rejection::NoPath(n) => {
                                    format!("NoPath({})", n.name())
                                }
                                other => format!("{other:?}"),
                            },
                            rejection_detail(game, reason, target.as_ref()),
                        ),
                    });
                }
            }
        }
        events.extend(out);
        if let (Some(why), Some(Aim::Walk(..)), false) = (refused, &target, self.retrying.get()) {
            if why.is_no_room() {
                self.blocked
                    .borrow_mut()
                    .push(walk_command.expect("a walking order"));
            }
        }
        refused.map_or(Ok(()), Err)
    }

    /// Send again the walking orders the game stopped for lack of room, now that the other jobs
    /// have moved their units: the units that could move did, and a square they left has room for
    /// those that waited. Unblocked first, then the blocked, as long as one more gets through, so a
    /// column of big stacks walking one way clears from its head.
    pub(crate) fn retry_blocked(&self, game: &mut Game, events: &mut Vec<Event>) {
        let mut waiting = std::mem::take(&mut *self.blocked.borrow_mut());
        self.retrying.set(true);
        while !waiting.is_empty() {
            let mut still = Vec::new();
            let mut sent = false;
            for command in waiting {
                match self.submit(game, command.clone(), events, false) {
                    Err(why) if why.is_no_room() => still.push(command),
                    Ok(()) => sent = true,
                    // Refused for another reason (the units have no moves left, say): over.
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
///
/// The wizard answers with what it remembers: `diplomats` is the game's table of memories (`memory.rs`);
/// a wizard that is not in it answers with an empty memory.
pub fn answer_proposals(game: &mut Game, player: PlayerId, diplomats: &Diplomats) -> Vec<Event> {
    let mut events = Vec::new();
    diplomacy::answer(&Ai::answering(player, diplomats), game, &mut events);
    events
}

/// Two AI wizards answer each other's proposals and counters until nothing is pending between
/// them (a negotiation has at most `max_rounds` messages). `from` made the proposal to `to`.
pub fn settle(game: &mut Game, from: PlayerId, to: PlayerId, diplomats: &Diplomats) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..=game.content().rules.config.diplomacy.max_rounds {
        let mut answered = false;
        for who in [to, from] {
            let waiting = game.human() != Some(who)
                && game
                    .diplomacy
                    .proposals()
                    .iter()
                    .any(|p| p.to == who && (p.from == from || p.from == to));
            if waiting {
                events.extend(answer_proposals(game, who, diplomats));
                answered = true;
            }
        }
        if !answered {
            break;
        }
    }
    events
}

pub use offer::answer_resales;

pub use controller::{play_round, AiSeat, NeutralSeat};
pub use memory::Diplomats;

/// A tuning number that an experiment may set from the environment (`AIK_<NAME>`).
pub(crate) fn knob(name: &str, default: f32) -> f32 {
    std::env::var(format!("AIK_{name}"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    const ME: PlayerId = crate::testworld::P0;
    const FOE: PlayerId = crate::testworld::P1;

    fn lost_a_battle(ai: &mut Ai, turn: u32) {
        let mut g = crate::testworld::game(crate::testworld::COAST_CITY, 8 /* high_men */);
        g.turn = turn;
        ai.notice(
            &g,
            &[Event::BattleEnded {
                winner: FOE,
                loser: ME,
            }],
        );
    }

    fn battles_lost(m: &memory::Memory) -> Option<u32> {
        m.with(FOE).map(|d| d.battles_lost)
    }

    #[test]
    fn a_wizard_hears_of_a_battle_it_lost_and_the_table_shares_what_it_remembers() {
        let table = Diplomats::new();
        let mut ai = Ai::new(ME);
        ai.join(&table);
        lost_a_battle(&mut ai, 4);
        assert_eq!(battles_lost(&table.of(ME)), Some(1));
        // A proposal made to it is answered by the memory of the real wizard, not an empty one.
        let judge = Ai::answering(ME, &table);
        assert_eq!(battles_lost(&judge.memory.get()), Some(1));
        // Without the table the answer is made with an empty memory.
        let stranger = Ai::answering(ME, &Diplomats::new());
        assert_eq!(battles_lost(&stranger.memory.get()), None);
    }

    #[test]
    fn the_memory_is_saved_with_the_plans_and_comes_back_into_the_table(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let table = Diplomats::new();
        let mut ai = Ai::new(ME);
        ai.join(&table);
        lost_a_battle(&mut ai, 9);
        let text = ai.to_text()?;
        // A game is loaded: a new AI for the wizard recalls the plans; the table keeps its handle.
        let mut fresh = vec![Ai::new(ME)];
        fresh[0].join(&table);
        let mut g = crate::testworld::game(crate::testworld::COAST_CITY, 8 /* high_men */);
        g.set_plans(vec![(ME, text)]);
        Ai::recall(&g, &mut fresh);
        assert_eq!(battles_lost(&fresh[0].memory.get()), Some(1));
        assert_eq!(battles_lost(&table.of(ME)), Some(1));
        Ok(())
    }

    #[test]
    fn a_copy_of_a_wizard_remembers_on_its_own() {
        let mut ai = Ai::new(ME);
        let copy = ai.clone();
        lost_a_battle(&mut ai, 2);
        assert_eq!(battles_lost(&copy.memory.get()), None);
    }
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
        let limit = g.max_tile();
        spawn(&mut g, P0, at(5, 9), 113, limit);
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
    fn an_order_stopped_by_a_full_square_goes_again_once_the_stack_on_it_has_moved_on() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let rear = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 1);
        let limit = g.max_tile();
        let head = spawn(&mut g, P0, at(5, 9), 113, limit);
        let ai = Ai::new(P0);
        let mut events = Vec::new();
        // The rear stack is sent first and finds the square full; the head stack then moves on.
        let first = ai.send(
            &mut g,
            Command::GoTo {
                units: rear.clone(),
                to: at(5, 9),
            },
            &mut events,
        );
        assert_eq!(first, Err(SendError::Stopped(MoveError::StackFull)));
        assert_eq!(g.units().unit_position(rear[0]), Some(at(5, 8)));
        ai.order(
            &mut g,
            Command::GoTo {
                units: head.clone(),
                to: at(5, 10),
            },
            &mut events,
        );
        assert_eq!(g.units().unit_position(head[0]), Some(at(5, 10)));
        ai.retry_blocked(&mut g, &mut events);
        assert_eq!(g.units().unit_position(rear[0]), Some(at(5, 9)));
        // What went through is not kept to be sent again.
        let before = g.units().unit_position(rear[0]);
        ai.retry_blocked(&mut g, &mut events);
        assert_eq!(g.units().unit_position(rear[0]), before);
    }

    #[test]
    fn a_move_with_no_moves_left_names_the_units_that_have_none() -> TestResult {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 1);
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let mut events = Vec::new();
        // Walk until the moves are spent, then ask for one more step.
        let mut y = 8;
        while g.units().unit_moves_left(units[0]) != 0 {
            y += 1;
            ai.order(
                &mut g,
                Command::MoveStack {
                    units: units.clone(),
                    to: at(5, y),
                },
                &mut events,
            );
            assert!(y < 20, "the moves never ran out");
        }
        ai.take_refusals();
        let sent = ai.send(
            &mut g,
            Command::MoveStack {
                units,
                to: at(5, y + 1),
            },
            &mut events,
        );
        assert_eq!(
            sent,
            Err(SendError::Rejected(f4x_core::game::Rejection::Move(
                MoveError::NoMovesLeft
            )))
        );
        let reasons: Vec<String> = ai.take_refusals().into_iter().map(|r| r.reason).collect();
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("1 of 1 units have no moves left (kinds 113)")),
            "{reasons:?}"
        );
        Ok(())
    }

    #[test]
    fn a_battle_with_bad_sides_says_who_stood_where() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let ours = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 2);
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let mut events = Vec::new();
        // The defenders are nobody at all.
        let _ = ai.send(
            &mut g,
            Command::AutoResolve {
                attackers: ours,
                defenders: Vec::new(),
                mana_budget: None,
            },
            &mut events,
        );
        let reasons: Vec<String> = ai.take_refusals().into_iter().map(|r| r.reason).collect();
        assert!(
            reasons.iter().any(
                |r| r.contains("attackers: 2 units of players [0] on (5, 8)")
                    && r.contains("defenders: no units")
            ),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_march_with_no_room_says_how_full_the_squares_beside_the_site_are() {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let party = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 2);
        let limit = g.max_tile();
        spawn(&mut g, P0, at(5, 10), 113, limit);
        let said = crate::assault::march_detail(&Ai::new(P0), &g, &party, at(5, 11));
        assert!(
            said.contains(&format!("(5, 10) holds {limit}")) && said.starts_with("party of 2"),
            "{said}"
        );
    }

    #[test]
    fn a_refused_order_logs_the_facts_of_the_failure() -> TestResult {
        use crate::testworld::ENEMY_CITY;
        let mut g = game(COAST_CITY, 8 /* high_men */);
        let units = spawn(&mut g, P0, at(5, 8), 113 /* high_men_paladins */, 1);
        let limit = g.max_tile();
        spawn(&mut g, P0, at(5, 9), 113, limit);
        let mut ai = Ai::new(P0);
        ai.record_refusals(true);
        let mut events = Vec::new();
        // A full square on the way: the order is taken and stops at its step.
        let _ = ai.send(
            &mut g,
            Command::GoTo {
                units: units.clone(),
                to: at(5, 9),
            },
            &mut events,
        );
        // The other island: no walk, and the log says where the walk stopped.
        let _ = ai.send(
            &mut g,
            Command::GoTo {
                units,
                to: at(ENEMY_CITY.0, ENEMY_CITY.1),
            },
            &mut events,
        );
        let reasons: Vec<String> = ai.take_refusals().into_iter().map(|r| r.reason).collect();
        assert!(
            reasons.iter().any(|r| r.contains(&format!(
                "OrderFailed StackFull{DETAIL_SEPARATOR}step (5, 9): full ({limit} of {limit})"
            ))),
            "{reasons:?}"
        );
        assert!(
            reasons.iter().any(|r| r.contains("NoPath(CutOff")
                && r.contains(&format!(
                    "{DETAIL_SEPARATOR}to ({}, {}): cut off at",
                    ENEMY_CITY.0, ENEMY_CITY.1
                ))
                && r.contains("is water")),
            "{reasons:?}"
        );
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
