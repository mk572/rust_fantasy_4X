//! Undoing orders within a turn.
//!
//! When undo is switched on ([`Game::set_undo`]), every command saves a snapshot of the world
//! first, and [`Game::undo`] goes back one command. Undo never gives an unfair look at hidden
//! things or a second try at luck, so a command that explored new squares, drew from the
//! random generator, or started a battle cannot be taken back, and neither can anything
//! before it. A step of the turn order (`game/step.rs`) clears the history.
//!
//! Tests: in tests/it/undo.rs.

use f4x_core::world::save::Snapshot;
use f4x_core::world::{Command, Event};

use super::save::GameState;
use super::Game;

/// How many commands back one can go.
const DEPTH: usize = 32;

/// One step of history: the game as it was before the command.
struct Entry {
    state: GameState,
    snapshot: Snapshot,
    label: String,
}

/// Whether undo is on, and what it can take back.
#[derive(Default)]
pub(super) struct History {
    on: bool,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoError {
    /// Undo is off, or there is nothing to take back.
    NothingToUndo,
    /// The saved state could not be rebuilt (content changed under it).
    Failed,
}

/// The command's name, for "Undo move".
fn label_of(command: &Command) -> String {
    let text = format!("{command:?}");
    text.split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("command")
        .to_string()
}

fn refused(events: &[Event]) -> bool {
    events.iter().any(|e| matches!(e, Event::Rejected(_)))
}

impl Game {
    /// Switch undo on or off; switching it off forgets the history.
    pub fn set_undo(&mut self, on: bool) {
        self.history.on = on;
        if !on {
            self.history.entries.clear();
        }
    }

    /// Whether undo is switched on.
    pub fn undo_enabled(&self) -> bool {
        self.history.on
    }

    pub fn can_undo(&self) -> bool {
        !self.history.entries.is_empty()
    }

    /// Forget what could be taken back.
    pub(super) fn forget_history(&mut self) {
        self.history.entries.clear();
    }

    /// Name of the command that [`Game::undo`] would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.history.entries.last().map(|e| e.label.as_str())
    }

    /// Run a command, recording how to take it back when that is allowed.
    pub(super) fn run_undoable(&mut self, command: Command) -> Vec<Event> {
        if !self.history.on || self.world.in_battle() {
            return self.world.apply(command, &mut self.state.rng);
        }
        let ends_history = matches!(command, Command::Battle(_) | Command::EnterSite { .. });
        let label = label_of(&command);
        let state = self.state.clone();
        let snapshot = (!ends_history).then(|| self.world.snapshot());
        let (draws, explored) = (self.draws(), self.world.explored_total());
        let events = self.world.apply(command, &mut self.state.rng);
        // A refused command changed nothing, so it leaves the history as it is.
        if refused(&events) {
            return events;
        }
        let irreversible = ends_history
            || self.draws() != draws
            || self.world.explored_total() != explored
            || self.world.in_battle()
            || events
                .iter()
                .any(|e| matches!(e, Event::BattleStarted { .. }));
        if irreversible {
            self.history.entries.clear();
        } else if let Some(snapshot) = snapshot {
            self.history.entries.push(Entry {
                state,
                snapshot,
                label,
            });
            if self.history.entries.len() > DEPTH {
                self.history.entries.remove(0);
            }
        }
        events
    }

    /// Take back the last command. Units and cities are rebuilt, so their `hecs` entities
    /// change: find them again by creation number (`entity_of_serial`).
    pub fn undo(&mut self) -> Result<String, UndoError> {
        let entry = self.history.entries.pop().ok_or(UndoError::NothingToUndo)?;
        let content = self.world.shared_content();
        // A copy is restored, so that a failure leaves the history as it was.
        match entry.snapshot.clone().restore(content) {
            Ok(mut world) => {
                // A save carries no name lists: the caller gave them, and they stay.
                world.set_names(self.world.names().clone());
                world.keep_locations_of(&self.world);
                self.world = world;
                self.state = entry.state;
                self.forget_last_command();
                Ok(entry.label)
            }
            Err(_) => {
                self.history.entries.push(entry);
                Err(UndoError::Failed)
            }
        }
    }
}
