use super::*;
use crate::rules::damage_type::DamageType;
use crate::rules::SpellId;
use rand::SeedableRng;

fn unit(names: &[u16]) -> Immunities {
    Immunities::of_numbers(names)
}

fn with_spells(names: &[u16], spells: &[u16]) -> Immunities {
    let spells: Vec<SpellId> = spells.iter().map(|&n| SpellId::new(n)).collect();
    Immunities::of_numbers_and_spells(names, &spells)
}

/// The defence a unit with these `Immunities` rolls against the attack the abilities `names`
/// make in this `mode`; `None` if it is immune to it.
fn defense(base: i32, unit: &Immunities, names: &[u16], mode: Mode, magical: bool) -> Option<i32> {
    let attack = AttackNature::of_numbers(mode, names, magical);
    effective_defense(
        base,
        unit,
        attack.types.with(delivery(mode)),
        attack.armor_piercing,
    )
}

fn types(list: &[DamageType]) -> DamageTypes {
    DamageTypes::of(list)
}

#[test]
fn a_unit_without_immunities_keeps_its_defense() {
    let none = unit(&[]);
    for (mode, names) in [
        (Mode::Melee, &[][..]),
        (Mode::Ranged, &[3 /* bow */][..]),
        (Mode::ThrownOrBreath, &[20 /* fire_breath */][..]),
        (Mode::Ranged, &[29 /* ice_bolt */][..]),
    ] {
        assert_eq!(defense(3, &none, names, mode, false), Some(3));
    }
}

#[test]
fn weapon_immunity_raises_defense_to_ten_against_normal_blows() {
    let golem = unit(&[72 /* weapon_immunity */]);
    assert_eq!(defense(3, &golem, &[], Mode::Melee, false), Some(10));
    // A defence already above ten stays.
    assert_eq!(defense(12, &golem, &[], Mode::Melee, false), Some(12));
    // Magical weapons (heroes, fantastic creatures, flaming blades) ignore it.
    assert_eq!(defense(3, &golem, &[], Mode::Melee, true), Some(3));
    // Normal arrows, boulders and thrown weapons are cut to the same ten.
    for names in [
        &[3 /* bow */][..],
        &[59 /* sling */],
        &[57 /* rock_throwing */],
        &[69 /* thrown */],
    ] {
        let mode = if names == [69 /* thrown */] {
            Mode::ThrownOrBreath
        } else {
            Mode::Ranged
        };
        assert_eq!(
            defense(3, &golem, names, mode, false),
            Some(10),
            "{names:?}"
        );
        assert_eq!(defense(3, &golem, names, mode, true), Some(3), "{names:?}");
    }
}

#[test]
fn missile_immunity_stops_every_missile_and_nothing_else() {
    // Arrows, sling stones, boulders and thrown weapons are all missiles, whether or not the
    // weapon is magical; a blow in melee is not.
    let shielded = unit(&[44 /* missile_immunity */]);
    for names in [
        &[3 /* bow */][..],
        &[59 /* sling */],
        &[57 /* rock_throwing */],
        &[69 /* thrown */],
    ] {
        let mode = if names == [69 /* thrown */] {
            Mode::ThrownOrBreath
        } else {
            Mode::Ranged
        };
        for magical in [false, true] {
            assert_eq!(
                defense(3, &shielded, names, mode, magical),
                None,
                "{names:?}"
            );
        }
    }
    assert_eq!(defense(3, &shielded, &[], Mode::Melee, false), Some(3));
}

#[test]
fn an_immunity_beats_a_floor() {
    // Weapon immunity only raises the defence; missile immunity stops the arrow outright, whether
    // or not the other rule is there to raise the defence.
    let both = unit(&[
        72, /* weapon_immunity */
        44, /* missile_immunity */
    ]);
    assert_eq!(defense(3, &both, &[3 /* bow */], Mode::Ranged, false), None);
    assert_eq!(defense(3, &both, &[3 /* bow */], Mode::Ranged, true), None);
    assert_eq!(defense(3, &both, &[], Mode::Melee, false), Some(10));
}

#[test]
fn an_immunity_stops_the_type_however_it_is_delivered() {
    // A fire-immune unit takes nothing from fire in melee either: no delivery is an exception.
    let salamander = unit(&[21 /* fire_immunity */]);
    for delivery in [DamageType::Melee, DamageType::Ranged] {
        assert_eq!(
            effective_defense(3, &salamander, types(&[DamageType::Fire, delivery]), false),
            None
        );
        assert_eq!(
            effective_defense(3, &salamander, types(&[DamageType::Cold, delivery]), false),
            Some(3)
        );
    }
}

#[test]
fn fire_immunity_stops_fire_breath_and_fireballs_but_not_lightning_or_melee() {
    let salamander = unit(&[21 /* fire_immunity */]);
    assert_eq!(
        defense(
            2,
            &salamander,
            &[20 /* fire_breath */],
            Mode::ThrownOrBreath,
            false
        ),
        None
    );
    assert_eq!(
        defense(
            2,
            &salamander,
            &[37 /* lightning_breath */],
            Mode::ThrownOrBreath,
            false
        ),
        // Lightning breath pierces armour: half of 2.
        Some(1)
    );
    assert_eq!(
        defense(
            2,
            &salamander,
            &[22 /* fireball_attack */],
            Mode::Ranged,
            false
        ),
        None
    );
    assert_eq!(defense(2, &salamander, &[], Mode::Melee, false), Some(2));
}

#[test]
fn cold_immunity_stops_the_ice_bolt_of_a_unit() {
    let yeti = unit(&[10 /* cold_immunity */]);
    assert_eq!(
        defense(2, &yeti, &[29 /* ice_bolt */], Mode::Ranged, false),
        None
    );
    assert_eq!(
        defense(2, &yeti, &[36 /* lightning_bolt */], Mode::Ranged, false),
        Some(2)
    );
}

#[test]
fn magic_immunity_stops_every_ranged_magic_but_not_melee_or_arrows() {
    let wise = unit(&[40 /* magic_immunity */]);
    for names in [
        &[22 /* fireball_attack */][..],
        &[36 /* lightning_bolt */],
        &[29 /* ice_bolt */],
        &[60 /* sorcery_bolt */],
        &[14 /* death_bolt */],
        &[20 /* fire_breath */],
        &[37 /* lightning_breath */],
    ] {
        let mode = if matches!(
            names[0],
            20 /* fire_breath */ | 37 /* lightning_breath */
        ) {
            Mode::ThrownOrBreath
        } else {
            Mode::Ranged
        };
        assert_eq!(defense(3, &wise, names, mode, false), None, "{names:?}");
    }
    assert_eq!(
        defense(3, &wise, &[3 /* bow */], Mode::Ranged, false),
        Some(3)
    );
    assert_eq!(
        defense(3, &wise, &[69 /* thrown */], Mode::ThrownOrBreath, false),
        Some(3)
    );
    assert_eq!(defense(3, &wise, &[], Mode::Melee, false), Some(3));
}

#[test]
fn illusions_ignore_defense_unless_the_defender_sees_through_them() {
    let plain = unit(&[]);
    let seer = unit(&[31 /* illusion_immunity */]);
    let illusion = |unit: &Immunities, mode, extra: &[u16]| {
        let mut names = vec![30 /* illusion_attack */];
        names.extend_from_slice(extra);
        defense(5, unit, &names, mode, false)
    };
    assert_eq!(illusion(&plain, Mode::Melee, &[]), Some(0));
    assert_eq!(illusion(&plain, Mode::Ranged, &[3 /* bow */]), Some(0));
    // Seeing through it is not being immune to it: the defence is what it was.
    assert_eq!(illusion(&seer, Mode::Melee, &[]), Some(5));
    // ... and the rest of the attack is met as it was.
    let golem = unit(&[
        72, /* weapon_immunity */
        31, /* illusion_immunity */
    ]);
    assert_eq!(illusion(&golem, Mode::Melee, &[]), Some(10));
}

#[test]
fn immunities_come_from_abilities() {
    let all = unit(&[
        72, /* weapon_immunity */
        40, /* magic_immunity */
        44, /* missile_immunity */
        21, /* fire_immunity */
        10, /* cold_immunity */
        52, /* poison_immunity */
        65, /* stoning_immunity */
        16, /* death_immunity */
        31, /* illusion_immunity */
    ]);
    for t in [
        DamageType::Weapon,
        DamageType::Magic,
        DamageType::Missile,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Poison,
        DamageType::Stoning,
        DamageType::Death,
        DamageType::Illusion,
    ] {
        assert!(all.has(t), "{t:?}");
    }
    assert!(unit(&[24 /* flying */, 23 /* first_strike */]).is_empty());
    assert!(Immunities::of_numbers(&[21, 3]).has(DamageType::Fire));
}

#[test]
fn a_rule_matches_any_of_its_against_types_and_all_of_its_with_types() {
    use crate::rules::damage_type::{DefenceEffect, DefenceRule};
    let rule = |against: &[DamageType], with: &[DamageType]| DefenceRule {
        against: types(against),
        with: types(with),
        effect: DefenceEffect::DefenceBonus(1),
    };
    let chaos_or_nature = rule(&[DamageType::ChaosRealm, DamageType::NatureRealm], &[]);
    assert!(chaos_or_nature.matches(types(&[DamageType::ChaosRealm])));
    assert!(chaos_or_nature.matches(types(&[DamageType::NatureRealm, DamageType::Melee])));
    assert!(!chaos_or_nature.matches(types(&[DamageType::LifeRealm])));
    let chaos_and_ranged = rule(&[DamageType::ChaosRealm], &[DamageType::Ranged]);
    assert!(chaos_and_ranged.matches(types(&[DamageType::ChaosRealm, DamageType::Ranged])));
    assert!(!chaos_and_ranged.matches(types(&[DamageType::ChaosRealm, DamageType::Melee])));
    // No `against` is every attack: only `with` counts.
    let any_ranged = rule(&[], &[DamageType::Ranged]);
    assert!(any_ranged.matches(types(&[DamageType::Fire, DamageType::Ranged])));
    assert!(!any_ranged.matches(types(&[DamageType::Fire, DamageType::Melee])));
}

#[test]
fn resistance_adds_up_by_realm() {
    let r = |abilities: &[u16], realm, spells: &[u16]| {
        effective_resistance(5, &with_spells(abilities, spells), Some(realm))
    };
    assert_eq!(r(&[], Realm::Chaos, &[]), 5);
    assert_eq!(r(&[40 /* magic_immunity */], Realm::Sorcery, &[]), 35);
    assert_eq!(r(&[], Realm::Chaos, &[2 /* resist_elements */]), 8);
    assert_eq!(r(&[], Realm::Life, &[2 /* resist_elements */]), 5);
    assert_eq!(r(&[], Realm::Nature, &[21 /* elemental_armor */]), 15);
    assert_eq!(r(&[], Realm::Death, &[144 /* righteousness */]), 35);
    assert_eq!(r(&[], Realm::Nature, &[144 /* righteousness */]), 5);
    assert_eq!(r(&[], Realm::Death, &[121 /* bless */]), 8);
    assert_eq!(r(&[], Realm::Life, &[121 /* bless */]), 5);
    assert_eq!(r(&[], Realm::Sorcery, &[41 /* resist_magic */]), 10);
    assert_eq!(
        r(
            &[],
            Realm::Chaos,
            &[
                41,  /* resist_magic */
                2,   /* resist_elements */
                121  /* bless */
            ]
        ),
        16
    );
    // No realm, no bonus: a plain weapon is not resisted.
    assert_eq!(
        effective_resistance(5, &with_spells(&[40], &[41 /* resist_magic */]), None),
        5
    );
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
        defense(6, &plain, &[1 /* armor_piercing */], Mode::Melee, false),
        Some(3)
    );
    // Rounded down.
    assert_eq!(
        defense(5, &plain, &[1 /* armor_piercing */], Mode::Melee, false),
        Some(2)
    );
    assert_eq!(
        defense(
            6,
            &plain,
            &[1 /* armor_piercing */, 3 /* bow */],
            Mode::Ranged,
            false
        ),
        Some(3)
    );
    let golem = unit(&[72 /* weapon_immunity */]);
    assert_eq!(
        defense(30, &golem, &[1 /* armor_piercing */], Mode::Melee, false),
        Some(15)
    );
    assert_eq!(
        defense(8, &golem, &[1 /* armor_piercing */], Mode::Melee, false),
        Some(10)
    );
    // An immunity still wins outright.
    let shielded = unit(&[44 /* missile_immunity */]);
    assert_eq!(
        defense(
            6,
            &shielded,
            &[1 /* armor_piercing */, 3 /* bow */],
            Mode::Ranged,
            false
        ),
        None
    );
}

#[test]
fn enchantments_add_defense_against_the_realms_they_guard_however_it_is_delivered() {
    let bonus = |spells: &[u16], attack: &[DamageType]| {
        with_spells(&[], spells).defence_bonus(types(attack))
    };
    // Elemental Armor and Resist Elements care about the realm, not about how it arrives.
    for delivery in [DamageType::Melee, DamageType::Ranged] {
        assert_eq!(
            bonus(
                &[21 /* elemental_armor */],
                &[DamageType::ChaosRealm, delivery]
            ),
            10
        );
        assert_eq!(
            bonus(
                &[2 /* resist_elements */],
                &[DamageType::NatureRealm, delivery]
            ),
            3
        );
        assert_eq!(
            bonus(
                &[21 /* elemental_armor */],
                &[DamageType::SorceryRealm, delivery]
            ),
            0
        );
        assert_eq!(
            bonus(&[121 /* bless */], &[DamageType::DeathRealm, delivery]),
            3
        );
        assert_eq!(
            bonus(&[121 /* bless */], &[DamageType::LifeRealm, delivery]),
            0
        );
    }
    // An attack with no realm gets no bonus.
    assert_eq!(
        bonus(&[121, 21], &[DamageType::Fire, DamageType::Ranged]),
        0
    );
}

#[test]
fn a_large_shield_adds_defense_against_ranged_attacks_only() {
    let shield = unit(&[34 /* large_shield */]);
    assert_eq!(
        shield.defence_bonus(types(&[DamageType::Weapon, DamageType::Ranged])),
        2
    );
    assert_eq!(
        shield.defence_bonus(types(&[
            DamageType::Magic,
            DamageType::ChaosRealm,
            DamageType::Ranged
        ])),
        2
    );
    assert_eq!(
        shield.defence_bonus(types(&[DamageType::Weapon, DamageType::Melee])),
        0
    );
}

#[test]
fn righteousness_turns_chaos_and_death_away_however_it_is_delivered() {
    let guarded = with_spells(&[], &[144 /* righteousness */]);
    for delivery in [DamageType::Melee, DamageType::Ranged] {
        assert!(guarded.stops(types(&[DamageType::ChaosRealm, delivery])));
        assert!(guarded.stops(types(&[DamageType::DeathRealm, delivery])));
        assert!(!guarded.stops(types(&[DamageType::NatureRealm, delivery])));
    }
    assert!(!with_spells(&[], &[121 /* bless */])
        .stops(types(&[DamageType::ChaosRealm, DamageType::Ranged])));
}
