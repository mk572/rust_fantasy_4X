//! Command logs and replays.
//!
//! A [`Replay`] is a snapshot of the game plus every command applied since, so the same game can
//! be played again from that point (the random stream is part of the snapshot). Commands name
//! units and cities by `hecs` entity, and entity ids are not the same after a load, so each
//! command is written with creation numbers ([`crate::game::serial::Serial`]) in their place and
//! resolved against the game it is played into. That is what [`stable_ref`] does for the
//! `Command` fields.
//!
//! Recording is local play's tool for replays and for saving a game with its history; it does
//! not make a network protocol (see `PLANNED.md`, the multiplayer item).

use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::game::save::{LoadError, Snapshot, SAVE_VERSION};
use crate::game::serial::Serial;
use crate::game::{Command, Event, Game};

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

#[derive(Default)]
struct Context {
    /// Writing: entity to creation number.
    to_serial: HashMap<hecs::Entity, u64>,
    /// Reading: creation number to entity.
    to_entity: HashMap<u64, hecs::Entity>,
    /// Spells by id, both ways: a command names a spell, not its number in the rules.
    spell_names: HashMap<crate::rules::SpellId, String>,
    spell_ids: HashMap<String, crate::rules::SpellId>,
}

thread_local! {
    static CONTEXT: RefCell<Context> = RefCell::new(Context::default());
}

/// `serde(with)` helpers for the entity fields of [`Command`].
pub mod stable_ref {
    use super::CONTEXT;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    fn serial_of<E: serde::ser::Error>(e: hecs::Entity) -> Result<u64, E> {
        CONTEXT.with(|c| {
            c.borrow()
                .to_serial
                .get(&e)
                .copied()
                .ok_or_else(|| E::custom("entity has no creation number"))
        })
    }

    fn entity_of<E: serde::de::Error>(n: u64) -> Result<hecs::Entity, E> {
        CONTEXT.with(|c| {
            c.borrow()
                .to_entity
                .get(&n)
                .copied()
                .ok_or_else(|| E::custom(format!("no unit or city numbered {n}")))
        })
    }

    /// A spell, written as its id (`fire_bolt`), so a replay still reads after `spells.ron`
    /// changes order.
    pub mod spell {
        use super::*;
        use crate::rules::SpellId;

        pub fn serialize<S: Serializer>(id: &SpellId, s: S) -> Result<S::Ok, S::Error> {
            CONTEXT.with(|c| {
                c.borrow()
                    .spell_names
                    .get(id)
                    .ok_or_else(|| serde::ser::Error::custom("unknown spell"))
                    .and_then(|name| name.serialize(s))
            })
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SpellId, D::Error> {
            let name = String::deserialize(d)?;
            CONTEXT.with(|c| {
                c.borrow()
                    .spell_ids
                    .get(&name)
                    .copied()
                    .ok_or_else(|| serde::de::Error::custom(format!("no spell called {name}")))
            })
        }
    }

    pub mod one {
        use super::*;

        pub fn serialize<S: Serializer>(e: &hecs::Entity, s: S) -> Result<S::Ok, S::Error> {
            serial_of::<S::Error>(*e)?.serialize(s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<hecs::Entity, D::Error> {
            entity_of(u64::deserialize(d)?)
        }
    }

    pub mod many {
        use super::*;

        pub fn serialize<S: Serializer>(v: &[hecs::Entity], s: S) -> Result<S::Ok, S::Error> {
            let numbers: Result<Vec<u64>, S::Error> =
                v.iter().map(|&e| serial_of::<S::Error>(e)).collect();
            numbers?.serialize(s)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<hecs::Entity>, D::Error> {
            Vec::<u64>::deserialize(d)?
                .into_iter()
                .map(entity_of::<D::Error>)
                .collect()
        }
    }
}

/// The recording in progress.
#[derive(Debug, Clone)]
pub(crate) struct Recording {
    start: Snapshot,
    commands: Vec<String>,
}

impl Game {
    /// Entities that carry creation numbers, both ways, into the thread's context.
    fn load_context(&self) {
        CONTEXT.with(|c| {
            let mut c = c.borrow_mut();
            c.to_serial.clear();
            c.to_entity.clear();
            c.spell_names.clear();
            c.spell_ids.clear();
            let spells = &self.content.rules.spells;
            for def in spells.defs() {
                if let Some(id) = spells.id_of(&def.id) {
                    c.spell_names.insert(id, def.id.clone());
                    c.spell_ids.insert(def.id.clone(), id);
                }
            }
            for (e, s) in self.world.query::<&Serial>().iter() {
                c.to_serial.insert(e, s.0);
                c.to_entity.insert(s.0, e);
            }
        });
    }

    /// Start recording from the game as it is now.
    pub fn start_recording(&mut self) -> Result<(), ReplayError> {
        self.recording = Some(Recording {
            start: self.snapshot(),
            commands: Vec::new(),
        });
        Ok(())
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
    pub(crate) fn record_command(&mut self, command: &Command) {
        if self.recording.is_none() {
            return;
        }
        self.load_context();
        let text = ron::ser::to_string(command);
        if let (Some(r), Ok(text)) = (self.recording.as_mut(), text) {
            r.commands.push(text);
        } else {
            // A command that cannot be written (an entity without a number) breaks the record.
            self.recording = None;
        }
    }

    /// The last command did nothing that stays (it was refused or taken back).
    pub(crate) fn forget_last_command(&mut self) {
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
        crate::game::save::check_replay_text(text).map_err(|e| ReplayError::Load(e.to_string()))?;
        ron::from_str(text).map_err(|e| ReplayError::Load(e.to_string()))
    }

    /// Play the replay's first `count` commands into a game built from its start (all of them
    /// with `usize::MAX`). The game goes on recording, so a replay of it carries the same
    /// history; its events are returned for each command.
    pub fn play(
        &self,
        content: crate::content::Content,
        count: usize,
    ) -> Result<(Game, Vec<Vec<Event>>), ReplayError> {
        let mut game = self
            .start
            .clone()
            .restore(content)
            .map_err(|e| ReplayError::Load(format!("{e:?}")))?;
        game.start_recording()?;
        let mut all = Vec::new();
        for (i, text) in self.commands.iter().take(count).enumerate() {
            game.load_context();
            let command: Command = ron::from_str(text).map_err(|_| ReplayError::Unresolved(i))?;
            all.push(game.apply(command));
        }
        Ok((game, all))
    }
}

#[cfg(test)]
mod tests;
