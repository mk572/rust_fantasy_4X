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
        // Lightning breath pierces armour: half of 2.
        1
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

#[test]
fn armor_piercing_halves_the_defense_before_immunities_set_their_floor() {
    let plain = unit(&[]);
    assert_eq!(
        defense(6, &plain, &["armor_piercing"], Mode::Melee, false),
        3
    );
    // Rounded down.
    assert_eq!(
        defense(5, &plain, &["armor_piercing"], Mode::Melee, false),
        2
    );
    assert_eq!(
        defense(6, &plain, &["armor_piercing", "bow"], Mode::Ranged, false),
        3
    );
    let golem = unit(&["weapon_immunity"]);
    assert_eq!(
        defense(30, &golem, &["armor_piercing"], Mode::Melee, false),
        15
    );
    assert_eq!(
        defense(8, &golem, &["armor_piercing"], Mode::Melee, false),
        10
    );
    // Immunity still wins outright.
    let shielded = unit(&["missile_immunity"]);
    assert_eq!(
        defense(
            6,
            &shielded,
            &["armor_piercing", "bow"],
            Mode::Ranged,
            false
        ),
        50
    );
}

#[test]
fn enchantments_add_defense_against_the_realms_they_guard() {
    let ranged = Mode::Ranged;
    let d = |realm, mode, names: &[&str]| defense_bonus(Some(realm), mode, names);
    assert_eq!(d(Realm::Chaos, ranged, &["elemental_armor"]), 10);
    assert_eq!(d(Realm::Nature, ranged, &["resist_elements"]), 3);
    assert_eq!(
        d(
            Realm::Nature,
            ranged,
            &["elemental_armor", "resist_elements"]
        ),
        10,
        "the better one counts"
    );
    assert_eq!(d(Realm::Chaos, Mode::Melee, &["elemental_armor"]), 0);
    assert_eq!(d(Realm::Sorcery, ranged, &["elemental_armor"]), 0);
    assert_eq!(d(Realm::Death, ranged, &["bless"]), 3);
    assert_eq!(d(Realm::Life, ranged, &["bless"]), 0);
    assert_eq!(
        defense_bonus(None, ranged, &["bless", "elemental_armor"]),
        0
    );
}

#[test]
fn righteousness_turns_ranged_chaos_and_death_away() {
    let r = |realm, mode| righteous_ward(Some(realm), mode, &["righteousness"]);
    assert!(r(Realm::Chaos, Mode::Ranged));
    assert!(r(Realm::Death, Mode::Ranged));
    assert!(!r(Realm::Nature, Mode::Ranged));
    assert!(!r(Realm::Chaos, Mode::Melee));
    assert!(!righteous_ward(
        Some(Realm::Chaos),
        Mode::Ranged,
        &["bless"]
    ));
}
