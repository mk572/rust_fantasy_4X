//! The resistance check of the rules: one ten-sided die against a resistance.

use rand::{Rng, RngCore};

/// A resistance check: roll a ten-sided die against the resistance plus the spell's modifier
/// (negative for a spell that is hard to resist). Returns by how much the roll beat the
/// resistance, so 0 means the target resisted.
pub fn resistance_failure(resistance: i32, modifier: i32, rng: &mut dyn RngCore) -> i32 {
    let roll = rng.gen_range(1..=10);
    (roll - (resistance + modifier)).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::GameRng;

    #[test]
    fn the_failure_is_the_margin_by_which_the_roll_beat_the_resistance() {
        let mut rng = GameRng::new(3);
        for _ in 0..200 {
            let failure = resistance_failure(4, 0, &mut rng);
            assert!((0..=6).contains(&failure));
        }
        assert_eq!(resistance_failure(10, 0, &mut rng), 0);
        assert_eq!(resistance_failure(0, -5, &mut rng).min(1), 1);
    }
}
