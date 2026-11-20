//! What a unit stops or resists: its defence rules (`defence` in `abilities.ron` and `spells.ron`) and
//! the formulas that weigh them against an attack. They follow the original's
//! `Battle_Unit_Defense_Special` and `Combat_Effective_Resistance`.
//!
//! - **What that does to the defence** ([`effective_defense`]): against anything but melee, a
//!   matching immunity makes the defence 50, so nothing gets through. A defence floor (weapon
//!   immunity: at least 10) applies to the type it names, in melee or not. An illusion ignores
//!   the defence altogether unless the defender sees through illusions.
//! - **Resistance** ([`effective_resistance`]): magic immunity adds 30, and some enchantments add to
//!   resistance against particular realms. The die is `f4x-battle`'s `resistance_failure`.
//!
//! Tests: in `f4x-core`'s `battle/immunity/tests.rs`, on the shipped content.

use super::damage_type::{DamageType, DamageTypes, DefenceEffect, DefenceRule};
use super::realm::Realm;
use super::{Ability, Rules, SpellId};

/// What a unit's abilities and lasting spells do against attacks: its defence rules (`defence`
/// in `abilities.ron` and `spells.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Immunities {
    rules: Vec<DefenceRule>,
}

impl Immunities {
    /// The defence rules of a unit's abilities and of the lasting spells on it.
    pub fn of(abilities: &[Ability], spells: &[SpellId], rules: &Rules) -> Immunities {
        let from_abilities = abilities
            .iter()
            .filter_map(|&a| rules.abilities.try_get(a))
            .flat_map(|def| def.defence.iter().copied());
        let from_spells = spells
            .iter()
            .filter_map(|&s| rules.spells.try_get(s))
            .flat_map(|def| def.defence.iter().copied());
        Immunities {
            rules: from_abilities.chain(from_spells).collect(),
        }
    }

    /// A lasting spell put on the unit adds its rules (for a defender built by hand in a test).
    #[cfg(any(test, feature = "testutil"))]
    pub fn add_spell(&mut self, spell: SpellId, rules: &Rules) {
        self.rules
            .extend(rules.spells.get(spell).defence.iter().copied());
    }

    fn matching(&self, attack: DamageTypes) -> impl Iterator<Item = &DefenceRule> {
        self.rules.iter().filter(move |r| r.matches(attack))
    }

    /// Whether the unit has an immunity, a floor or a seeing-through against this type: what
    /// the abilities named after a type (fire immunity) give.
    pub fn has(&self, t: DamageType) -> bool {
        self.rules.iter().any(|r| {
            r.against.contains(t)
                && matches!(
                    r.effect,
                    DefenceEffect::Immune
                        | DefenceEffect::SeeThrough
                        | DefenceEffect::DefenceFloor(_)
                )
        })
    }

    /// Whether an attack of these types does nothing to the unit: it is immune to one of them.
    pub fn stops(&self, attack: DamageTypes) -> bool {
        self.matching(attack)
            .any(|r| r.effect == DefenceEffect::Immune)
    }

    /// The type of the attack the unit is immune to, for the log: the first of the attack's
    /// types that an immunity of the unit matches.
    pub fn stopped_by(&self, attack: DamageTypes) -> Option<DamageType> {
        self.matching(attack)
            .filter(|r| r.effect == DefenceEffect::Immune)
            .find_map(|r| r.against.iter().find(|t| attack.contains(*t)))
    }

    /// The same for what `meets` counts: an immunity, a floor or a seen-through illusion.
    pub fn met_by(&self, attack: DamageTypes) -> Option<DamageType> {
        self.matching(attack)
            .filter(|r| {
                matches!(
                    r.effect,
                    DefenceEffect::Immune
                        | DefenceEffect::SeeThrough
                        | DefenceEffect::DefenceFloor(_)
                )
            })
            .find_map(|r| r.against.iter().find(|t| attack.contains(*t)))
    }

    /// Whether the unit sees through an illusion.
    pub fn sees_through(&self, attack: DamageTypes) -> bool {
        self.matching(attack)
            .any(|r| r.effect == DefenceEffect::SeeThrough)
    }

    /// Whether the unit has a rule against any type of this attack that an immunity, a floor or a
    /// seen-through illusion makes: what a planner treats as a wasted attack, and a spell of
    /// illusions or death is refused by.
    pub fn meets(&self, attack: DamageTypes) -> bool {
        self.matching(attack).any(|r| {
            matches!(
                r.effect,
                DefenceEffect::Immune | DefenceEffect::SeeThrough | DefenceEffect::DefenceFloor(_)
            )
        })
    }

    /// The types of an attack as this unit takes it: an attack that a rule of the unit turns into
    /// doom damage (a sleeper's melee and ranged blows) keeps only the types the rule lists, and
    /// is of the doom type. Other attacks are as they are.
    pub fn convert(&self, attack: DamageTypes) -> DamageTypes {
        self.matching(attack)
            .find_map(|r| match r.effect {
                DefenceEffect::BecomesDoom(keeps) => Some(keeps),
                _ => None,
            })
            .map_or(attack, |keeps| attack.both(keeps).with(DamageType::Doom))
    }

    /// The least defence the unit has against this attack (weapon immunity), if it has one.
    pub fn floor_against(&self, attack: DamageTypes) -> Option<i32> {
        self.matching(attack)
            .filter_map(|r| match r.effect {
                DefenceEffect::DefenceFloor(n) => Some(n),
                _ => None,
            })
            .max()
    }

    /// What is added to the defence against this attack.
    pub fn defence_bonus(&self, attack: DamageTypes) -> i32 {
        self.matching(attack)
            .filter_map(|r| match r.effect {
                DefenceEffect::DefenceBonus(n) => Some(n),
                _ => None,
            })
            .sum()
    }

    /// What is added to the resistance against this attack's spell or effect.
    pub fn resistance_bonus(&self, attack: DamageTypes) -> i32 {
        self.matching(attack)
            .filter_map(|r| match r.effect {
                DefenceEffect::ResistanceBonus(n) => Some(n),
                _ => None,
            })
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[cfg(any(test, feature = "testutil"))]
impl Immunities {
    /// For tests: the defence rules the shipped content gives these abilities (by number).
    pub fn of_numbers(numbers: &[u16]) -> Immunities {
        Immunities::of_numbers_and_spells(numbers, &[])
    }

    /// For tests: the same with lasting spells on the unit.
    pub fn of_numbers_and_spells(numbers: &[u16], spells: &[SpellId]) -> Immunities {
        let abilities: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
        Immunities::of(&abilities, spells, &crate::testdir::shipped().rules)
    }
}

/// The defence a unit rolls against one attack, or `None` if the unit is immune to it: the
/// attack does nothing. `types` are the types of the attack with its delivery.
///
/// An illusion the defender cannot see through ignores every armour. Armour piercing halves the
/// defence (rounding down); a floor (weapon immunity) then raises it.
pub fn effective_defense(
    defense: i32,
    unit: &Immunities,
    types: DamageTypes,
    armor_piercing: bool,
) -> Option<i32> {
    if unit.stops(types) {
        return None;
    }
    if types.contains(DamageType::Illusion) && !unit.sees_through(types) {
        return Some(0);
    }
    let defense = if armor_piercing { defense / 2 } else { defense };
    Some(
        unit.floor_against(types)
            .map_or(defense, |f| defense.max(f)),
    )
}

/// A unit's resistance against a spell or attack of `realm`: its own, 30 more with magic
/// immunity against any realm, and what its lasting spells add against that realm
/// (`resistance_bonus` of their `defence`).
pub fn effective_resistance(resistance: i32, unit: &Immunities, realm: Option<Realm>) -> i32 {
    let Some(realm) = realm else {
        return resistance;
    };
    let magic = if unit.has(DamageType::Magic) { 30 } else { 0 };
    resistance + magic + unit.resistance_bonus(DamageTypes::of(&[realm.damage_type()]))
}
