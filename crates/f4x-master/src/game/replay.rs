//! Command logs and replays.
//!
//! A [`Replay`] is a snapshot of the world plus every command applied since, so the same game can
//! be played again from that point (the random stream is part of the snapshot). Units and cities
//! are written as creation numbers (`f4x_core::world::stable_ref`).
//!
//! Recording is local play's tool for replays and for saving a game with its history; it does
//! not make a network protocol (see task 811, the multiplayer item).
//!
//! Tests: in tests/it/replay.rs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use f4x_core::world::save::{check_versions, LoadError, Snapshot, SAVE_VERSION};
use f4x_core::world::{Command, Event};
use f4x_def::content::Content;

use super::Game;

/// What a recorded game is: where it started and what was done since.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub version: u32,
    pub start: Snapshot,
    /// One command each, as text with creation numbers in place of entities.
    pub commands: Vec<String>,
}

/// Why a replay could not be written or played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// Not recording.
    NotRecording,
    /// The text is not a replay, or is of another version.
    Load(String),
    /// Command number `index` named a unit or city that does not exist.
    Unresolved(usize),
}

/// The recording in progress.
pub(super) struct Recording {
    start: Snapshot,
    commands: Vec<String>,
}

/// Just the version fields of a replay, read before the rest so that a file of another layout is
/// refused for its version and not for whatever it fails to parse as. The structs carry the names
/// of the ones they stand for, which RON checks.
mod header {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Snapshot {
        #[serde(default)]
        pub game_version: String,
    }

    #[derive(Deserialize)]
    pub struct Replay {
        pub version: u32,
        pub start: Snapshot,
    }
}

impl Game {
    /// Start recording from the game as it is now.
    pub fn start_recording(&mut self) {
        self.recording = Some(Recording {
            start: self.world.snapshot(),
            commands: Vec::new(),
        });
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording and hand over what was recorded.
    pub fn stop_recording(&mut self) -> Option<Replay> {
        let r = self.recording.take()?;
        Some(Replay {
            version: SAVE_VERSION,
            start: r.start,
            commands: r.commands,
        })
    }

    /// The recording so far, which goes on.
    pub fn replay_so_far(&self) -> Result<Replay, ReplayError> {
        let r = self.recording.as_ref().ok_or(ReplayError::NotRecording)?;
        Ok(Replay {
            version: SAVE_VERSION,
            start: r.start.clone(),
            commands: r.commands.clone(),
        })
    }

    /// Write `command` down, before it runs (it names entities that may not survive it).
    pub(super) fn record_command(&mut self, command: &Command) {
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        match self.world.command_to_text(command) {
            Some(text) => recording.commands.push(text),
            // A command that cannot be written (an entity without a number) breaks the record.
            None => self.recording = None,
        }
    }

    /// The last command did nothing that stays (it was refused or taken back).
    pub(super) fn forget_last_command(&mut self) {
        if let Some(r) = self.recording.as_mut() {
            r.commands.pop();
        }
    }
}

impl Replay {
    pub fn to_text(&self) -> Result<String, LoadError> {
        let body =
            ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().depth_limit(8))
                .map_err(|e| LoadError::Parse(e.to_string()))?;
        Ok(format!("// f4x replay, version {}\n{body}\n", self.version))
    }

    pub fn from_text(text: &str) -> Result<Self, ReplayError> {
        let load = |e: String| ReplayError::Load(e);
        let h: header::Replay =
            ron::from_str(text).map_err(|e| load(LoadError::Parse(e.to_string()).to_string()))?;
        check_versions(h.version, &h.start.game_version).map_err(|e| load(e.to_string()))?;
        ron::from_str(text).map_err(|e| load(e.to_string()))
    }

    /// Play the replay's first `count` commands into a game built from its start (all of them
    /// with `usize::MAX`). The game goes on recording, so a replay of it carries the same
    /// history; its events are returned for each command.
    pub fn play(
        &self,
        content: impl Into<Arc<Content>>,
        count: usize,
    ) -> Result<(Game, Vec<Vec<Event>>), ReplayError> {
        let world = self
            .start
            .clone()
            .restore(content)
            .map_err(|e| ReplayError::Load(format!("{e:?}")))?;
        let mut game = Game::new(world);
        game.start_recording();
        let mut all = Vec::new();
        for (i, text) in self.commands.iter().take(count).enumerate() {
            let command = game
                .world
                .command_from_text(text)
                .ok_or(ReplayError::Unresolved(i))?;
            all.push(game.apply(command));
        }
        Ok((game, all))
    }
}
