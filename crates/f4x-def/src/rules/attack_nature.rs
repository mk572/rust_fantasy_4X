//! What an attack is made of: how it is delivered and its damage types, from the attack abilities of
//! the unit (`attack` in `abilities.ron`). They follow the original's `Battle_Unit_Attack_Immunities`.
//!
//! A *normal weapon* is any weapon that is not magical. Heroes, fantastic and summoned creatures and
//! units with a flaming, blessed or similar weapon have magical weapons and ignore weapon immunity
//! (`Game::unit_has_magical_weapon`).
//!
//! Tests: in `f4x-core`'s `battle/immunity/tests.rs`, on the shipped content.

use super::damage_type::{DamageType, DamageTypes, Delivery};
use super::realm::Realm;
use super::{Ability, Rules};

/// How an attack is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Melee,
    /// A thrown weapon or breath, used just before melee.
    ThrownOrBreath,
    /// Arrows, boulders or spell shots from a distance.
    Ranged,
}

/// What an attack is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackNature {
    /// Its damage types: what the target is immune to, and the realm for the resistance checks.
    pub types: DamageTypes,
    /// It halves the defence it meets.
    pub armor_piercing: bool,
}

#[cfg(any(test, feature = "testutil"))]
impl AttackNature {
    /// For tests: the nature of a unit's attack with these abilities (by number) in the shipped content.
    pub fn of_numbers(mode: Mode, numbers: &[u16], magical_weapon: bool) -> AttackNature {
        let abilities: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
        AttackNature::of_unit_attack(
            mode,
            &abilities,
            magical_weapon,
            false,
            &crate::testdir::shipped().rules,
        )
    }
}

impl AttackNature {
    /// The realm among its types, if it is magical.
    pub fn realm(&self) -> Option<Realm> {
        self.types.realm_num().and_then(Realm::from_num)
    }

    /// A magical attack of `realm`, made of `elements` as well.
    pub fn magical(realm: Realm, elements: &[DamageType]) -> AttackNature {
        let mut types = DamageTypes::of(elements);
        types.insert(DamageType::Magic);
        types.insert(realm.damage_type());
        AttackNature {
            types,
            armor_piercing: false,
        }
    }

    /// The nature of one of a unit's attacks. `mode` says which: its melee blows, or its
    /// thrown, breath or ranged attack. What the abilities add is in their `attack` (`abilities.ron`); a
    /// weapon is a normal one unless `magical_weapon`.
    pub fn of_unit_attack(
        mode: Mode,
        abilities: &[Ability],
        magical_weapon: bool,
        magic_exceptions: bool,
        rules: &Rules,
    ) -> AttackNature {
        let mut nature = AttackNature::default();
        let mut exception = false;
        if mode == Mode::Melee {
            nature.types.insert(DamageType::MundaneWeapon);
        }
        for attack in abilities
            .iter()
            .filter_map(|&a| rules.abilities.try_get(a))
            .filter_map(|def| def.attack.as_ref())
            .filter(|a| a.delivery == Delivery::Any || mode != Mode::Melee)
        {
            nature.types.union(DamageTypes::of(&attack.types));
            nature.armor_piercing |= attack.armor_piercing;
            exception |= attack.magic_immunity_exception;
        }
        // The exceptions to magic immunity (the rule setting): such an attack is not of the magic
        // type.
        if magic_exceptions && exception {
            nature.types = nature.types.without(DamageType::Magic);
        }
        if magical_weapon {
            nature.types = nature.types.without(DamageType::MundaneWeapon);
        }
        nature
    }
}

/// How an attack reaches its target, as a damage type.
pub fn delivery(mode: Mode) -> DamageType {
    match mode {
        Mode::Melee => DamageType::Melee,
        Mode::ThrownOrBreath | Mode::Ranged => DamageType::Ranged,
    }
}
