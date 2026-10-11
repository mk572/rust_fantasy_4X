//! A bank of scenarios whose odds were measured, not guessed.
//!
//! The strength of an army is hard to read off its units (flying, ranged attacks, immunities,
//! walls and spells all matter), so the odds of a scenario are measured by letting the scripted
//! AI play it a few times on both sides (with different dice). The scripted AI is only the
//! yardstick here: a scenario is `Overwhelming` when the attacker wins every time, `Overwhelmed`
//! when it never wins, and `Even` in between. A bank is written once and trained from.

use f4x_core::battle::Side;
use f4x_core::content::Content;

use crate::env::BattleEnv;
use crate::policy;
use crate::scenario::{BattleScenario, Odds};

/// How many times a scenario is played to measure its odds.
pub const SIMS: u64 = 4;

/// The attacker's wins out of [`SIMS`] battles of the scenario with different dice, or `None`
/// if it cannot be built.
pub fn calibrate(content: &Content, scenario: &BattleScenario) -> Option<u32> {
    let mut wins = 0;
    for k in 0..SIMS {
        let mut s = scenario.clone();
        s.seed = scenario.seed.wrapping_add(k * 1_000_003);
        let mut env = BattleEnv::new(content, s).ok()?;
        while !env.done() {
            let c = policy::scripted_command(env.battle()?);
            env.step(c).ok()?;
        }
        wins += u32::from(env.outcome.winner == Some(Side::Attacker));
    }
    Some(wins)
}

/// The class of a scenario from the attacker's wins.
pub fn class(wins: u32) -> Odds {
    match wins {
        0 => Odds::Overwhelmed,
        w if u64::from(w) == SIMS => Odds::Overwhelming,
        _ => Odds::Even,
    }
}

/// Scenarios by class.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Bank {
    pub scenarios: Vec<BattleScenario>,
}

impl Bank {
    pub fn to_text(&self) -> String {
        ron::ser::to_string(self).unwrap_or_default()
    }

    pub fn from_text(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    /// The scenario for a draw `k`: scenarios are taken in turn by class (even, overwhelming,
    /// overwhelmed) so every class is seen equally often, whatever the bank holds.
    pub fn pick(&self, k: u64) -> BattleScenario {
        let class = [Odds::Even, Odds::Overwhelming, Odds::Overwhelmed][(k % 3) as usize];
        let of: Vec<&BattleScenario> = self.scenarios.iter().filter(|s| s.odds == class).collect();
        let pool: Vec<&BattleScenario> = if of.is_empty() {
            self.scenarios.iter().collect()
        } else {
            of
        };
        let mut x = k / 3 + 0x9e37_79b9;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        pool[(x % pool.len() as u64) as usize].clone()
    }
}
