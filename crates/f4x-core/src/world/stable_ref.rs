//! Commands as text. A [`Command`] names units and cities by `hecs` entity, and entity ids are not
//! the same after a load, so a command written down carries creation numbers
//! ([`crate::world::serial::Serial`]) in their place and is resolved against the world it is read
//! into. The `one` and `many` helpers do that for the entity fields of [`Command`]; the replays of
//! a game (`f4x-master`) are written with them.
//!
//! Tests: in f4x-master/tests/it/replay.rs.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::world::serial::Serial;
use crate::world::{Command, World};

#[derive(Default)]
struct Context {
    /// Writing: entity to creation number.
    to_serial: HashMap<hecs::Entity, u64>,
    /// Reading: creation number to entity.
    to_entity: HashMap<u64, hecs::Entity>,
}

thread_local! {
    static CONTEXT: RefCell<Context> = RefCell::new(Context::default());
}

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

impl World {
    /// Entities that carry creation numbers, both ways, into the thread's context.
    fn load_context(&self) {
        CONTEXT.with(|c| {
            let mut c = c.borrow_mut();
            c.to_serial.clear();
            c.to_entity.clear();
            for (e, s) in self.ecs.query::<&Serial>().iter() {
                c.to_serial.insert(e, s.0);
                c.to_entity.insert(s.0, e);
            }
        });
    }

    /// `command` as text, with creation numbers for the units and cities it names; `None` when it
    /// names one that has no number.
    pub fn command_to_text(&self, command: &Command) -> Option<String> {
        self.load_context();
        ron::ser::to_string(command).ok()
    }

    /// The command [`World::command_to_text`] wrote, for the units and cities of this world;
    /// `None` when the text is no command or names a unit or city that is not here.
    pub fn command_from_text(&self, text: &str) -> Option<Command> {
        self.load_context();
        ron::from_str(text).ok()
    }
}
