//! The learned battle player for the game (task 171): it plays the battles of AI wizards.
//!
//! `LearnedBrain::load` reads a checkpoint written by training; `Game::set_battle_brain` plugs it
//! in. Without a checkpoint nothing changes: the game plays battles with the plain AI.

use std::path::Path;

use burn::prelude::*;
use f4x_core::battle::game::BattleBrain;
use f4x_core::battle::BattleCommand;
use f4x_core::content::Content;
use f4x_core::game::Game;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

use crate::act;
use crate::agent::{next_for_agent, Next, Request};
use crate::net::PolicyNet;
use crate::obs::{Brief, ObsSpec};
use crate::train::choose;

pub struct LearnedBrain {
    net: PolicyNet,
    spec: ObsSpec,
    device: Device,
    /// What the strategic AI asks of the battle: the default is to win with some care and to
    /// be allowed to flee.
    pub brief: Brief,
    wizard_turn: [u32; 2],
    rng: Pcg64Mcg,
    /// Show the network the scripted AI's advice (the network was trained both ways).
    pub advice: bool,
}

impl LearnedBrain {
    pub fn load(content: &Content, checkpoint: &Path) -> Result<Self, String> {
        let spec = ObsSpec::new(content);
        let device = Device::flex();
        let net = crate::net::load_checkpoint(&spec, checkpoint, &device)?;
        Ok(Self {
            net,
            spec,
            device,
            brief: Brief {
                stakes: 0.5,
                may_flee: true,
                ..Brief::default()
            },
            wizard_turn: [0; 2],
            rng: Pcg64Mcg::seed_from_u64(7),
            advice: false,
        })
    }
}

impl BattleBrain for LearnedBrain {
    fn command(&mut self, game: &Game) -> BattleCommand {
        let fallback = || {
            game.battle()
                .map_or(BattleCommand::EndTurn, crate::policy::scripted_command)
        };
        let Some(side) = game.battle().map(|b| b.active) else {
            return BattleCommand::EndTurn;
        };
        loop {
            let Ok(next) =
                next_for_agent(game, &self.spec, side, self.brief, &mut self.wizard_turn)
            else {
                return fallback();
            };
            let p = match next {
                Next::Auto(c) => return c,
                Next::Ask(p) => *p,
            };
            let c = choose(
                &self.net,
                &self.spec,
                &self.device,
                &[&p],
                &[self.advice],
                true,
                &mut self.rng,
            )[0];
            match &p.request {
                Request::Unit { unit, .. } => {
                    return act::unit_command(c.index, *unit, &p.obs).unwrap_or_else(fallback);
                }
                Request::Wizard { casts, .. } => match c.index {
                    act::SKIP => continue,
                    act::FLEE => return BattleCommand::Flee,
                    i => {
                        return act::wizard_cast(casts, &p.obs, i - act::SPELL, c.aim, c.mana_bin)
                            .unwrap_or_else(fallback);
                    }
                },
            }
        }
    }
}
