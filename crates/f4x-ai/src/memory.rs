//! What a wizard remembers of its dealings with the others (task 760, `docs/ai-diplomacy-b.md`).
//!
//! The wizard's own [`Ai`](crate::Ai) keeps its [`Memory`] and writes it into the save with its plans. The
//! memory is behind a shared handle ([`Shared`]) so that a [`Diplomats`] table, one for each game and made
//! by the host, can hold the same memory of every wizard: a proposal is answered by
//! `diplomacy::rate(game, ..)` through `answer_proposals` and `settle`, which have no `Ai` of the judge, and
//! the table gives them the memory of the real wizard. A host that makes no table loses nothing but this: the
//! judge then answers with an empty memory.
//!
//! The memory is fed by the events a human would be told of (`Controller::notice` in `f4x-master`,
//! [`Memory::notice`]): the battles the wizard fought and the cities it won and lost.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use f4x_core::game::Event;
use f4x_core::player::PlayerId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The dealings of one wizard with another: what it has been told of the fighting between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Dealing {
    pub with: PlayerId,
    pub battles_won: u32,
    pub battles_lost: u32,
    pub cities_taken: u32,
    pub cities_lost: u32,
    /// The turn of the last battle or city between them.
    pub last_clash: u32,
}

impl Dealing {
    fn new(with: PlayerId) -> Self {
        Self {
            with,
            battles_won: 0,
            battles_lost: 0,
            cities_taken: 0,
            cities_lost: 0,
            last_clash: 0,
        }
    }
}

/// Which diplomacy a wizard plays: A is the AI's statecraft as it was before task 761, B the new design
/// of `docs/ai-diplomacy-b.md` (tasks 762 and 763), built beside it so that a soak can compare them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum System {
    #[default]
    A,
    B,
}

impl System {
    pub fn name(self) -> &'static str {
        match self {
            System::A => "A",
            System::B => "B",
        }
    }
}

/// Everything a wizard keeps of the others.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Memory {
    /// The diplomacy it plays; A for a save that has none.
    #[serde(default)]
    pub system: System,
    pub dealings: Vec<Dealing>,
}

impl Memory {
    /// The dealings with `with`, if there were any. (Read by system B, task 762; until then the tests.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with(&self, with: PlayerId) -> Option<&Dealing> {
        self.dealings.iter().find(|d| d.with == with)
    }

    fn dealing(&mut self, with: PlayerId) -> &mut Dealing {
        let i = match self.dealings.iter().position(|d| d.with == with) {
            Some(i) => i,
            None => {
                self.dealings.push(Dealing::new(with));
                self.dealings.len() - 1
            }
        };
        &mut self.dealings[i]
    }

    /// Take in what `me` has been told: the battles it fought and the cities it won or lost. What
    /// happened to others is not its business.
    pub(crate) fn notice(&mut self, me: PlayerId, turn: u32, events: &[Event]) {
        for event in events {
            match *event {
                Event::BattleEnded { winner, loser } if winner == me && loser != me => {
                    let d = self.dealing(loser);
                    d.battles_won += 1;
                    d.last_clash = turn;
                }
                Event::BattleEnded { winner, loser } if loser == me && winner != me => {
                    let d = self.dealing(winner);
                    d.battles_lost += 1;
                    d.last_clash = turn;
                }
                Event::CityCaptured { from, to, .. } if to == me && from != me => {
                    let d = self.dealing(from);
                    d.cities_taken += 1;
                    d.last_clash = turn;
                }
                Event::CityCaptured { from, to, .. } if from == me && to != me => {
                    let d = self.dealing(to);
                    d.cities_lost += 1;
                    d.last_clash = turn;
                }
                _ => {}
            }
        }
    }
}

/// A memory behind a handle, shared by the wizard and the table. A copy of the wizard (`Clone`) has
/// a memory of its own, not the same one.
#[derive(Debug, Default)]
pub(crate) struct Shared(Rc<RefCell<Memory>>);

impl Clone for Shared {
    fn clone(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }
}

impl Shared {
    pub(crate) fn get(&self) -> std::cell::Ref<'_, Memory> {
        self.0.borrow()
    }

    pub(crate) fn put(&self) -> std::cell::RefMut<'_, Memory> {
        self.0.borrow_mut()
    }

    /// Put `memory` in place of what the handle holds, keeping the handle (and so the table's).
    pub(crate) fn replace(&self, memory: Memory) {
        *self.0.borrow_mut() = memory;
    }
}

/// Writes the memory a handle holds, reads it into a new handle.
pub(crate) mod shared {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(m: &Shared, s: S) -> Result<S::Ok, S::Error> {
        m.get().serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Shared, D::Error> {
        Ok(Shared(Rc::new(RefCell::new(Memory::deserialize(d)?))))
    }
}

/// The memory of every wizard of a game, for the code that answers for a wizard without its `Ai`.
/// One for each game, made by the host and handed to every `Ai` with [`crate::Ai::join`]; cloning
/// it shares the table.
#[derive(Debug, Clone, Default)]
pub struct Diplomats(Rc<RefCell<BTreeMap<PlayerId, Rc<RefCell<Memory>>>>>);

impl Diplomats {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&self, player: PlayerId, memory: &Shared) {
        self.0.borrow_mut().insert(player, Rc::clone(&memory.0));
    }

    /// What `player` remembers; an empty memory when it has none in the table.
    pub(crate) fn of(&self, player: PlayerId) -> Memory {
        self.0
            .borrow()
            .get(&player)
            .map(|m| m.borrow().clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: PlayerId = PlayerId(1);
    const FOE: PlayerId = PlayerId(2);
    const THIRD: PlayerId = PlayerId(3);

    #[test]
    fn a_wizard_notes_the_battles_it_fought_and_the_cities_it_won_and_lost() {
        let mut m = Memory::default();
        let city = hecs::Entity::DANGLING;
        m.notice(
            ME,
            10,
            &[
                Event::BattleEnded {
                    winner: ME,
                    loser: FOE,
                },
                Event::BattleEnded {
                    winner: FOE,
                    loser: ME,
                },
                Event::BattleEnded {
                    winner: FOE,
                    loser: ME,
                },
                Event::CityCaptured {
                    city,
                    from: FOE,
                    to: ME,
                },
                Event::CityCaptured {
                    city,
                    from: ME,
                    to: FOE,
                },
                Event::CityCaptured {
                    city,
                    from: ME,
                    to: FOE,
                },
            ],
        );
        let d = m.with(FOE).copied().expect("dealings with the foe");
        assert_eq!(
            (
                d.battles_won,
                d.battles_lost,
                d.cities_taken,
                d.cities_lost,
                d.last_clash
            ),
            (1, 2, 1, 2, 10)
        );
    }

    #[test]
    fn what_happens_between_others_is_not_noted() {
        let mut m = Memory::default();
        m.notice(
            ME,
            3,
            &[Event::BattleEnded {
                winner: FOE,
                loser: THIRD,
            }],
        );
        assert!(m.dealings.is_empty());
    }

    #[test]
    fn the_table_gives_the_memory_of_the_wizard_and_an_empty_one_for_a_stranger() {
        let table = Diplomats::new();
        let mine = Shared::default();
        table.add(ME, &mine);
        mine.put().notice(
            ME,
            5,
            &[Event::BattleEnded {
                winner: ME,
                loser: FOE,
            }],
        );
        assert_eq!(table.of(ME).with(FOE).map(|d| d.battles_won), Some(1));
        assert!(table.of(THIRD).dealings.is_empty());
    }

    #[test]
    fn the_memory_is_written_and_read_back() -> Result<(), Box<dyn std::error::Error>> {
        #[derive(Serialize, Deserialize)]
        struct Holder(#[serde(with = "shared")] Shared);
        let mut m = Memory::default();
        m.notice(
            ME,
            7,
            &[Event::BattleEnded {
                winner: FOE,
                loser: ME,
            }],
        );
        let text = ron::ser::to_string(&Holder(Shared(Rc::new(RefCell::new(m.clone())))))?;
        let back: Holder = ron::de::from_str(&text)?;
        assert_eq!(*back.0.get(), m);
        Ok(())
    }
}
