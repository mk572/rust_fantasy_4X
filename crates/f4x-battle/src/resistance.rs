//! The dice of resistance and dispelling.

use f4x_def::rules::BattleRules;
use rand::{Rng, RngCore};

/// A resistance check: roll the resistance die (`resistance_die`) against the resistance plus the
/// spell's modifier (negative for a spell that is hard to resist). Returns by how much the roll beat
/// the resistance, so 0 means the target resisted.
pub fn resistance_failure(
    rules: &BattleRules,
    resistance: i32,
    modifier: i32,
    rng: &mut dyn RngCore,
) -> i32 {
    let roll = rng.gen_range(1..=rules.resistance_die.max(1));
    (roll - (resistance + modifier)).max(0)
}

/// Whether a dispel of `strength` beats something of `difficulty`: a roll of 1 to `dispel_die` at or
/// under `dispel_die * strength / (strength + difficulty)`.
pub fn dispel_roll(
    rules: &BattleRules,
    strength: u32,
    difficulty: u32,
    rng: &mut dyn RngCore,
) -> bool {
    if strength == 0 {
        return false;
    }
    let die = u64::from(rules.dispel_die.max(1));
    let chance = die * u64::from(strength) / u64::from(strength + difficulty);
    rng.next_u64() % die < chance
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::GameRng;

    #[test]
    fn the_failure_is_the_margin_by_which_the_roll_beat_the_resistance() {
        let rules = &f4x_def::testdir::shipped().rules.battle;
        let die = rules.resistance_die;
        let mut rng = GameRng::new(3);
        for resistance in 0..die {
            for _ in 0..200 {
                let failure = resistance_failure(rules, resistance, 0, &mut rng);
                assert!((0..=die - resistance).contains(&failure), "{failure}");
            }
        }
        assert_eq!(resistance_failure(rules, die, 0, &mut rng), 0);
        assert_eq!(resistance_failure(rules, 0, -die / 2, &mut rng).min(1), 1);
    }
}
