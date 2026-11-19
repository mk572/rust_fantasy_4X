//! Damage types: what an attack is made of, and the defence rules that key on them.
//!
//! An attack carries a [`DamageTypes`] set (a Fire Bolt is fire, chaos realm and magic). An
//! ability gives its unit [`DefenceRule`]s (`defence` in `abilities.ron`), each matching one type,
//! and the attacks it makes ([`AbilityAttack`]).

use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// One damage type. The realm types are the original's realms (`realms.ron`); `Death` is the
/// kind that kills or frightens (what `death_immunity` stops), not the death realm.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, EnumIter,
)]
pub enum DamageType {
    /// A normal (not magical) weapon.
    Weapon,
    /// An arrow or a sling stone.
    Missile,
    /// Spells and special ranged attacks (bolts, breath, gazes): what magic immunity stops.
    Magic,
    Fire,
    Cold,
    Lightning,
    Poison,
    Stoning,
    Death,
    /// Ignores the defence of a defender that cannot see through illusions.
    Illusion,
    NatureRealm,
    SorceryRealm,
    ChaosRealm,
    LifeRealm,
    DeathRealm,
    ArcaneRealm,
}

impl DamageType {
    fn bit(self) -> u32 {
        1 << self as u32
    }

    /// The type of the realm with this number in `realms.ron`.
    pub fn of_realm(num: u16) -> Option<DamageType> {
        Some(match num {
            0 => DamageType::NatureRealm,
            1 => DamageType::SorceryRealm,
            2 => DamageType::ChaosRealm,
            3 => DamageType::LifeRealm,
            4 => DamageType::DeathRealm,
            5 => DamageType::ArcaneRealm,
            _ => return None,
        })
    }

    /// The number of the realm in `realms.ron`, if this is a realm type.
    pub fn realm_num(self) -> Option<u16> {
        Some(match self {
            DamageType::NatureRealm => 0,
            DamageType::SorceryRealm => 1,
            DamageType::ChaosRealm => 2,
            DamageType::LifeRealm => 3,
            DamageType::DeathRealm => 4,
            DamageType::ArcaneRealm => 5,
            DamageType::Weapon
            | DamageType::Missile
            | DamageType::Magic
            | DamageType::Fire
            | DamageType::Cold
            | DamageType::Lightning
            | DamageType::Poison
            | DamageType::Stoning
            | DamageType::Death
            | DamageType::Illusion => return None,
        })
    }
}

/// A set of damage types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DamageTypes(u32);

impl DamageTypes {
    pub const NONE: DamageTypes = DamageTypes(0);

    pub fn of(types: &[DamageType]) -> DamageTypes {
        types.iter().fold(DamageTypes::NONE, |set, &t| set.with(t))
    }

    pub fn with(self, t: DamageType) -> DamageTypes {
        DamageTypes(self.0 | t.bit())
    }

    pub fn without(self, t: DamageType) -> DamageTypes {
        DamageTypes(self.0 & !t.bit())
    }

    /// The types in this set that are not in `other`.
    pub fn minus(self, other: DamageTypes) -> DamageTypes {
        DamageTypes(self.0 & !other.0)
    }

    pub fn insert(&mut self, t: DamageType) {
        *self = self.with(t);
    }

    pub fn union(&mut self, other: DamageTypes) {
        self.0 |= other.0;
    }

    pub fn contains(self, t: DamageType) -> bool {
        self.0 & t.bit() != 0
    }

    /// Whether the two sets have a type in common.
    pub fn intersects(self, other: DamageTypes) -> bool {
        self.0 & other.0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The realm type in the set, as its number in `realms.ron` (the first, if there are several).
    pub fn realm_num(self) -> Option<u16> {
        use strum::IntoEnumIterator;
        DamageType::iter()
            .filter(|&t| self.contains(t))
            .find_map(DamageType::realm_num)
    }
}

/// What a defence rule does to a part of an attack of the type it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefenceEffect {
    /// The part does nothing.
    Immune,
    /// The defence is at least this much against the part.
    DefenceFloor(i32),
}

/// What an ability does against one damage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefenceRule {
    pub against: DamageType,
    pub effect: DefenceEffect,
}

/// How an ability's attack reaches its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Delivery {
    /// Every attack of the unit.
    #[default]
    Any,
    /// Its ranged, thrown or breath attack only.
    Ranged,
}

/// The damage an ability adds to the attack of its unit (`attack` in `abilities.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AbilityAttack {
    pub delivery: Delivery,
    pub types: Vec<DamageType>,
    /// The attack halves the defence it meets.
    pub armor_piercing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn a_set_holds_what_was_put_in() {
        let set = DamageTypes::of(&[DamageType::Fire, DamageType::ChaosRealm]);
        assert!(set.contains(DamageType::Fire));
        assert!(set.contains(DamageType::ChaosRealm));
        assert!(!set.contains(DamageType::Cold));
        assert!(!set.without(DamageType::Fire).contains(DamageType::Fire));
        assert!(set.intersects(DamageTypes::of(&[DamageType::Fire, DamageType::Cold])));
        assert!(!set.intersects(DamageTypes::of(&[DamageType::Cold])));
        assert!(DamageTypes::NONE.is_empty());
        assert_eq!(
            set.minus(DamageTypes::of(&[DamageType::Fire])),
            DamageTypes::of(&[DamageType::ChaosRealm])
        );
    }

    #[test]
    fn every_type_has_its_own_bit() {
        let all = DamageType::iter().fold(DamageTypes::NONE, |s, t| s.with(t));
        assert_eq!(
            all.0.count_ones() as usize,
            DamageType::iter().count(),
            "two types share a bit"
        );
    }

    #[test]
    fn realm_types_round_trip_through_the_realm_number() {
        for t in DamageType::iter() {
            if let Some(num) = t.realm_num() {
                assert_eq!(DamageType::of_realm(num), Some(t));
            }
        }
        assert_eq!(
            DamageTypes::of(&[DamageType::Fire, DamageType::ChaosRealm]).realm_num(),
            Some(2)
        );
        assert_eq!(DamageTypes::of(&[DamageType::Fire]).realm_num(), None);
    }
}
