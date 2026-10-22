//! Playing a battle as a series of decisions for the agent (`docs/learning-ai.md` 7.3 and 9).
//!
//! A [`Driver`] runs a [`BattleEnv`] until the agent has to decide something, hands that over as a
//! [`Pending`] decision (the observation, what is allowed, and the scripted AI's advice), and
//! applies the answer. Sides the agent does not play are played by a function the caller gives
//! (the scripted AI, a random player, or another network).
//!
//! The unit that decides is the one the scripted battle AI would move next, so the agent chooses
//! what that unit does, not which unit goes (a pointer over the units is a later step); the
//! scripted AI's own order for that unit is the *advice*. A turn ends by itself when the
//! scripted AI has nothing left to move.

use f4x_core::battle::spell::LegalCast;
use f4x_core::battle::{Battle, BattleCommand, Side};
use f4x_core::game::Game;

use crate::act::{self, WizardOptions};
use crate::env::BattleEnv;
use crate::obs::{Brief, ObsSpec, Observation};
use crate::policy;

/// What the agent is asked.
#[derive(Debug, Clone)]
pub enum Request {
    /// The wizard: cast nothing, flee, or cast. Once at the start of the side's turn.
    Wizard {
        options: WizardOptions,
        casts: Vec<LegalCast>,
    },
    /// The unit `unit` (a battle unit number) gives an order.
    Unit {
        unit: usize,
        /// The unit's token in the observation.
        slot: usize,
        mask: Vec<bool>,
        /// The index in the unit vocabulary of the scripted AI's order, if it has one.
        advice: Option<usize>,
    },
}

#[derive(Debug, Clone)]
pub struct Pending {
    pub side: Side,
    pub obs: Observation,
    pub request: Request,
}

/// What the agent's side is to do next.
pub enum Next {
    /// A command that needs no decision (the turn ends when the scripted AI has nothing to move).
    Auto(BattleCommand),
    Ask(Box<Pending>),
}

/// The next step for `side` in the battle of `game`, played by the agent: the wizard's decision
/// once a turn (`wizard_turn` remembers the turn of the last one), then the units'.
pub fn next_for_agent(
    game: &Game,
    spec: &ObsSpec,
    side: Side,
    brief: Brief,
    wizard_turn: &mut [u32; 2],
) -> Result<Next, String> {
    let battle = game.battle().ok_or("no battle")?;
    let turn = battle.turn;
    // A new battle starts again from turn 1.
    if wizard_turn.iter().any(|&t| t > turn) {
        *wizard_turn = [0; 2];
    }
    if wizard_turn[side.index()] != turn {
        wizard_turn[side.index()] = turn;
        let obs = spec.observe(game, side, brief).ok_or("no observation")?;
        let casts = game.legal_battle_casts();
        let options = act::wizard_options(&casts, brief.may_flee, &obs);
        if options.any() {
            return Ok(Next::Ask(Box::new(Pending {
                side,
                obs,
                request: Request::Wizard { options, casts },
            })));
        }
    }
    let scripted = policy::scripted_command(battle);
    let unit = match &scripted {
        BattleCommand::EndTurn => return Ok(Next::Auto(BattleCommand::EndTurn)),
        BattleCommand::Move { unit, .. }
        | BattleCommand::Attack { unit, .. }
        | BattleCommand::Shoot { unit, .. }
        | BattleCommand::AttackWall { unit, .. }
        | BattleCommand::ShootWall { unit, .. }
        | BattleCommand::Done { unit } => *unit,
        other => return Err(format!("the scripted AI gave {other:?}")),
    };
    let obs = spec.observe(game, side, brief).ok_or("no observation")?;
    let legal = battle.legal_commands();
    let mask = act::unit_mask(&legal, unit, &obs);
    let slot = (0..crate::obs::MAX_UNITS)
        .find(|&t| obs.unit_mask[t] && obs.unit_ids[t] == unit)
        .ok_or("the acting unit has no token")?;
    let advice = act::unit_index(&scripted, &obs);
    Ok(Next::Ask(Box::new(Pending {
        side,
        obs,
        request: Request::Unit {
            unit,
            slot,
            mask,
            advice,
        },
    })))
}

pub struct Driver<'a> {
    pub env: BattleEnv,
    spec: &'a ObsSpec,
    /// What each side's strategic AI asked of it.
    pub briefs: [Brief; 2],
    /// The turn of each side's last wizard decision.
    wizard_turn: [u32; 2],
}

impl<'a> Driver<'a> {
    pub fn new(env: BattleEnv, spec: &'a ObsSpec, brief: Brief) -> Self {
        Self {
            env,
            spec,
            briefs: [brief; 2],
            wizard_turn: [0; 2],
        }
    }

    /// Give `side` a brief: its mana budget limits the wizard's casting in the game.
    pub fn set_brief(&mut self, side: Side, brief: Brief) {
        self.briefs[side.index()] = brief;
        self.env
            .game
            .set_battle_mana_budget(side, brief.mana_budget);
    }

    /// Play on until a side in `agents` has a decision, or the battle is over (`None`). The
    /// sides not in `agents` give their commands through `opponent`.
    pub fn advance(
        &mut self,
        agents: [bool; 2],
        opponent: &mut dyn FnMut(&Battle, Side) -> BattleCommand,
    ) -> Result<Option<Pending>, String> {
        loop {
            if self.env.done() {
                return Ok(None);
            }
            let Some(side) = self.env.to_play() else {
                return Ok(None);
            };
            let battle = self.env.battle().ok_or("no battle")?;
            if !agents[side.index()] {
                let c = opponent(battle, side);
                self.env.step(c)?;
                continue;
            }
            match next_for_agent(
                &self.env.game,
                self.spec,
                side,
                self.briefs[side.index()],
                &mut self.wizard_turn,
            )? {
                Next::Auto(c) => {
                    self.env.step(c)?;
                }
                Next::Ask(p) => return Ok(Some(*p)),
            }
        }
    }

    /// Answer a unit decision.
    pub fn answer_unit(&mut self, pending: &Pending, index: usize) -> Result<(), String> {
        let Request::Unit { unit, .. } = &pending.request else {
            return Err("not a unit decision".into());
        };
        let c = act::unit_command(index, *unit, &pending.obs).ok_or("no such order")?;
        self.env.step(c)?;
        Ok(())
    }

    /// Answer a wizard decision: `choice` over the wizard vocabulary, then (for a spell) the aim
    /// and the mana bin.
    pub fn answer_wizard(
        &mut self,
        pending: &Pending,
        choice: usize,
        aim: usize,
        mana_bin: usize,
    ) -> Result<(), String> {
        let Request::Wizard { casts, .. } = &pending.request else {
            return Err("not a wizard decision".into());
        };
        match choice {
            act::SKIP => Ok(()),
            act::FLEE => self.env.step(BattleCommand::Flee).map(|_| ()),
            c => {
                let cmd = act::wizard_cast(casts, &pending.obs, c - act::SPELL, aim, mana_bin)
                    .ok_or("no such cast")?;
                self.env.step(cmd).map(|_| ())
            }
        }
    }
}
