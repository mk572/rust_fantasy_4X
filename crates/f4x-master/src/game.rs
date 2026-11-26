//! The game: a world and the flow that advances it (task 822, `docs/turn-structure.md` section 10).
//!
//! A [`Game`] reads like its [`World`] (it dereferences to it), but whoever holds one changes the
//! world only by sending a command ([`Game::apply`]). Setting up a world, cheats and tests have a
//! way of their own, [`Game::world_mut`].
//!
//! The turn order (`game/step.rs`), undo (`game/undo.rs`) and replays (`game/replay.rs`) are the
//! game's: they see every command.
//!
//! Tests: in tests/it.

pub mod replay;
pub mod step;
pub mod undo;

use f4x_battle::battle::BattleCommand;
use f4x_core::world::{Command, Event, World};

/// A game being played.
pub struct Game {
    world: World,
    recording: Option<replay::Recording>,
    history: undo::History,
}

impl Game {
    /// A game of this world, with undo off and nothing recorded.
    pub fn new(world: World) -> Self {
        Self {
            world,
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

    /// The world, when the game around it is no longer needed.
    pub fn into_world(self) -> World {
        self.world
    }

    /// A player's command: run, written down when the game is recording, and kept for undo.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        // A game that has ended takes nothing more, and records nothing.
        if self.world.game_over.is_some() {
            return self.world.apply(command);
        }
        self.record_command(&command);
        let events = self.run_undoable(command);
        // A refused command changed nothing, so it is not part of the record.
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            self.forget_last_command();
        }
        events
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

/// Tests set a world up and change it through the game.
#[cfg(feature = "testutil")]
impl std::ops::DerefMut for Game {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
