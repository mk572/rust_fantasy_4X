//! Immunities and resistance in combat.
//!
//! Pure functions over numbers, as in `attack.rs`. They follow the original's
//! `Battle_Unit_Attack_Immunities`, `Battle_Unit_Defense_Special`,
//! `Combat_Effective_Resistance` and `Combat_Resistance_Check`:
//!
//! - **What an attack is made of** ([`AttackNature`]): a normal weapon, a missile, fire, a
//!   magical shot, an illusion. A unit is stopped by an attack when it has an immunity to any
//!   of those parts ([`Immunities`]).
//! - **What that does to the defence** ([`effective_defense`]): against anything but melee, a
//!   matching immunity makes the defence 50, so nothing gets through. Weapon immunity against
//!   a normal weapon, in melee or not, raises the defence to at least 10. Magic immunity makes
//!   the defence 50 against any ranged magic. An illusion ignores the defence altogether unless
//!   the defender sees through illusions.
//! - **Resistance** ([`effective_resistance`], [`resistance_failure`]): a ten-sided die that
//!   rolls above the resistance fails; magic immunity adds 30, and some enchantments add to
//!   resistance against particular realms.
//!
//! A *normal weapon* is any weapon that is not magical. Heroes, fantastic and summoned
//! creatures and units with a flaming, blessed or similar weapon have magical weapons and
//! ignore weapon immunity (`Game::unit_has_magical_weapon`).
//!
//! Tests: in battle/immunity/tests.rs.

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
    pub fn from_id(id: &str) -> Option<Realm> {
        Some(match id {
            "nature" => Realm::Nature,
            "sorcery" => Realm::Sorcery,
            "chaos" => Realm::Chaos,
            "life" => Realm::Life,
            "death" => Realm::Death,
            "arcane" => Realm::Arcane,
            _ => return None,
        })
    }
}

/// Immunities a unit has, or the immunities that stop an attack (the same nine, so an attack
/// is stopped when the two share one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Immunities {
    pub weapon: bool,
    pub magic: bool,
    pub missiles: bool,
    pub fire: bool,
    pub cold: bool,
    pub poison: bool,
    pub stoning: bool,
    pub death: bool,
    pub illusion: bool,
}

impl Immunities {
    /// The immunities among a unit's abilities (`weapon_immunity`, `magic_immunity`, ...).
    pub fn of(abilities: &[String]) -> Immunities {
        let has = |name: &str| abilities.iter().any(|a| a == name);
        Immunities {
            weapon: has("weapon_immunity"),
            magic: has("magic_immunity"),
            missiles: has("missile_immunity"),
            fire: has("fire_immunity"),
            cold: has("cold_immunity"),
            poison: has("poison_immunity"),
            stoning: has("stoning_immunity"),
            death: has("death_immunity"),
            illusion: has("illusion_immunity"),
        }
    }

    /// From the ability names a spell's data lists, ignoring names that are not immunities.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Immunities {
        let list: Vec<String> = names.into_iter().map(str::to_string).collect();
        Immunities::of(&list)
    }

    /// Whether the two have an immunity in common.
    pub fn meets(&self, other: &Immunities) -> bool {
        (self.weapon && other.weapon)
            || (self.magic && other.magic)
            || (self.missiles && other.missiles)
            || (self.fire && other.fire)
            || (self.cold && other.cold)
            || (self.poison && other.poison)
            || (self.stoning && other.stoning)
            || (self.death && other.death)
            || (self.illusion && other.illusion)
    }

    pub fn is_empty(&self) -> bool {
        *self == Immunities::default()
    }
}

/// What an attack is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackNature {
    /// The immunities that stop it.
    pub stopped_by: Immunities,
    /// The realm it belongs to, if it is magical.
    pub realm: Option<Realm>,
    /// It halves the defence it meets (`armor_piercing`; lightning breath does too).
    pub armor_piercing: bool,
}

impl AttackNature {
    /// The nature of one of a unit's attacks. `mode` says which: its melee blows, or its
    /// thrown, breath or ranged attack (the kind found among `abilities`).
    pub fn of_unit_attack(mode: Mode, abilities: &[String], magical_weapon: bool) -> AttackNature {
        let has = |name: &str| abilities.iter().any(|a| a == name);
        let mut stopped_by = Immunities {
            illusion: has("illusion_attack"),
            ..Immunities::default()
        };
        let mut realm = None;
        let mut armor_piercing = has("armor_piercing");
        if mode == Mode::Melee {
            stopped_by.weapon = !magical_weapon;
            return AttackNature {
                stopped_by,
                realm,
                armor_piercing,
            };
        }
        match ranged_flavour(abilities) {
            Flavour::Arrows => {
                stopped_by.missiles = true;
                stopped_by.weapon = !magical_weapon;
            }
            Flavour::Boulders | Flavour::Thrown => stopped_by.weapon = !magical_weapon,
            Flavour::FireBreath => {
                stopped_by.fire = true;
                realm = Some(Realm::Chaos);
            }
            Flavour::LightningBreath => {
                realm = Some(Realm::Chaos);
                armor_piercing = true;
            }
            Flavour::Bolt(r) => {
                stopped_by.magic = true;
                realm = Some(r);
            }
            Flavour::Gaze(r) => realm = Some(r),
            Flavour::None => {}
        }
        AttackNature {
            stopped_by,
            realm,
            armor_piercing,
        }
    }
}

/// Which ranged attack a unit has (in the order the battle gives them precedence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavour {
    None,
    Arrows,
    Boulders,
    Thrown,
    FireBreath,
    LightningBreath,
    Bolt(Realm),
    Gaze(Realm),
}

fn ranged_flavour(abilities: &[String]) -> Flavour {
    let has = |name: &str| abilities.iter().any(|a| a == name);
    if has("bow") || has("sling") {
        Flavour::Arrows
    } else if has("rock_throwing") {
        Flavour::Boulders
    } else if has("thrown") {
        Flavour::Thrown
    } else if has("fire_breath") {
        Flavour::FireBreath
    } else if has("lightning_breath") {
        Flavour::LightningBreath
    } else if has("multi_gaze") {
        Flavour::Gaze(Realm::Chaos)
    } else if has("stoning_gaze") {
        Flavour::Gaze(Realm::Nature)
    } else if has("death_gaze") {
        Flavour::Gaze(Realm::Death)
    } else if has("fireball_attack")
        || has("lightning_bolt")
        || has("drow_bolt")
        || has("death_bolt")
    {
        Flavour::Bolt(Realm::Chaos)
    } else if has("sorcery_bolt") {
        Flavour::Bolt(Realm::Sorcery)
    } else if has("ice_bolt") || has("nature_bolt") || has("sprite_bolt") || has("shaman_bolt") {
        Flavour::Bolt(Realm::Nature)
    } else {
        Flavour::None
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
) -> i32 {
    let mut stopped = attack.stopped_by;
    // Seeing through illusions takes the illusion out of the attack; what is left is an
    // illusion the defender cannot see through, which no armour helps against.
    if stopped.illusion && unit.illusion {
        stopped.illusion = false;
    }
    if stopped.illusion {
        return 0;
    }
    let defense = if attack.armor_piercing {
        defense / 2
    } else {
        defense
    };
    let mut immune = false;
    let mut at_least_ten = false;
    if stopped.meets(unit) && mode != Mode::Melee {
        immune = true;
    }
    if stopped.weapon && unit.weapon {
        immune = false;
        at_least_ten = true;
    }
    if unit.magic && attack.realm.is_some() && mode != Mode::Melee {
        immune = true;
    }
    if immune {
        return IMMUNE_DEFENSE;
    }
    if at_least_ten {
        return defense.max(10);
    }
    defense
}

/// A unit's resistance against a spell or attack of `realm`: its own, and 30 more with magic
/// immunity against any realm. `enchantments` are the ids of its lasting spells; each adds to
/// resistance against particular realms:
/// - Righteousness +30 against chaos and death, Bless +3 against chaos and death;
/// - Elemental Armor +10, or else Resist Elements +3, against chaos and nature;
/// - Resist Magic +5 against any realm.
pub fn effective_resistance(
    resistance: i32,
    unit: &Immunities,
    realm: Option<Realm>,
    enchantments: &[&str],
) -> i32 {
    let mut out = resistance;
    let has = |id: &str| enchantments.contains(&id);
    let Some(realm) = realm else {
        return out;
    };
    if unit.magic {
        out += 30;
    }
    let dark = matches!(realm, Realm::Chaos | Realm::Death);
    let elemental = matches!(realm, Realm::Chaos | Realm::Nature);
    if dark && has("righteousness") {
        out += 30;
    }
    if elemental {
        if has("elemental_armor") {
            out += 10;
        } else if has("resist_elements") {
            out += 3;
        }
    }
    if dark && has("bless") {
        out += 3;
    }
    if has("resist_magic") {
        out += 5;
    }
    out
}

/// The defence a unit's lasting spells add against an attack of `realm` (`Battle_Unit_Defense_Special`):
/// Elemental Armor +10, or else Resist Elements +3, against ranged chaos and nature; Bless +3
/// against chaos and death.
pub fn defense_bonus(realm: Option<Realm>, mode: Mode, enchantments: &[&str]) -> i32 {
    let has = |id: &str| enchantments.contains(&id);
    let mut out = 0;
    if matches!(realm, Some(Realm::Chaos | Realm::Nature)) && mode != Mode::Melee {
        if has("elemental_armor") {
            out += 10;
        } else if has("resist_elements") {
            out += 3;
        }
    }
    if matches!(realm, Some(Realm::Chaos | Realm::Death)) && has("bless") {
        out += 3;
    }
    out
}

/// Righteousness: a ranged attack of chaos or death cannot touch the unit.
pub fn righteous_ward(realm: Option<Realm>, mode: Mode, enchantments: &[&str]) -> bool {
    matches!(realm, Some(Realm::Chaos | Realm::Death))
        && mode != Mode::Melee
        && enchantments.contains(&"righteousness")
}

/// The defence an unbeatable ward or immunity gives.
pub const IMMUNE_DEFENSE: i32 = 50;

/// A resistance check: roll a ten-sided die against the resistance plus the spell's modifier
/// (negative for a spell that is hard to resist). Returns by how much the roll beat the
/// resistance, so 0 means the target resisted.
pub fn resistance_failure(resistance: i32, modifier: i32, rng: &mut dyn RngCore) -> i32 {
    let roll = rng.gen_range(1..=10);
    (roll - (resistance + modifier)).max(0)
}

#[cfg(test)]
mod tests;
