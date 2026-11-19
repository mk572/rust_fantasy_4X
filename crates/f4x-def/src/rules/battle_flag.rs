//! Facts about a unit in a battle that its abilities set (`flags` in `abilities.ron`). The battle
//! reads them from the unit's [`BattleFlags`], not from the numbers of abilities.

use serde::{Deserialize, Serialize};
use strum::EnumIter;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, EnumIter,
)]
pub enum BattleFlag {
    /// Strikes before the defender can answer.
    FirstStrike,
    /// Cancels the enemy's first strike.
    NegatesFirstStrike,
    /// Its ranged attacks reach farther without a penalty.
    LongRange,
    /// Its normal attacks count as magical: weapon immunity does not stop them.
    MagicalWeapon,
    /// Moves over any terrain and over the walls.
    Flies,
    /// Takes away flight (a web, sleep).
    Grounds,
    /// Passes through walls and is not slowed by mud.
    NonCorporeal,
    /// Goes to any free square for one move.
    Teleports,
}

impl BattleFlag {
    fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of battle flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct BattleFlags(u16);

impl BattleFlags {
    pub const NONE: BattleFlags = BattleFlags(0);

    pub fn of(flags: &[BattleFlag]) -> BattleFlags {
        flags.iter().fold(BattleFlags::NONE, |s, &f| s.with(f))
    }

    pub fn with(self, f: BattleFlag) -> BattleFlags {
        BattleFlags(self.0 | f.bit())
    }

    pub fn without(self, f: BattleFlag) -> BattleFlags {
        BattleFlags(self.0 & !f.bit())
    }

    pub fn contains(self, f: BattleFlag) -> bool {
        self.0 & f.bit() != 0
    }

    pub fn insert(&mut self, f: BattleFlag) {
        *self = self.with(f);
    }

    pub fn union(&mut self, other: BattleFlags) {
        self.0 |= other.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn a_set_holds_what_was_put_in() {
        let set = BattleFlags::of(&[BattleFlag::Flies, BattleFlag::LongRange]);
        assert!(set.contains(BattleFlag::Flies) && set.contains(BattleFlag::LongRange));
        assert!(!set.contains(BattleFlag::Teleports));
        assert!(!set.without(BattleFlag::Flies).contains(BattleFlag::Flies));
    }

    #[test]
    fn every_flag_has_its_own_bit() {
        let all = BattleFlag::iter().fold(BattleFlags::NONE, |s, f| s.with(f));
        assert_eq!(all.0.count_ones() as usize, BattleFlag::iter().count());
    }
}
