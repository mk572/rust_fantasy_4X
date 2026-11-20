//! The dice of resistance and dispelling.

use rand::{Rng, RngCore};

/// A resistance check: roll a ten-sided die against the resistance plus the spell's modifier
/// (negative for a spell that is hard to resist). Returns by how much the roll beat the
/// resistance, so 0 means the target resisted.
pub fn resistance_failure(resistance: i32, modifier: i32, rng: &mut dyn RngCore) -> i32 {
    let roll = rng.gen_range(1..=10);
    (roll - (resistance + modifier)).max(0)
}

/// Whether a dispel of `strength` beats something of `difficulty`: a roll of 1 to 250 at or
/// under `250 * strength / (strength + difficulty)`.
pub fn dispel_roll(strength: u32, difficulty: u32, rng: &mut dyn RngCore) -> bool {
    if strength == 0 {
        return false;
    }
    let chance = 250 * u64::from(strength) / u64::from(strength + difficulty);
    rng.next_u64() % 250 < chance
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
