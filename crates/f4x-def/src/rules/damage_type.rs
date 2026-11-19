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
    /// A normal (not magical) weapon: what weapon immunity is a floor against, and what a
    /// magical weapon is not.
    MundaneWeapon,
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
    /// How the damage reaches its target: in a melee blow, or from a distance (a shot, a thrown
    /// weapon, breath, a gaze, a spell). Every attack has one; it is added where the attack is
    /// made.
    Melee,
    Ranged,
    NatureRealm,
    SorceryRealm,
    ChaosRealm,
    LifeRealm,
    DeathRealm,
    ArcaneRealm,
    /// Doom damage: every die hits and the target rolls no defence. An attack is of this type
    /// from its source (a doom gaze, a Chaos weapon, a Doom Bolt) or from a sleeping target
    /// (`DefenceEffect::BecomesDoom`). Only a nullifying immunity (magic immunity, Righteousness)
    /// still stops it.
    Doom,
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
            DamageType::MundaneWeapon
            | DamageType::Missile
            | DamageType::Magic
            | DamageType::Fire
            | DamageType::Cold
            | DamageType::Lightning
            | DamageType::Poison
            | DamageType::Stoning
            | DamageType::Death
            | DamageType::Illusion
            | DamageType::Melee
            | DamageType::Ranged
            | DamageType::Doom => return None,
        })
    }
}

/// A set of damage types. Saved and written in the content as the list of its types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "Vec<DamageType>", into = "Vec<DamageType>")]
pub struct DamageTypes(u32);

impl From<Vec<DamageType>> for DamageTypes {
    fn from(types: Vec<DamageType>) -> Self {
        DamageTypes::of(&types)
    }
}

impl From<DamageTypes> for Vec<DamageType> {
    fn from(set: DamageTypes) -> Self {
        set.iter().collect()
    }
}

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

    /// The types in both sets.
    pub fn both(self, other: DamageTypes) -> DamageTypes {
        DamageTypes(self.0 & other.0)
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

    /// The types in the set, in the order of [`DamageType`].
    pub fn iter(self) -> impl Iterator<Item = DamageType> {
        use strum::IntoEnumIterator;
        DamageType::iter().filter(move |&t| self.contains(t))
    }

    /// Whether every type of `other` is in this set.
    pub fn contains_all(self, other: DamageTypes) -> bool {
        self.0 & other.0 == other.0
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
        self.iter().find_map(DamageType::realm_num)
    }
}

/// What a defence rule does to the attacks it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefenceEffect {
    /// The attack does no damage.
    Immune,
    /// The defender is not fooled: an illusion does not ignore its defence, and a spell of
    /// illusions is refused.
    SeeThrough,
    /// The defence is at least this much against the attack.
    DefenceFloor(i32),
    /// This much is added to the defence against the attack.
    DefenceBonus(i32),
    /// This much is added to the resistance against the attack's spell or effect.
    ResistanceBonus(i32),
    /// The attack becomes doom damage: its types are cut to the ones listed (the types that a
    /// nullifying immunity stops: magic, the realms of Righteousness) and Doom. A target asleep
    /// under Black Sleep takes every melee and ranged blow so.
    BecomesDoom(DamageTypes),
}

/// What an ability or a lasting spell does against some attacks (`defence` in `abilities.ron` and
/// `spells.ron`). It matches an attack that has any of the types in `against` (every attack when
/// that is empty) and all of the types in `with`: Elemental Armor is against chaos or nature,
/// a large shield with ranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefenceRule {
    #[serde(default)]
    pub against: DamageTypes,
    #[serde(default)]
    pub with: DamageTypes,
    pub effect: DefenceEffect,
}

impl DefenceRule {
    /// Whether the rule applies to an attack made of these types.
    pub fn matches(&self, attack: DamageTypes) -> bool {
        (self.against.is_empty() || self.against.intersects(attack))
            && attack.contains_all(self.with)
    }
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

/// How a unit's ranged attack works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RangedKind {
    /// No ranged attack.
    #[default]
    None,
    /// Arrows, slings and boulders: fired at a distance with a penalty, using ammunition.
    Missile,
    /// Spell shots: no range penalty, using ammunition (mana).
    Magic,
    /// Thrown weapons and breath: used just before melee, no ammunition.
    Short,
}

/// The damage an ability adds to the attack of its unit (`attack` in `abilities.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AbilityAttack {
    pub delivery: Delivery,
    /// The ranged attack the ability gives the unit (none for an ability that only adds to it).
    pub kind: RangedKind,
    /// The damage is the unit's own bodily strength (a thrown boulder), not a weapon's: a web
    /// takes it as it takes a melee blow.
    pub bodily: bool,
    pub types: Vec<DamageType>,
    /// The attack halves the defence it meets.
    pub armor_piercing: bool,
    /// One of the original's exceptions to magic immunity: with the rule setting
    /// `magic_immunity_exceptions` the unit's attack is not of the magic type (illusion, the
    /// Chaos power, the Doom Gaze).
    pub magic_immunity_exception: bool,
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
    fn a_set_is_saved_as_its_list_of_types() {
        let set = DamageTypes::of(&[DamageType::ChaosRealm, DamageType::Fire]);
        let text = ron::to_string(&set).expect("a set serialises");
        assert_eq!(text, "[Fire,ChaosRealm]");
        assert_eq!(
            ron::from_str::<DamageTypes>(&text).expect("it reads back"),
            set
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
