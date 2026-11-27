//! Command logs and replays.
//!
//! A [`Replay`] is a saved game plus every command and every step of the turn order since, so
//! the same game can be played again from that point (the random stream is part of the save). Units and cities are written as creation numbers (`f4x_core::world::stable_ref`).
//!
//! Recording is local play's tool for replays and for saving a game with its history; it does
//! not make a network protocol (see task 811, the multiplayer item).
//!
//! Tests: in tests/it/replay.rs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use f4x_core::world::save::{check_versions, ContentCheck, LoadError, SAVE_VERSION};
use f4x_core::world::{Command, Event};
use f4x_def::content::Content;

use super::save::SaveFile;
use super::step::Step;
use super::Game;

/// What a recorded game is: where it started and what was done since.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub version: u32,
    /// The game it started from, with no plans: a replay needs no computer players.
    pub start: SaveFile,
    /// What was done, in order.
    pub entries: Vec<Entry>,
}

/// One thing done to a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    /// A command, as text with creation numbers in place of entities.
    Command(String),
    /// A step of the turn order.
    Step(Step),
}

/// Why a replay could not be written or played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// Not recording.
    NotRecording,
    /// The text is not a replay, or is of another version.
    Load(String),
    /// The command in entry number `index` named a unit or city that does not exist.
    Unresolved(usize),
}

/// The recording in progress.
pub(super) struct Recording {
    start: SaveFile,
    entries: Vec<Entry>,
}

/// Just the version fields of a replay, read before the rest so that a file of another layout is
/// refused for its version and not for whatever it fails to parse as. The structs carry the names
/// of the ones they stand for, which RON checks.
mod header {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Start {
        #[serde(default)]
        pub game_version: String,
    }

    #[derive(Deserialize)]
    pub struct Replay {
        pub version: u32,
        pub start: Start,
    }
}

impl Game {
    /// Start recording from the game as it is now.
    pub fn start_recording(&mut self) {
        self.recording = Some(Recording {
            start: self.save_file(Vec::new()),
            entries: Vec::new(),
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
            entries: r.entries,
        })
    }

    /// The recording so far, which goes on.
    pub fn replay_so_far(&self) -> Result<Replay, ReplayError> {
        let r = self.recording.as_ref().ok_or(ReplayError::NotRecording)?;
        Ok(Replay {
            version: SAVE_VERSION,
            start: r.start.clone(),
            entries: r.entries.clone(),
        })
    }

    /// Write `command` down, before it runs (it names entities that may not survive it).
    pub(super) fn record_command(&mut self, command: &Command) {
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        match self.world.command_to_text(command) {
            Some(text) => recording.entries.push(Entry::Command(text)),
            // A command that cannot be written (an entity without a number) breaks the record.
            None => self.recording = None,
        }
    }

    /// Write a step down.
    pub(super) fn record_step(&mut self, step: Step) {
        if let Some(recording) = self.recording.as_mut() {
            recording.entries.push(Entry::Step(step));
        }
    }

    /// The last command did nothing that stays (it was refused or taken back).
    pub(super) fn forget_last_command(&mut self) {
        if let Some(r) = self.recording.as_mut() {
            r.entries.pop();
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

    /// Play the replay's first `count` entries into a game built from its start (all of them
    /// with `usize::MAX`). The game goes on recording, so a replay of it carries the same
    /// history; its events are returned for each entry.
    pub fn play(
        &self,
        content: impl Into<Arc<Content>>,
        count: usize,
    ) -> Result<(Game, Vec<Vec<Event>>), ReplayError> {
        let mut game = self
            .start
            .clone()
            .restore(content, ContentCheck::Strict)
            .map_err(|e| ReplayError::Load(format!("{e:?}")))?
            .game;
        game.start_recording();
        let mut all = Vec::new();
        for (i, entry) in self.entries.iter().take(count).enumerate() {
            all.push(match entry {
                Entry::Command(text) => {
                    let command = game
                        .world
                        .command_from_text(text)
                        .ok_or(ReplayError::Unresolved(i))?;
                    game.apply(command)
                }
                Entry::Step(step) => game.take(*step),
            });
        }
        Ok((game, all))
    }
}
