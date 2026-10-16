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
use crate::scenario::BattleScenario;

/// What a battle came to, counted in units (the game has no unit value yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// The side of each battle unit, by number (kept so the dead of the last step are known).
    sides: Vec<Side>,
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
        for u in &battle.units {
            sent[u.side.index()] += 1;
        }
        let mut env = Self {
            game,
            scenario,
            outcome: Outcome {
                sent,
                ..Outcome::default()
            },
            alive: sent,
            cast: [Vec::new(), Vec::new()],
            sides: Vec::new(),
            steps: 0,
        };
        env.note_sides();
        Ok(env)
    }

    fn note_sides(&mut self) {
        if let Some(b) = self.game.battle() {
            self.sides = b.units.iter().map(|u| u.side).collect();
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
        self.note_sides();
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
                    if let Some(&s) = self.sides.get(*unit) {
                        self.outcome.lost[s.index()] += 1;
                        self.alive[s.index()] = self.alive[s.index()].saturating_sub(1);
                    }
                }
                BattleEvent::Fled { side } => {
                    self.outcome.fled[side.index()] = true;
                    self.outcome.saved_by_fleeing[side.index()] = self.alive[side.index()];
                    self.alive[side.index()] = 0;
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
                let saved = &mut self.outcome.saved_by_fleeing[side.index()];
                *saved = saved.saturating_sub(*units);
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
