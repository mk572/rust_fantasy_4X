//! The battle environment: one battle at a time, stepped one command by one.
//!
//! The side to play gives a command; both sides can be played by the caller (self-play). Every
//! command goes through `Game::battle_command`, the rules' own entry point, and the commands to
//! choose from are the lists the console shows (`Battle::legal_commands`,
//! `Game::legal_battle_casts`), so the environment can never offer a refused command.

use f4x_core::battle::{Battle, BattleCommand, BattleEvent, Side};
use f4x_core::content::Content;
use f4x_core::Game;

use crate::build::{build, ATTACKER, DEFENDER};
use crate::reward::{is_hurt, unit_value};
use crate::scenario::BattleScenario;

/// What a battle came to: units counted, and their value (`reward::unit_value`: one plus a
/// hundredth of the cost).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Outcome {
    /// `None` while the battle goes on.
    pub winner: Option<Side>,
    /// Units each side began with, indexed by [`Side::index`].
    pub sent: [u32; 2],
    /// Units each side lost (died).
    pub lost: [u32; 2],
    /// Units that got away when the side fled.
    pub saved_by_fleeing: [u32; 2],
    /// Mana each side's wizard spent from the reserve.
    pub mana_spent: [i32; 2],
    pub fled: [bool; 2],
    /// Different spells each side's wizard cast (a spell cast twice counts once).
    pub unique_spells: [u32; 2],
    /// The same in value: of the units each side began with, of those that died, and of those
    /// that got away when the side fled (after the units lost on the way: see `UnitsLostFleeing`,
    /// the lost ones are taken in proportion, the event does not say which).
    pub sent_value: [f32; 2],
    pub lost_value: [f32; 2],
    pub saved_value: [f32; 2],
    /// Of `saved_value`, the value of those below half their hit points.
    pub saved_hurt_value: [f32; 2],
    /// The value of each side's units still standing below half their hit points when the
    /// battle ended (as the last command before it left them; a side that fled has none).
    pub standing_hurt_value: [f32; 2],
}

/// A battle unit as it stood before the last command.
#[derive(Debug, Clone, Copy)]
struct Note {
    side: Side,
    value: f32,
    active: bool,
    hurt: bool,
}

impl Outcome {
    /// Enemy units destroyed by `side`.
    pub fn destroyed_by(&self, side: Side) -> u32 {
        self.lost[side.other().index()]
    }
}

/// What a step did.
#[derive(Debug, Clone)]
pub struct Step {
    pub events: Vec<BattleEvent>,
    pub done: bool,
}

pub struct BattleEnv {
    pub game: Game,
    pub scenario: BattleScenario,
    pub outcome: Outcome,
    alive: [u32; 2],
    /// The spells each side has cast so far, to count the different ones.
    cast: [Vec<f4x_core::rules::SpellId>; 2],
    /// Each battle unit by number as it stood before the last command (so the dead of the step
    /// are known with their side and value).
    notes: Vec<Note>,
    died: Vec<bool>,
    /// The value and number of the units of a side when it fled, before the lost ones.
    fled_value: [f32; 2],
    fled_hurt_value: [f32; 2],
    fled_count: [u32; 2],
    alive_value: [f32; 2],
    pub steps: u32,
}

/// The most commands one battle may take before it is stopped (a policy that never ends its
/// turn would otherwise run for ever).
pub const STEP_LIMIT: u32 = 20_000;

impl BattleEnv {
    pub fn new(content: &Content, scenario: BattleScenario) -> Result<Self, String> {
        let game = build(content, &scenario)?;
        let battle = game.battle().ok_or("no battle")?;
        let mut sent = [0u32; 2];
        let mut sent_value = [0.0f32; 2];
        for u in &battle.units {
            sent[u.side.index()] += 1;
            sent_value[u.side.index()] += unit_value(content, u);
        }
        let mut env = Self {
            game,
            scenario,
            outcome: Outcome {
                sent,
                sent_value,
                ..Outcome::default()
            },
            alive: sent,
            cast: [Vec::new(), Vec::new()],
            notes: Vec::new(),
            died: Vec::new(),
            fled_value: [0.0; 2],
            fled_hurt_value: [0.0; 2],
            fled_count: [0; 2],
            alive_value: sent_value,
            steps: 0,
        };
        env.note_units();
        Ok(env)
    }

    fn note_units(&mut self) {
        if let Some(b) = self.game.battle() {
            let content = self.game.content();
            let notes: Vec<Note> = b
                .units
                .iter()
                .map(|u| Note {
                    side: u.side,
                    value: unit_value(content, u),
                    active: u.is_active(),
                    hurt: is_hurt(u),
                })
                .collect();
            self.died.resize(notes.len(), false);
            self.notes = notes;
        }
    }

    pub fn battle(&self) -> Option<&Battle> {
        self.game.battle()
    }

    pub fn done(&self) -> bool {
        self.outcome.winner.is_some() || !self.game.in_battle() || self.steps >= STEP_LIMIT
    }

    /// The side to play.
    pub fn to_play(&self) -> Option<Side> {
        self.battle().map(|b| b.active)
    }

    /// Every command the side to play may give, spells included (each spell at its least and
    /// at its most mana).
    pub fn actions(&self) -> Vec<BattleCommand> {
        let Some(b) = self.battle() else {
            return Vec::new();
        };
        let mut out = b.legal_commands();
        for lc in self.game.legal_battle_casts() {
            let mut manas = vec![lc.least_mana];
            if lc.most_mana > lc.least_mana {
                manas.push(lc.most_mana);
            }
            for &aim in &lc.aims {
                for &mana in &manas {
                    out.push(BattleCommand::Cast {
                        spell: lc.spell,
                        aim,
                        mana,
                    });
                }
            }
        }
        out
    }

    /// Give a command for the side to play.
    pub fn step(&mut self, command: BattleCommand) -> Result<Step, String> {
        if self.done() {
            return Err("the battle is over".into());
        }
        self.note_units();
        let events = self
            .game
            .battle_command(command)
            .map_err(|e| format!("refused: {e:?}"))?;
        self.steps += 1;
        let mut seen = Vec::new();
        for e in &events {
            if let f4x_core::Event::Battle(b) = e {
                seen.push(b.clone());
            }
        }
        for b in &seen {
            match b {
                BattleEvent::UnitDied { unit } => {
                    if let Some(&n) = self.notes.get(*unit) {
                        let i = n.side.index();
                        self.outcome.lost[i] += 1;
                        self.outcome.lost_value[i] += n.value;
                        self.alive[i] = self.alive[i].saturating_sub(1);
                        self.alive_value[i] -= n.value;
                        self.died[*unit] = true;
                    }
                }
                BattleEvent::Fled { side } => {
                    let i = side.index();
                    self.outcome.fled[i] = true;
                    self.outcome.saved_by_fleeing[i] = self.alive[i];
                    self.fled_count[i] = self.alive[i];
                    self.alive[i] = 0;
                    for (k, n) in self.notes.iter().enumerate() {
                        if n.side == *side && n.active && !self.died[k] {
                            self.fled_value[i] += n.value;
                            if n.hurt {
                                self.fled_hurt_value[i] += n.value;
                            }
                        }
                    }
                    self.outcome.saved_value[i] = self.fled_value[i];
                    self.outcome.saved_hurt_value[i] = self.fled_hurt_value[i];
                }
                BattleEvent::SpellCast {
                    side, mana, spell, ..
                } => {
                    self.outcome.mana_spent[side.index()] += *mana;
                    let seen = &mut self.cast[side.index()];
                    if !seen.contains(spell) {
                        seen.push(*spell);
                        self.outcome.unique_spells[side.index()] += 1;
                    }
                }
                BattleEvent::Ended { winner } => self.outcome.winner = Some(*winner),
                _ => {}
            }
        }
        // Fleeing costs units, settled when the battle ends (task 250): only those that got
        // away count as saved.
        for e in &events {
            if let f4x_core::Event::UnitsLostFleeing { player, units } = e {
                let side = if *player == ATTACKER {
                    Side::Attacker
                } else {
                    Side::Defender
                };
                let i = side.index();
                let saved = &mut self.outcome.saved_by_fleeing[i];
                *saved = saved.saturating_sub(*units);
                // Which units were lost is not said: take them in proportion.
                let kept = self.fled_count[i].saturating_sub(*units) as f32
                    / self.fled_count[i].max(1) as f32;
                self.outcome.saved_value[i] = self.fled_value[i] * kept;
                self.outcome.saved_hurt_value[i] = self.fled_hurt_value[i] * kept;
            }
        }
        if self.outcome.winner.is_some() && self.outcome.standing_hurt_value == [0.0; 2] {
            for (k, n) in self.notes.iter().enumerate() {
                if n.active && n.hurt && !self.died[k] && !self.outcome.fled[n.side.index()] {
                    self.outcome.standing_hurt_value[n.side.index()] += n.value;
                }
            }
        }
        Ok(Step {
            events: seen,
            done: self.done(),
        })
    }
}

/// The wizards of a scenario: who is the attacker and who the defender.
pub const WIZARDS: [f4x_core::city::PlayerId; 2] = [ATTACKER, DEFENDER];
