//! The game: a world and the flow that advances it (task 822, `docs/turn-structure.md` section 10).
//!
//! A [`Game`] reads like its [`World`] (it dereferences to it), but whoever holds one changes the
//! world only by sending a command ([`Game::apply`]). Setting up a world, cheats and tests have a
//! way of their own, [`Game::world_mut`] and, for what draws, [`Game::world_and_rng`].
//!
//! The random numbers are the game's: it hands its generator to each world operation that draws.
//!
//! The turn order (`game/step.rs`), what a player may do on another's turn (`game/reaction.rs`),
//! the attacks that wait for an answer (`game/attack.rs`),
//! the end of the game (`game/end.rs`), the saved game (`game/save.rs`), undo (`game/undo.rs`)
//! and replays (`game/replay.rs`) are the game's: they see every command.
//!
//! Tests: in tests/it.

pub mod attack;
pub mod end;
pub mod reaction;
pub mod replay;
pub mod save;
pub mod step;
pub mod undo;

use f4x_battle::battle::BattleCommand;
use f4x_core::battle::world::BattleStartError;
use f4x_core::player::PlayerId;
use f4x_core::world::rng::GameRng;
use f4x_core::world::{Command, Event, Rejection, World};

/// A game being played.
pub struct Game {
    world: World,
    state: save::GameState,
    recording: Option<replay::Recording>,
    history: undo::History,
}

impl Game {
    /// A game of this world in its first round, with undo off and nothing recorded. `rng` is the
    /// generator all its random numbers come from; one that made the world goes on from there.
    pub fn new(world: World, rng: GameRng) -> Self {
        let state = save::GameState::first_round(rng, step::turn_players(&world));
        Self {
            world,
            state,
            recording: None,
            history: undo::History::default(),
        }
    }

    /// The world as it is.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The world to change by hand: for setting one up, for cheats and for tests. A player sends
    /// commands ([`Game::apply`]) instead.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// The world and the generator, to change the world by hand in a way that draws.
    pub fn world_and_rng(&mut self) -> (&mut World, &mut GameRng) {
        (&mut self.world, &mut self.state.rng)
    }

    /// The seed this game started from.
    pub fn seed(&self) -> u64 {
        self.state.rng.seed()
    }

    /// How many random numbers have been drawn.
    pub fn draws(&self) -> u64 {
        self.state.rng.draws()
    }

    /// The world, when the game around it is no longer needed.
    pub fn into_world(self) -> World {
        self.world
    }

    /// A command `player` sends: run, written down when the game is recording, and kept for
    /// undo. Refused when the game has ended, when the game does not have the player, when the
    /// command only sets a world up, when it is another player's to give
    /// ([`World::may_command`]), and on another player's turn ([`Game::turn`]) when it is no
    /// reaction (`game/reaction.rs`). What opens or answers a reaction ends the undo history.
    /// An attack on the human is not fought but held until it answers (`game/attack.rs`): such a
    /// command has no events of its own.
    pub fn apply(&mut self, player: PlayerId, command: Command) -> Vec<Event> {
        // A refusal changes nothing and records nothing.
        if let Some(why) = self.refusal(player, &command) {
            return vec![Event::Rejected(why)];
        }
        self.record_command(player, &command);
        let reacting = self.turn().is_some_and(|active| active != player);
        let requests = self.world.diplomacy.proposals().to_vec();
        let answered = self.answered_attack(&command);
        let mut events = match self.attack_to_hold(&command) {
            Some((attackers, defenders)) => match self.hold_attack(attackers, defenders) {
                Ok(()) => {
                    self.forget_history();
                    Vec::new()
                }
                Err(why) => vec![Event::Rejected(Rejection::BattleStart(why))],
            },
            None => self.run_undoable(command.clone()),
        };
        // A refused command changed nothing, so it is not part of the record.
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            self.forget_last_command();
        } else {
            if let Some(at) = answered {
                self.state.attacks.remove(at);
            }
            let opened = self.open_reactions(&events);
            if opened || reacting || requests != self.world.diplomacy.proposals() {
                self.forget_history();
            }
        }
        events.extend(self.check_end());
        events
    }

    /// Why the game itself turns `command` of `player` down, before the world sees it.
    fn refusal(&self, player: PlayerId, command: &Command) -> Option<Rejection> {
        if self.state.game_over.is_some() {
            Some(Rejection::GameOver)
        } else if self.world.player(player).is_none() {
            Some(Rejection::NoSuchPlayer)
        } else if command.is_setup() {
            Some(Rejection::SetupOnly)
        } else if !self.world.may_command(player, command) {
            Some(Rejection::NotYourCommand)
        } else if self.turn().is_some_and(|active| active != player)
            && !self.is_reaction(player, command)
        {
            Some(Rejection::NotYourTurn)
        } else if matches!(command, Command::Defend { .. })
            && self.answered_attack(command).is_none()
        {
            Some(Rejection::BattleStart(BattleStartError::NoSuchAttack))
        } else {
            None
        }
    }

    /// The next command of the side to play in the battle being fought
    /// ([`World::next_battle_command`]).
    pub fn next_battle_command(&mut self) -> Option<BattleCommand> {
        self.world.next_battle_command()
    }
}

impl std::ops::Deref for Game {
    type Target = World;

    fn deref(&self) -> &World {
        &self.world
    }
}

/// Tests put a game in the round they need.
#[cfg(feature = "testutil")]
impl Game {
    /// Skip to `round` without playing the rounds between: the clock moves, and the wars are as
    /// much older or younger.
    pub fn set_round(&mut self, round: u32) {
        let skipped = i64::from(round) - i64::from(self.state.round);
        self.world.scenario_age_wars(skipped);
        self.state.round = round;
    }
}

/// Tests give the orders of whichever player they need.
#[cfg(feature = "testutil")]
impl Game {
    /// Send `command` as the player it acts for ([`World::command_actor`]), or run it on the
    /// world when it sets the world up ([`Command::is_setup`]).
    pub fn command(&mut self, command: Command) -> Vec<Event> {
        if command.is_setup() {
            let mut events = self.world.apply(command, &mut self.state.rng);
            events.extend(self.check_end());
            return events;
        }
        // One that names nobody is sent by whoever comes first: the world refuses it.
        let player = self.world.command_actor(&command);
        let player = player.or_else(|| self.world.player_ids().first().copied());
        self.apply(player.unwrap_or(f4x_core::player::MONSTERS), command)
    }
}

// Tests call the world operations that draw through the game, which hands them its generator.
#[cfg(feature = "testutil")]
f4x_core::drawing_ops!(Game);

#[cfg(feature = "testutil")]
impl From<f4x_core::world::testutil::Table> for Game {
    fn from(table: f4x_core::world::testutil::Table) -> Self {
        Self::new(table.world, table.rng)
    }
}

impl From<(World, GameRng)> for Game {
    fn from((world, rng): (World, GameRng)) -> Self {
        Self::new(world, rng)
    }
}

/// Tests set a world up and change it through the game.
#[cfg(feature = "testutil")]
impl std::ops::DerefMut for Game {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
