//! Facts about a unit that its abilities set (`flags` in `abilities.ron`): how it fights and how
//! it moves. The game asks for a flag, not for the number of an ability, so a rule that belongs
//! to an ability is data on it. A battle unit holds its [`UnitFlags`].

use serde::{Deserialize, Serialize};
use strum::EnumIter;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, EnumIter,
)]
pub enum UnitFlag {
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
    /// Walks on water.
    WaterWalking,
    /// Crosses any land at road speed, and takes its whole stack with it.
    Pathfinding,
    /// Crosses forests for one move, and takes its whole stack with it.
    Forester,
    /// Crosses hills and mountains for one move, and takes its whole stack with it.
    Mountaineer,
    /// Carries its whole stack through the air.
    WindWalking,
    /// Takes its stack to the other plane without a tower.
    PlanarTravel,
}

impl UnitFlag {
    fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of unit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct UnitFlags(u16);

impl UnitFlags {
    pub const NONE: UnitFlags = UnitFlags(0);

    pub fn of(flags: &[UnitFlag]) -> UnitFlags {
        flags.iter().fold(UnitFlags::NONE, |s, &f| s.with(f))
    }

    pub fn with(self, f: UnitFlag) -> UnitFlags {
        UnitFlags(self.0 | f.bit())
    }

    pub fn without(self, f: UnitFlag) -> UnitFlags {
        UnitFlags(self.0 & !f.bit())
    }

    pub fn contains(self, f: UnitFlag) -> bool {
        self.0 & f.bit() != 0
    }

    pub fn insert(&mut self, f: UnitFlag) {
        *self = self.with(f);
    }

    pub fn union(&mut self, other: UnitFlags) {
        self.0 |= other.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn a_set_holds_what_was_put_in() {
        let set = UnitFlags::of(&[UnitFlag::Flies, UnitFlag::LongRange]);
        assert!(set.contains(UnitFlag::Flies) && set.contains(UnitFlag::LongRange));
        assert!(!set.contains(UnitFlag::Teleports));
        assert!(!set.without(UnitFlag::Flies).contains(UnitFlag::Flies));
    }

    #[test]
    fn every_flag_has_its_own_bit() {
        let all = UnitFlag::iter().fold(UnitFlags::NONE, |s, f| s.with(f));
        assert_eq!(all.0.count_ones() as usize, UnitFlag::iter().count());
    }
}
