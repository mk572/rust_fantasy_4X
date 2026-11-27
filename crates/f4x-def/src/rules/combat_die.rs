//! The die a blow and a block roll (`combat_die` of `battle.ron`).
//!
//! Tests: at the end of this file.

use serde::{Deserialize, Serialize};

/// The die a blow and a block roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CombatDie {
    /// The sides, numbered from 1.
    pub sides: i32,
    /// The lowest side that succeeds without a plus.
    pub success_from: i32,
}

impl CombatDie {
    /// The lowest side that succeeds with `pluses`: each lowers it by one, and the highest side
    /// always succeeds.
    pub fn lowest_success(&self, pluses: i32) -> i32 {
        self.success_from
            .saturating_sub(pluses)
            .min(self.sides)
            .max(1)
    }

    /// The odds that one die succeeds with `pluses`.
    pub fn odds(&self, pluses: i32) -> f32 {
        let good = self.sides - self.lowest_success(pluses) + 1;
        good.max(0) as f32 / self.sides.max(1) as f32
    }
}

#[cfg(test)]
mod tests {
    fn shipped() -> crate::rules::BattleRules {
        crate::testdir::shipped().rules.battle.clone()
    }

    #[test]
    fn each_plus_makes_one_more_side_succeed_until_all_do_or_only_the_highest_does() {
        let die = shipped().combat_die;
        let one_side = 1.0 / die.sides as f32;
        // The highest side is always left, and no more than every side succeeds.
        assert_eq!(die.odds(i32::MIN), one_side);
        assert_eq!(die.odds(i32::MAX), 1.0);
        for pluses in -die.sides..die.sides {
            let lowest = die.lowest_success(pluses);
            let good = (1..=die.sides).filter(|&side| side >= lowest).count();
            let share = good as f32 / die.sides as f32;
            assert_eq!(die.odds(pluses), share, "{pluses} pluses");
            let moved = die.lowest_success(pluses) - die.lowest_success(pluses + 1);
            let free = lowest > 1 && die.success_from - pluses <= die.sides;
            assert_eq!(moved, i32::from(free), "{pluses} pluses");
        }
        assert_eq!(die.lowest_success(0), die.success_from);
    }

    #[test]
    fn armour_piercing_leaves_the_percent_of_the_defence_rounded_down() {
        let rules = shipped();
        for defense in 0..20 {
            let left = defense * rules.armor_piercing_defense_percent / 100;
            assert_eq!(rules.pierced(defense), left, "defence {defense}");
        }
    }
}
