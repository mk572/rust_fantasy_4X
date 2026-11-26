//! Undoing orders within a turn.
//!
//! When undo is switched on (`World::set_undo`), every command saves a snapshot of the game
//! first, and `World::undo` goes back one command. Undo never gives an unfair look at hidden
//! things or a second try at luck, so a command that explored new squares, drew from the
//! random generator, or started a battle cannot be taken back, and neither can anything
//! before it. Ending the turn clears the history.

use crate::world::save::Snapshot;
use crate::world::{Command, Event, World};

/// How many commands back one can go.
const DEPTH: usize = 32;

/// One step of history: the game as it was before the command.
pub(crate) struct UndoEntry {
    snapshot: Snapshot,
    label: String,
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

impl World {
    /// A refused command changed nothing, so it is not part of the record.
    fn forget_if_rejected(&mut self, events: &[Event]) {
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            self.forget_last_command();
        }
    }

    /// Switch undo on or off; switching it off forgets the history.
    pub fn set_undo(&mut self, on: bool) {
        self.undo_on = on;
        if !on {
            self.undo_stack.clear();
        }
    }

    /// Whether undo is switched on.
    pub fn undo_enabled(&self) -> bool {
        self.undo_on
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Name of the command that `undo` would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|e| e.label.as_str())
    }

    /// Everything the player has explored, summed: it only ever grows.
    fn explored_total(&self) -> usize {
        self.player_ids()
            .into_iter()
            .map(|id| {
                crate::map::Plane::all()
                    .map(|pl| self.fog.explored_count(id, pl))
                    .sum::<usize>()
            })
            .sum()
    }

    /// Run a command, recording how to take it back when that is allowed.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        if self.game_over.is_some() {
            return vec![Event::Rejected(crate::world::Rejection::GameOver)];
        }
        self.record_command(&command);
        if !self.undo_on || self.battle.is_some() {
            let events = self.execute(command);
            self.forget_if_rejected(&events);
            return events;
        }
        let ends_history = matches!(
            command,
            Command::EndTurn
                | Command::StartRound
                | Command::StartPlayerTurn { .. }
                | Command::EndPlayerTurn { .. }
                | Command::EndRound
                | Command::Battle(_)
                | Command::EnterSite { .. }
        );
        let label = label_of(&command);
        let before = (!ends_history).then(|| self.snapshot());
        let (draws, explored) = (self.rng.draws(), self.explored_total());
        let events = self.execute(command);
        self.forget_if_rejected(&events);
        let rejected = events.iter().any(|e| matches!(e, Event::Rejected(_)));
        // A refused command changed nothing, so it leaves the history as it is.
        let irreversible = !rejected
            && (ends_history
                || self.rng.draws() != draws
                || self.explored_total() != explored
                || self.battle.is_some()
                || events
                    .iter()
                    .any(|e| matches!(e, Event::BattleStarted { .. })));
        if irreversible {
            self.undo_stack.clear();
        } else if !rejected {
            if let Some(snapshot) = before {
                self.undo_stack.push(UndoEntry { snapshot, label });
                if self.undo_stack.len() > DEPTH {
                    self.undo_stack.remove(0);
                }
            }
        }
        events
    }

    /// Take back the last command. Units and cities are rebuilt, so their `hecs` entities
    /// change: find them again by creation number (`entity_of_serial`).
    pub fn undo(&mut self) -> Result<String, UndoError> {
        let entry = self.undo_stack.pop().ok_or(UndoError::NothingToUndo)?;
        let rest = std::mem::take(&mut self.undo_stack);
        let recording = self.recording.take();
        let content = self.content.clone();
        match entry.snapshot.restore(content) {
            Ok(mut world) => {
                world.undo_on = true;
                world.undo_stack = rest;
                world.recording = recording;
                *self = world;
                self.forget_last_command();
                Ok(entry.label)
            }
            Err(_) => {
                self.recording = recording;
                self.undo_stack = rest;
                Err(UndoError::Failed)
            }
        }
    }
}
