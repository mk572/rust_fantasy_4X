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

use f4x_core::diplomacy::DiplomacyEvent;
use f4x_core::game::{Event, Game};
use f4x_core::player::PlayerId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// What a wizard does to another now and then, with a cool-down between two (`Memory::did`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Did {
    Warn,
    Demand,
    Gift,
    Trade,
}

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
    /// The weariness of the war these events belong to (system B, `diplo_b.rs`): up for a battle or
    /// a city lost, down for one won or taken, never below 0. It starts again when a war begins.
    #[serde(default)]
    pub weary: i32,
    /// The turn the war `weary` belongs to began (0: none).
    #[serde(default)]
    pub war_since: u32,
    /// The turn it last offered this one peace (system B's cool-down; 0: never).
    #[serde(default)]
    pub offered: u32,
    /// The turn it last asked this one for a pact or an alliance or gave it a gift (system B; 0: never).
    #[serde(default)]
    pub courted: u32,
    /// The turn it last warned, demanded of, gave a gift to and offered a trade to this one (0: never).
    #[serde(default)]
    pub warned: u32,
    #[serde(default)]
    pub demanded: u32,
    #[serde(default)]
    pub gifted: u32,
    #[serde(default)]
    pub traded: u32,
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
            weary: 0,
            war_since: 0,
            offered: 0,
            courted: 0,
            warned: 0,
            demanded: 0,
            gifted: 0,
            traded: 0,
        }
    }
}

/// What a wizard has seen another do that the others dislike (system B's fear): cities razed, wars
/// declared, treaties broken. `points` fade by half every `Knobs::ruth_half` turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Acts {
    pub who: PlayerId,
    pub razed: u32,
    pub wars: u32,
    pub broken: u32,
    /// The ruthlessness it adds up to, as of turn `stamp`.
    pub points: i32,
    pub stamp: u32,
}

/// Everything a wizard keeps of the others.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Memory {
    pub dealings: Vec<Dealing>,
    /// What it has seen the others do (system B's fear).
    #[serde(default)]
    pub acts: Vec<Acts>,
    /// The wizard it has chosen as its enemy and the turn it chose it, and the turn it last looked at the
    /// others (system B's friends and enemies, `diplo_b_stance.rs`).
    #[serde(default)]
    pub enemy: Option<(PlayerId, u32)>,
    #[serde(default)]
    pub reviewed: u32,
    /// What it feels about each of the others (task 780, `opinion.rs`).
    #[serde(default)]
    pub opinions: Vec<crate::opinion::Opinion>,
    /// The turn of the last per-turn look at the others (`Memory::look`, task 783).
    #[serde(default)]
    pub looked: Option<u32>,
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

    /// How ruthless `who` has been, as `me` saw it, faded to turn `now`.
    pub(crate) fn ruthlessness(&self, who: PlayerId, now: u32, half: u32) -> i32 {
        self.acts.iter().find(|a| a.who == who).map_or(0, |a| {
            let halvings = (now.saturating_sub(a.stamp) / half.max(1)).min(30);
            a.points >> halvings
        })
    }

    fn acts_of(&mut self, who: PlayerId) -> &mut Acts {
        let i = match self.acts.iter().position(|a| a.who == who) {
            Some(i) => i,
            None => {
                self.acts.push(Acts {
                    who,
                    razed: 0,
                    wars: 0,
                    broken: 0,
                    points: 0,
                    stamp: 0,
                });
                self.acts.len() - 1
            }
        };
        &mut self.acts[i]
    }

    /// Write down what a wizard met did: it razed a city, declared a war or broke a treaty.
    fn ruthless(&mut self, who: PlayerId, now: u32, kind: Ruth) {
        let k = crate::diplo_b::Knobs::current();
        let a = self.acts_of(who);
        a.points = (a.points >> (now.saturating_sub(a.stamp) / k.ruth_half.max(1)).min(30)).max(0);
        a.stamp = now;
        match kind {
            Ruth::Razed => {
                a.razed += 1;
                a.points += k.ruth_raze;
            }
            Ruth::War => {
                a.wars += 1;
                a.points += k.ruth_war;
            }
            Ruth::Broken => {
                a.broken += 1;
                a.points += k.ruth_broken;
            }
            Ruth::Fortress => a.points += k.ruth_fortress,
            Ruth::Destroyed => a.points += k.ruth_destroyed,
        }
    }

    /// Write down that `with` was courted (a pact asked for, a gift given) on this turn.
    pub(crate) fn courted(&mut self, with: PlayerId, turn: u32) {
        self.dealing(with).courted = turn;
    }

    /// Write down that something was done to `with` on this turn (a warning, a demand, a gift, a trade offer).
    pub(crate) fn did(&mut self, with: PlayerId, what: Did, turn: u32) {
        let d = self.dealing(with);
        match what {
            Did::Warn => d.warned = turn,
            Did::Demand => d.demanded = turn,
            Did::Gift => d.gifted = turn,
            Did::Trade => d.traded = turn,
        }
    }

    /// Whether `cool` turns have passed since `what` was done to `with`.
    pub(crate) fn due(&self, with: PlayerId, what: Did, turn: u32, cool: u32) -> bool {
        let last = self.with(with).map_or(0, |d| match what {
            Did::Warn => d.warned,
            Did::Demand => d.demanded,
            Did::Gift => d.gifted,
            Did::Trade => d.traded,
        });
        last == 0 || turn >= last + cool
    }

    /// Write down that peace was offered to `with` on this turn.
    pub(crate) fn offered_peace(&mut self, with: PlayerId, turn: u32) {
        self.dealing(with).offered = turn;
    }

    /// Take in what `me` has been told: the battles it fought and the cities it won or lost against
    /// another wizard or neutral race. What happened to others, and its fights with monsters, are not
    /// its business.
    pub(crate) fn notice(&mut self, me: PlayerId, game: &Game, events: &[Event]) {
        use crate::diplo_b::Knobs;
        let turn = game.turn;
        let known =
            |p: PlayerId| p != me && (game.is_wizard(p) || game.race_of_player(p).is_some());
        let k = Knobs::current();
        for event in events {
            self.feel(game, me, event);
            // What the others do that is ruthless, seen when they are known to it.
            let act = match *event {
                Event::CityRazed { player, .. } => Some((player, Ruth::Razed)),
                Event::Diplomacy(DiplomacyEvent::WarDeclared { attacker, .. }) => {
                    Some((attacker, Ruth::War))
                }
                Event::Diplomacy(DiplomacyEvent::TreatyBroken { breaker, .. }) => {
                    Some((breaker, Ruth::Broken))
                }
                // Whoever the winner was to the one who fell, it is a power to fear.
                Event::Diplomacy(DiplomacyEvent::FortressTaken { winner, loser })
                    if loser != me =>
                {
                    Some((winner, Ruth::Fortress))
                }
                Event::Diplomacy(DiplomacyEvent::WizardDestroyed { winner, loser })
                    if loser != me =>
                {
                    Some((winner, Ruth::Destroyed))
                }
                _ => None,
            };
            if let Some((who, kind)) = act {
                if known(who) && game.have_met(me, who) {
                    self.ruthless(who, turn, kind);
                }
                continue;
            }
            let (with, battle, won, city) = match *event {
                Event::BattleEnded { winner, loser } if winner == me => (loser, true, true, false),
                Event::BattleEnded { winner, loser } if loser == me => (winner, true, false, false),
                Event::CityCaptured { from, to, .. } if to == me => (from, false, true, true),
                Event::CityCaptured { from, to, .. } if from == me => (to, false, false, true),
                _ => continue,
            };
            if !known(with) {
                continue;
            }
            // A war that began since the last clash is another war: its weariness starts at 0.
            let since = game
                .war_age(me, with)
                .map_or(0, |age| turn.saturating_sub(age));
            let d = self.dealing(with);
            if d.war_since != since {
                d.war_since = since;
                d.weary = 0;
            }
            d.last_clash = turn;
            let (up, down) = if city {
                (k.city_lost, k.city_taken)
            } else {
                (k.battle_lost, k.battle_won)
            };
            match (battle, won) {
                (true, true) => d.battles_won += 1,
                (true, false) => d.battles_lost += 1,
                (false, true) => d.cities_taken += 1,
                (false, false) => d.cities_lost += 1,
            }
            d.weary = (d.weary + if won { -down } else { up }).max(0);
        }
    }
}

/// The acts a wizard counts against another.
enum Ruth {
    Razed,
    War,
    Broken,
    Fortress,
    Destroyed,
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

/// The feeling at or below which a wizard will not talk to the other (not at war).
const AUDIENCE_REFUSED_BELOW: i32 = -90;

impl Diplomats {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&self, player: PlayerId, memory: &Shared) {
        self.0.borrow_mut().insert(player, Rc::clone(&memory.0));
    }

    pub(crate) fn add_if_absent(&self, player: PlayerId, memory: &Shared) {
        self.0
            .borrow_mut()
            .entry(player)
            .or_insert_with(|| Rc::clone(&memory.0));
    }

    /// What `me` feels about `other` (`Memory::opinion`); the start when it has no memory in the table.
    pub(crate) fn opinion(
        &self,
        game: &Game,
        me: PlayerId,
        other: PlayerId,
    ) -> crate::opinion::Opinion {
        match self.0.borrow().get(&me) {
            Some(m) => m.borrow().opinion(game, me, other),
            None => Memory::default().opinion(game, me, other),
        }
    }

    /// How friendly `me` feels about `other`.
    pub(crate) fn friendship(&self, game: &Game, me: PlayerId, other: PlayerId) -> i32 {
        self.opinion(game, me, other).friendship
    }

    /// How friendly `me` feels about `other`, -100..=100: what the human is shown of a wizard's regard
    /// (task 784; 781 refines it with the fear and the strategic regard).
    pub fn feeling(&self, game: &Game, me: PlayerId, other: PlayerId) -> i32 {
        self.friendship(game, me, other)
    }

    /// Set what `me` feels about `other` outright: the demo screens and tests that need a wizard in a mood.
    /// A wizard that has no memory in the table gets one.
    pub fn set_feeling(&self, game: &Game, me: PlayerId, other: PlayerId, to: i32) {
        let memory = Rc::clone(
            self.0
                .borrow_mut()
                .entry(me)
                .or_insert_with(|| Rc::new(RefCell::new(Memory::default()))),
        );
        memory.borrow_mut().set_friendship(game, me, other, to);
    }

    /// Whether `me` will not see `other` to talk (the human's "will not see you"): it hates the other
    /// beyond reason and is not at war with them (at war a wizard always hears an offer of peace).
    pub fn refuses_audience(&self, game: &Game, me: PlayerId, other: PlayerId) -> bool {
        self.friendship(game, me, other) <= AUDIENCE_REFUSED_BELOW
            && game.treaty(me, other) != f4x_core::diplomacy::Treaty::War
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

    use crate::testworld::{game, COAST_CITY, P0, P1};
    use f4x_core::player::MONSTERS;

    const ME: PlayerId = P0;
    const FOE: PlayerId = P1;
    /// The monsters: not a wizard, so its fights are not dealings.
    const THIRD: PlayerId = MONSTERS;

    /// A game at turn `turn`.
    fn at_turn(turn: u32) -> Game {
        let mut g = game(COAST_CITY, 8 /* high_men */);
        g.turn = turn;
        g
    }

    #[test]
    fn a_wizard_notes_the_battles_it_fought_and_the_cities_it_won_and_lost() {
        let mut m = Memory::default();
        let city = hecs::Entity::DANGLING;
        m.notice(
            ME,
            &at_turn(10),
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
    fn what_happens_between_others_and_with_monsters_is_not_noted() {
        let mut m = Memory::default();
        m.notice(
            ME,
            &at_turn(3),
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
            &at_turn(5),
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
            &at_turn(7),
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
