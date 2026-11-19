//! Immunities and resistance in combat.
//!
//! Pure functions over numbers, as in `attack.rs`. They follow the original's
//! `Battle_Unit_Attack_Immunities`, `Battle_Unit_Defense_Special`,
//! `Combat_Effective_Resistance` and `Combat_Resistance_Check`:
//!
//! - **What an attack is made of** ([`AttackNature`]): a set of damage types
//!   (`f4x-def/src/rules/damage_type.rs`), from the attack abilities of the unit (`attack` in
//!   `abilities.ron`). A unit is stopped by an attack when it is immune to one of its types
//!   ([`Immunities`], from the `defence` of its abilities).
//! - **What that does to the defence** ([`effective_defense`]): against anything but melee, a
//!   matching immunity makes the defence 50, so nothing gets through. A defence floor (weapon
//!   immunity: at least 10) applies to the type it names, in melee or not. An illusion ignores
//!   the defence altogether unless the defender sees through illusions.
//! - **Resistance** ([`effective_resistance`], [`resistance_failure`]): a ten-sided die that
//!   rolls above the resistance fails; magic immunity adds 30, and some enchantments add to
//!   resistance against particular realms.
//!
//! A *normal weapon* is any weapon that is not magical. Heroes, fantastic and summoned
//! creatures and units with a flaming, blessed or similar weapon have magical weapons and
//! ignore weapon immunity (`Game::unit_has_magical_weapon`).
//!
//! Tests: in battle/immunity/tests.rs.

use crate::rules::damage_type::{DamageType, DamageTypes, DefenceEffect, Delivery};
use crate::rules::{Ability, Rules};
use crate::spell::unit_effect::{UnitEffect, UnitEffects};
use rand::{Rng, RngCore};

use super::attack::Mode;

/// The realms an attack or spell can belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Realm {
    Nature,
    Sorcery,
    Chaos,
    Life,
    Death,
    Arcane,
}

impl Realm {
    /// The realm with this number in `realms.ron` (the original's `sbr_*` numbers).
    pub fn from_num(num: u16) -> Option<Realm> {
        Some(match num {
            0 => Realm::Nature,
            1 => Realm::Sorcery,
            2 => Realm::Chaos,
            3 => Realm::Life,
            4 => Realm::Death,
            5 => Realm::Arcane,
            _ => return None,
        })
    }

    /// The realm of a definition's [`RealmId`].
    pub fn of(id: crate::rules::RealmId) -> Option<Realm> {
        Realm::from_num(id.num())
    }

    /// The realm's number in `realms.ron`.
    pub fn num(self) -> u16 {
        self as u16
    }

    /// The realm as a damage type.
    pub fn damage_type(self) -> DamageType {
        match self {
            Realm::Nature => DamageType::RealmNature,
            Realm::Sorcery => DamageType::RealmSorcery,
            Realm::Chaos => DamageType::RealmChaos,
            Realm::Life => DamageType::RealmLife,
            Realm::Death => DamageType::RealmDeath,
            Realm::Arcane => DamageType::RealmArcane,
        }
    }

    /// The realm's id.
    pub fn id(self) -> crate::rules::RealmId {
        crate::rules::RealmId::new(self.num())
    }
}

/// The damage types a unit has a defence rule against, from its abilities (`defence` in
/// `abilities.ron`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Immunities {
    /// Every type it has a rule for.
    types: DamageTypes,
    /// Of those, the types it meets with a defence floor instead of being immune to them.
    floored: DamageTypes,
    floor: i32,
}

impl Immunities {
    /// The defence rules of a unit's abilities.
    pub fn of(abilities: &[Ability], rules: &Rules) -> Immunities {
        let mut out = Immunities::default();
        for rule in abilities
            .iter()
            .filter_map(|&a| rules.abilities.try_get(a))
            .flat_map(|def| &def.defence)
        {
            out.types.insert(rule.against);
            if let DefenceEffect::DefenceFloor(floor) = rule.effect {
                out.floored.insert(rule.against);
                out.floor = out.floor.max(floor);
            }
        }
        out
    }

    /// Whether the unit has a defence rule against this type.
    pub fn has(&self, t: DamageType) -> bool {
        self.types.contains(t)
    }

    /// Whether an attack of these types does nothing to the unit: it is immune to one of them. A
    /// type it meets with a floor (weapon) or sees through (illusion) does not count.
    pub fn stops(&self, attack: DamageTypes) -> bool {
        attack.intersects(self.types.minus(self.floored).without(DamageType::Illusion))
    }

    /// Whether the unit has a defence rule against any type of this attack, a floor and a seen-through illusion included: what a planner treats as a wasted attack.
    pub fn meets(&self, attack: DamageTypes) -> bool {
        attack.intersects(self.types)
    }

    /// Whether it raises its defence against this attack to a floor (weapon immunity).
    pub fn floors(&self, attack: DamageTypes) -> bool {
        attack.intersects(self.floored)
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

#[cfg(test)]
impl Immunities {
    /// For tests: the defence rules the shipped content gives these abilities (by number).
    pub(crate) fn of_numbers(numbers: &[u16]) -> Immunities {
        let abilities: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
        Immunities::of(&abilities, &crate::game::testutil::shipped().rules)
    }
}

/// What an attack is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackNature {
    /// Its damage types: what the target is immune to, and the realm for the resistance checks.
    pub types: DamageTypes,
    /// It halves the defence it meets.
    pub armor_piercing: bool,
}

#[cfg(test)]
impl AttackNature {
    /// For tests: the nature of a unit's attack with these abilities (by number) in the shipped content.
    pub(crate) fn of_numbers(mode: Mode, numbers: &[u16], magical_weapon: bool) -> AttackNature {
        let abilities: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
        AttackNature::of_unit_attack(
            mode,
            &abilities,
            magical_weapon,
            &crate::game::testutil::shipped().rules,
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
        rules: &Rules,
    ) -> AttackNature {
        let mut nature = AttackNature::default();
        if mode == Mode::Melee {
            nature.types.insert(DamageType::Weapon);
        }
        for attack in abilities
            .iter()
            .filter_map(|&a| rules.abilities.try_get(a))
            .filter_map(|def| def.attack.as_ref())
            .filter(|a| a.delivery == Delivery::Any || mode != Mode::Melee)
        {
            nature.types.union(DamageTypes::of(&attack.types));
            nature.armor_piercing |= attack.armor_piercing;
        }
        if magical_weapon {
            nature.types = nature.types.without(DamageType::Weapon);
        }
        nature
    }
}

/// The defence a unit rolls against one attack.
///
/// Armour piercing halves the defence first (rounding down). An immunity in common with a non-melee attack makes the defence 50. Weapon immunity then
/// sets it to at least 10 against a normal weapon, even if the attack was stopped outright
/// above (the original's order), and magic immunity sets it to 50 against any ranged magic.
pub fn effective_defense(
    defense: i32,
    unit: &Immunities,
    attack: &AttackNature,
    mode: Mode,
    immune_defense: i32,
) -> i32 {
    // An illusion the defender cannot see through ignores every armour.
    if attack.types.contains(DamageType::Illusion) && !unit.has(DamageType::Illusion) {
        return 0;
    }
    let defense = if attack.armor_piercing {
        defense / 2
    } else {
        defense
    };
    let mut immune = unit.stops(attack.types) && mode != Mode::Melee;
    let mut floored = false;
    if unit.floors(attack.types) {
        immune = false;
        floored = true;
    }
    if immune {
        return immune_defense;
    }
    if floored {
        return defense.max(unit.floor);
    }
    defense
}

/// A unit's resistance against a spell or attack of `realm`: its own, and 30 more with magic
/// immunity against any realm. `effects` are what its lasting spells do; each adds to
/// resistance against particular realms:
/// - Righteousness +30 against chaos and death, Bless +3 against chaos and death;
/// - Elemental Armor +10, or else Resist Elements +3, against chaos and nature;
/// - Resist Magic +5 against any realm.
pub fn effective_resistance(
    resistance: i32,
    unit: &Immunities,
    realm: Option<Realm>,
    effects: UnitEffects,
) -> i32 {
    let mut out = resistance;
    let has = |e: UnitEffect| effects.contains(e);
    let Some(realm) = realm else {
        return out;
    };
    if unit.has(DamageType::Magic) {
        out += 30;
    }
    let dark = matches!(realm, Realm::Chaos | Realm::Death);
    let elemental = matches!(realm, Realm::Chaos | Realm::Nature);
    if dark && has(UnitEffect::Righteousness) {
        out += 30;
    }
    if elemental {
        if has(UnitEffect::ElementalArmor) {
            out += 10;
        } else if has(UnitEffect::ResistElements) {
            out += 3;
        }
    }
    if dark && has(UnitEffect::Bless) {
        out += 3;
    }
    if has(UnitEffect::ResistMagic) {
        out += 5;
    }
    out
}

/// The defence a unit's lasting spells add against an attack of `realm` (`Battle_Unit_Defense_Special`):
/// Elemental Armor +10, or else Resist Elements +3, against ranged chaos and nature; Bless +3
/// against chaos and death.
pub fn defense_bonus(realm: Option<Realm>, mode: Mode, effects: UnitEffects) -> i32 {
    let has = |e: UnitEffect| effects.contains(e);
    let mut out = 0;
    if matches!(realm, Some(Realm::Chaos | Realm::Nature)) && mode != Mode::Melee {
        if has(UnitEffect::ElementalArmor) {
            out += 10;
        } else if has(UnitEffect::ResistElements) {
            out += 3;
        }
    }
    if matches!(realm, Some(Realm::Chaos | Realm::Death)) && has(UnitEffect::Bless) {
        out += 3;
    }
    out
}

/// Righteousness: a ranged attack of chaos or death cannot touch the unit.
pub fn righteous_ward(realm: Option<Realm>, mode: Mode, effects: UnitEffects) -> bool {
    matches!(realm, Some(Realm::Chaos | Realm::Death))
        && mode != Mode::Melee
        && effects.contains(UnitEffect::Righteousness)
}

/// A resistance check: roll a ten-sided die against the resistance plus the spell's modifier
/// (negative for a spell that is hard to resist). Returns by how much the roll beat the
/// resistance, so 0 means the target resisted.
pub fn resistance_failure(resistance: i32, modifier: i32, rng: &mut dyn RngCore) -> i32 {
    let roll = rng.gen_range(1..=10);
    (roll - (resistance + modifier)).max(0)
}

#[cfg(test)]
mod tests;
