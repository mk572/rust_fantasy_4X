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

use rand::{Rng, RngCore};

use super::attack::Mode;

/// The realms an attack or spell can belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        if mode == Mode::Melee {
            stopped_by.weapon = !magical_weapon;
            return AttackNature { stopped_by, realm };
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
            Flavour::LightningBreath => realm = Some(Realm::Chaos),
            Flavour::Bolt(r) => {
                stopped_by.magic = true;
                realm = Some(r);
            }
            Flavour::None => {}
        }
        AttackNature { stopped_by, realm }
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
/// An immunity in common with a non-melee attack makes the defence 50. Weapon immunity then
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
        return 50;
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

/// A resistance check: roll a ten-sided die against the resistance plus the spell's modifier
/// (negative for a spell that is hard to resist). Returns by how much the roll beat the
/// resistance, so 0 means the target resisted.
pub fn resistance_failure(resistance: i32, modifier: i32, rng: &mut dyn RngCore) -> i32 {
    let roll = rng.gen_range(1..=10);
    (roll - (resistance + modifier)).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn abilities(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn unit(names: &[&str]) -> Immunities {
        Immunities::of(&abilities(names))
    }

    fn defense(base: i32, unit: &Immunities, names: &[&str], mode: Mode, magical: bool) -> i32 {
        let attack = AttackNature::of_unit_attack(mode, &abilities(names), magical);
        effective_defense(base, unit, &attack, mode)
    }

    #[test]
    fn a_unit_without_immunities_keeps_its_defense() {
        let none = unit(&[]);
        for (mode, names) in [
            (Mode::Melee, &[][..]),
            (Mode::Ranged, &["bow"][..]),
            (Mode::ThrownOrBreath, &["fire_breath"][..]),
            (Mode::Ranged, &["ice_bolt"][..]),
        ] {
            assert_eq!(defense(3, &none, names, mode, false), 3);
        }
    }

    #[test]
    fn weapon_immunity_raises_defense_to_ten_against_normal_blows() {
        let golem = unit(&["weapon_immunity"]);
        assert_eq!(defense(3, &golem, &[], Mode::Melee, false), 10);
        // A defence already above ten stays.
        assert_eq!(defense(12, &golem, &[], Mode::Melee, false), 12);
        // Magical weapons (heroes, fantastic creatures, flaming blades) ignore it.
        assert_eq!(defense(3, &golem, &[], Mode::Melee, true), 3);
        // Normal arrows, boulders and thrown weapons are cut to the same ten.
        for names in [&["bow"][..], &["sling"], &["rock_throwing"], &["thrown"]] {
            let mode = if names == ["thrown"] {
                Mode::ThrownOrBreath
            } else {
                Mode::Ranged
            };
            assert_eq!(defense(3, &golem, names, mode, false), 10, "{names:?}");
            assert_eq!(defense(3, &golem, names, mode, true), 3, "{names:?}");
        }
    }

    #[test]
    fn missile_immunity_stops_arrows_but_not_boulders_or_melee() {
        let shielded = unit(&["missile_immunity"]);
        assert_eq!(defense(3, &shielded, &["bow"], Mode::Ranged, false), 50);
        assert_eq!(defense(3, &shielded, &["sling"], Mode::Ranged, true), 50);
        assert_eq!(
            defense(3, &shielded, &["rock_throwing"], Mode::Ranged, false),
            3
        );
        assert_eq!(
            defense(3, &shielded, &["thrown"], Mode::ThrownOrBreath, false),
            3
        );
        assert_eq!(defense(3, &shielded, &[], Mode::Melee, false), 3);
    }

    #[test]
    fn a_unit_with_both_weapon_and_missile_immunity_gets_only_ten_against_arrows() {
        // The original's order: weapon immunity is checked last and replaces the stop.
        let both = unit(&["weapon_immunity", "missile_immunity"]);
        assert_eq!(defense(3, &both, &["bow"], Mode::Ranged, false), 10);
        // A magical bow is stopped by missile immunity alone.
        assert_eq!(defense(3, &both, &["bow"], Mode::Ranged, true), 50);
    }

    #[test]
    fn fire_immunity_stops_fire_breath_only() {
        let salamander = unit(&["fire_immunity"]);
        assert_eq!(
            defense(
                2,
                &salamander,
                &["fire_breath"],
                Mode::ThrownOrBreath,
                false
            ),
            50
        );
        assert_eq!(
            defense(
                2,
                &salamander,
                &["lightning_breath"],
                Mode::ThrownOrBreath,
                false
            ),
            2
        );
        assert_eq!(
            defense(2, &salamander, &["fireball_attack"], Mode::Ranged, false),
            2
        );
        assert_eq!(defense(2, &salamander, &[], Mode::Melee, false), 2);
    }

    #[test]
    fn magic_immunity_stops_every_ranged_magic_but_not_melee_or_arrows() {
        let wise = unit(&["magic_immunity"]);
        for names in [
            &["fireball_attack"][..],
            &["lightning_bolt"],
            &["ice_bolt"],
            &["sorcery_bolt"],
            &["death_bolt"],
            &["fire_breath"],
            &["lightning_breath"],
        ] {
            let mode = if names[0].ends_with("breath") {
                Mode::ThrownOrBreath
            } else {
                Mode::Ranged
            };
            assert_eq!(defense(3, &wise, names, mode, false), 50, "{names:?}");
        }
        assert_eq!(defense(3, &wise, &["bow"], Mode::Ranged, false), 3);
        assert_eq!(
            defense(3, &wise, &["thrown"], Mode::ThrownOrBreath, false),
            3
        );
        assert_eq!(defense(3, &wise, &[], Mode::Melee, false), 3);
    }

    #[test]
    fn illusions_ignore_defense_unless_the_defender_sees_through_them() {
        let plain = unit(&[]);
        let seer = unit(&["illusion_immunity"]);
        assert_eq!(
            defense(5, &plain, &["illusion_attack"], Mode::Melee, false),
            0
        );
        assert_eq!(
            defense(5, &plain, &["illusion_attack", "bow"], Mode::Ranged, false),
            0
        );
        assert_eq!(
            defense(5, &seer, &["illusion_attack"], Mode::Melee, false),
            5
        );
        // Seeing through the illusion leaves the rest of the attack as it was.
        assert_eq!(
            defense(5, &seer, &["illusion_attack"], Mode::Melee, false),
            5
        );
        let golem = unit(&["weapon_immunity", "illusion_immunity"]);
        assert_eq!(
            defense(5, &golem, &["illusion_attack"], Mode::Melee, false),
            10
        );
    }

    #[test]
    fn immunities_come_from_abilities() {
        let all = unit(&[
            "weapon_immunity",
            "magic_immunity",
            "missile_immunity",
            "fire_immunity",
            "cold_immunity",
            "poison_immunity",
            "stoning_immunity",
            "death_immunity",
            "illusion_immunity",
        ]);
        assert!(all.weapon && all.magic && all.missiles && all.fire && all.cold);
        assert!(all.poison && all.stoning && all.death && all.illusion);
        assert!(unit(&["flying", "first_strike"]).is_empty());
        assert!(Immunities::from_names(["fire_immunity", "bow"]).fire);
    }

    #[test]
    fn resistance_adds_up_by_realm() {
        let none = unit(&[]);
        let wise = unit(&["magic_immunity"]);
        let r = |imm: &Immunities, realm, enchants: &[&str]| {
            effective_resistance(5, imm, Some(realm), enchants)
        };
        assert_eq!(r(&none, Realm::Chaos, &[]), 5);
        assert_eq!(r(&wise, Realm::Sorcery, &[]), 35);
        assert_eq!(r(&none, Realm::Chaos, &["resist_elements"]), 8);
        assert_eq!(r(&none, Realm::Life, &["resist_elements"]), 5);
        assert_eq!(
            r(
                &none,
                Realm::Nature,
                &["elemental_armor", "resist_elements"]
            ),
            15
        );
        assert_eq!(r(&none, Realm::Death, &["righteousness"]), 35);
        assert_eq!(r(&none, Realm::Nature, &["righteousness"]), 5);
        assert_eq!(r(&none, Realm::Death, &["bless"]), 8);
        assert_eq!(r(&none, Realm::Life, &["bless"]), 5);
        assert_eq!(r(&none, Realm::Sorcery, &["resist_magic"]), 10);
        assert_eq!(
            r(
                &none,
                Realm::Chaos,
                &["resist_magic", "resist_elements", "bless"]
            ),
            16
        );
        // No realm, no bonus: a plain weapon is not resisted.
        assert_eq!(effective_resistance(5, &wise, None, &["resist_magic"]), 5);
    }

    #[test]
    fn a_resistance_check_fails_by_how_far_the_die_beats_the_resistance() {
        let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(7);
        let mut fails = 0;
        let mut worst = 0;
        let n = 20_000;
        for _ in 0..n {
            let f = resistance_failure(4, 0, &mut rng);
            assert!((0..=6).contains(&f));
            fails += i32::from(f > 0);
            worst = worst.max(f);
        }
        // Resistance 4 fails on a 5 to 10: six in ten.
        let share = f64::from(fails) / f64::from(n);
        assert!((share - 0.6).abs() < 0.02, "{share}");
        assert_eq!(worst, 6);
        // A penalty makes the same unit fail more, a bonus less, and 10 or more never fails.
        let count = |res, modi, rng: &mut rand_pcg::Pcg64Mcg| {
            (0..n)
                .filter(|_| resistance_failure(res, modi, rng) > 0)
                .count()
        };
        assert!(count(4, -2, &mut rng) > count(4, 0, &mut rng));
        assert!(count(4, 3, &mut rng) < count(4, 0, &mut rng));
        assert_eq!(count(9, 1, &mut rng), 0);
        assert_eq!(count(35, 0, &mut rng), 0);
    }
}
