//! The battle agent's action space (`docs/learning-ai.md` 7.3), as fixed vocabularies with masks.
//!
//! A battle is played as a sequence of decisions of two kinds:
//!
//! - a **unit decision**: the unit whose turn it is (the one the scripted AI would move next, see
//!   [`crate::agent`]) picks one of [`UNIT_VOCAB`] orders: walk to a square, strike or shoot an
//!   enemy (by its token), strike or shoot a wall section, or be done;
//! - a **wizard decision**, once at the start of each of the side's turns when the wizard has
//!   something to decide: cast nothing, flee, or a spell, then its aim and its mana.
//!
//! Every choice is masked by what [`f4x_core::battle::Battle::legal_commands`] and
//! [`f4x_core::game::Game::legal_battle_casts`] allow, so the agent cannot give a refused order.

use f4x_core::battle::map::{GRID_H, GRID_W};
use f4x_core::battle::{BattleCommand, SpellAim};
use f4x_core::rules::SpellId;

use crate::obs::{Observation, MAX_SPELLS, MAX_UNITS};

/// Squares of the battle field.
pub const CELLS: usize = (GRID_W * GRID_H) as usize;

pub const MOVE: usize = 0;
pub const ATTACK: usize = MOVE + CELLS;
pub const SHOOT: usize = ATTACK + MAX_UNITS;
pub const WALL_ATTACK: usize = SHOOT + MAX_UNITS;
pub const WALL_SHOOT: usize = WALL_ATTACK + CELLS;
pub const DONE: usize = WALL_SHOOT + CELLS;
/// Orders a unit can give.
pub const UNIT_VOCAB: usize = DONE + 1;

/// Wizard choice: nothing, flee, or spell slot `k` as `SPELL + k`.
pub const SKIP: usize = 0;
pub const FLEE: usize = 1;
pub const SPELL: usize = 2;
pub const WIZARD_VOCAB: usize = SPELL + MAX_SPELLS;

/// A spell's aim: none, a unit token, or a square.
pub const AIM_NONE: usize = 0;
pub const AIM_UNIT: usize = 1;
pub const AIM_CELL: usize = AIM_UNIT + MAX_UNITS;
pub const AIM_VOCAB: usize = AIM_CELL + CELLS;

/// Mana: the least the spell takes, or the most.
pub const MANA_VOCAB: usize = 2;

pub fn cell_index((x, y): (i32, i32)) -> usize {
    (y * GRID_W + x) as usize
}

pub fn cell_of(i: usize) -> (i32, i32) {
    ((i as i32) % GRID_W, (i as i32) / GRID_W)
}

/// The token of a battle unit in the observation.
fn slot_of(obs: &Observation, unit: usize) -> Option<usize> {
    (0..MAX_UNITS).find(|&t| obs.unit_mask[t] && obs.unit_ids[t] == unit)
}

/// The index of a command in the unit vocabulary, if it is one a unit gives.
pub fn unit_index(c: &BattleCommand, obs: &Observation) -> Option<usize> {
    Some(match c {
        BattleCommand::Move { to, .. } => MOVE + cell_index(*to),
        BattleCommand::Attack { target, .. } => ATTACK + slot_of(obs, *target)?,
        BattleCommand::Shoot { target, .. } => SHOOT + slot_of(obs, *target)?,
        BattleCommand::AttackWall { cell, .. } => WALL_ATTACK + cell_index(*cell),
        BattleCommand::ShootWall { cell, .. } => WALL_SHOOT + cell_index(*cell),
        BattleCommand::Done { .. } => DONE,
        _ => return None,
    })
}

/// The command for unit `unit` that an index of the unit vocabulary stands for.
pub fn unit_command(index: usize, unit: usize, obs: &Observation) -> Option<BattleCommand> {
    let slot = |i: usize| obs.unit_ids.get(i).copied().filter(|_| obs.unit_mask[i]);
    Some(match index {
        i if i < ATTACK => BattleCommand::Move {
            unit,
            to: cell_of(i - MOVE),
        },
        i if i < SHOOT => BattleCommand::Attack {
            unit,
            target: slot(i - ATTACK)?,
        },
        i if i < WALL_ATTACK => BattleCommand::Shoot {
            unit,
            target: slot(i - SHOOT)?,
        },
        i if i < WALL_SHOOT => BattleCommand::AttackWall {
            unit,
            cell: cell_of(i - WALL_ATTACK),
        },
        i if i < DONE => BattleCommand::ShootWall {
            unit,
            cell: cell_of(i - WALL_SHOOT),
        },
        DONE => BattleCommand::Done { unit },
        _ => return None,
    })
}

/// Which orders of the unit vocabulary `unit` may give now.
pub fn unit_mask(legal: &[BattleCommand], unit: usize, obs: &Observation) -> Vec<bool> {
    let mut mask = vec![false; UNIT_VOCAB];
    for c in legal {
        let of_unit = match c {
            BattleCommand::Move { unit: u, .. }
            | BattleCommand::Attack { unit: u, .. }
            | BattleCommand::Shoot { unit: u, .. }
            | BattleCommand::AttackWall { unit: u, .. }
            | BattleCommand::ShootWall { unit: u, .. }
            | BattleCommand::Done { unit: u } => *u == unit,
            _ => false,
        };
        if of_unit {
            if let Some(i) = unit_index(c, obs) {
                mask[i] = true;
            }
        }
    }
    mask
}

/// The index of an aim in the aim vocabulary.
pub fn aim_index(aim: SpellAim, obs: &Observation) -> Option<usize> {
    Some(match aim {
        SpellAim::None => AIM_NONE,
        SpellAim::Unit(u) => AIM_UNIT + slot_of(obs, u)?,
        SpellAim::Tile(c) => AIM_CELL + cell_index(c),
    })
}

pub fn aim_of(index: usize, obs: &Observation) -> Option<SpellAim> {
    Some(match index {
        AIM_NONE => SpellAim::None,
        i if i < AIM_CELL => {
            let t = i - AIM_UNIT;
            SpellAim::Unit(obs.unit_ids.get(t).copied().filter(|_| obs.unit_mask[t])?)
        }
        i if i < AIM_VOCAB => SpellAim::Tile(cell_of(i - AIM_CELL)),
        _ => return None,
    })
}

/// The spell slot of a spell in the observation.
pub fn spell_slot(obs: &Observation, spell: SpellId) -> Option<usize> {
    obs.spell_ids.iter().position(|&s| s == spell)
}

/// What the wizard may decide now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardOptions {
    /// Over [`WIZARD_VOCAB`].
    pub choice: Vec<bool>,
    /// For each spell slot, the aims it takes (over [`AIM_VOCAB`]) and whether more than the
    /// least mana can be put in.
    pub aims: Vec<Vec<bool>>,
    pub more_mana: Vec<bool>,
}

impl WizardOptions {
    /// Whether there is anything to decide besides skipping.
    pub fn any(&self) -> bool {
        self.choice.iter().skip(1).any(|&b| b)
    }
}

pub fn wizard_options(
    casts: &[f4x_core::battle::spell::LegalCast],
    may_flee: bool,
    obs: &Observation,
) -> WizardOptions {
    let mut choice = vec![false; WIZARD_VOCAB];
    choice[SKIP] = true;
    choice[FLEE] = may_flee;
    let mut aims = vec![Vec::new(); MAX_SPELLS];
    let mut more_mana = vec![false; MAX_SPELLS];
    for lc in casts {
        let Some(k) = spell_slot(obs, lc.spell) else {
            continue;
        };
        let mut mask = vec![false; AIM_VOCAB];
        for &a in &lc.aims {
            if let Some(i) = aim_index(a, obs) {
                mask[i] = true;
            }
        }
        if mask.iter().any(|&b| b) {
            choice[SPELL + k] = true;
            aims[k] = mask;
            more_mana[k] = lc.most_mana > lc.least_mana;
        }
    }
    WizardOptions {
        choice,
        aims,
        more_mana,
    }
}

/// The spell cast, if the choice is one: its id, from the slot.
pub fn wizard_cast(
    casts: &[f4x_core::battle::spell::LegalCast],
    obs: &Observation,
    slot: usize,
    aim: usize,
    mana_bin: usize,
) -> Option<BattleCommand> {
    let spell = *obs.spell_ids.get(slot)?;
    let lc = casts.iter().find(|c| c.spell == spell)?;
    Some(BattleCommand::Cast {
        spell,
        aim: aim_of(aim, obs)?,
        mana: if mana_bin == 1 {
            lc.most_mana
        } else {
            lc.least_mana
        },
    })
}
