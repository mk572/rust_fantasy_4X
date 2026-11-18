use super::*;
use crate::rules::stats::{Modifier, Stat};
use crate::rules::Ability;
use crate::testdir::shipped;

#[test]
fn the_formulas_follow_the_table() {
    let f = &shipped().rules;
    // Level 5 (the sixth): x = 6.
    let (n, s) = (Tier::Normal, Tier::Super);
    let both = |name: &str, level| (f.hero_value(name, n, level), f.hero_value(name, s, level));
    assert_eq!(both("leadership", 5), (2, 3));
    assert_eq!(both("blademaster", 5), (3, 4));
    assert_eq!(both("constitution", 5), (6, 9));
    assert_eq!(both("might", 5), (6, 9));
    assert_eq!(both("agility", 5), (6, 9));
    assert_eq!(both("arcane_power", 5), (6, 9));
    assert_eq!(both("prayermaster", 5), (6, 9));
    assert_eq!(both("legendary", 5), (18, 27));
    assert_eq!(both("armsmaster", 5), (12, 18));
    // Sage goes by the level itself.
    assert_eq!(both("sage", 5), (15, 22));
    assert_eq!(f.hero_value("might", Tier::None, 5), 0);
    assert_eq!(f.hero_value("not_an_ability", n, 5), 0);
    // Rounded down: x = 2.
    assert_eq!(
        (
            f.hero_value("blademaster", s, 1),
            f.hero_value("might", s, 1),
            f.hero_value("legendary", s, 1)
        ),
        (1, 3, 9)
    );
}

#[test]
fn a_changed_factor_in_the_content_changes_the_heros_stats() {
    let mut rules = shipped().rules.clone();
    let hero = HeroAbilities::from_ids(&[Ability::new(85) /* might */]);
    let melee = |r: &crate::rules::Rules| {
        hero.modifiers(r, 3, false)
            .iter()
            .find(|m| m.stat == Stat::Melee)
            .map(|m| m.value)
    };
    assert_eq!(
        melee(&rules),
        Some(HalfInt::from_int(4)),
        "x = 4 at the shipped factor 1/1"
    );
    rules
        .abilities
        .by_num_mut(85)
        .and_then(|a| a.per_level.as_mut())
        .expect("might has a per_level")
        .numerator = 2;
    assert_eq!(
        melee(&rules),
        Some(HalfInt::from_int(8)),
        "the factor is read from the content"
    );
}

#[test]
fn every_hero_ability_has_its_fraction_and_a_doubled_one_is_never_worth_less() {
    let f = &shipped().rules;
    assert_eq!(problems(f), Vec::<String>::new());
    for &name in TIERED {
        for level in 0..5 {
            assert!(
                f.hero_value(name, Tier::Super, level) >= f.hero_value(name, Tier::Normal, level),
                "{name} at {level}"
            );
        }
    }
}

#[test]
fn arcane_power_needs_a_magic_attack() {
    let f = &shipped().rules;
    let a = HeroAbilities::from_ids(&[
        Ability::new(87), /* arcane_power */
        Ability::new(86), /* super_might */
    ]);
    let find = |mods: &[Modifier], stat| mods.iter().find(|m| m.stat == stat).map(|m| m.value);
    assert_eq!(find(&a.modifiers(f, 1, false), Stat::Ranged), None);
    assert_eq!(
        find(&a.modifiers(f, 1, true), Stat::Ranged),
        Some(HalfInt::from_int(2))
    );
    assert_eq!(
        find(&a.modifiers(f, 1, true), Stat::Melee),
        Some(HalfInt::from_int(3))
    );
    assert_eq!(find(&a.modifiers(f, 1, true), Stat::Defense), None);
    let charmed = HeroAbilities::from_ids(&[Ability::new(95) /* charmed */]);
    assert_eq!(
        find(&charmed.modifiers(f, 1, false), Stat::Resistance),
        Some(HalfInt::from_int(f.config.renown.charmed_resistance))
    );
}

#[test]
fn numbers_make_tiers_and_flags() {
    let a = HeroAbilities::from_ids(&[
        Ability::new(75), // leadership
        Ability::new(86), // super_might
        Ability::new(96), // noble
        Ability::new(97), // female
    ]);
    assert_eq!(a.leadership, Tier::Normal);
    assert_eq!(a.might, Tier::Super);
    assert_eq!(a.sage, Tier::None);
    assert!(a.noble && a.female && !a.lucky);
}

#[test]
fn ids_round_trip() {
    // super armsmaster, legendary, lucky, charmed
    let numbers = [82, 77, 39, 95];
    let given: Vec<Ability> = numbers.iter().map(|&n| Ability::new(n)).collect();
    let a = HeroAbilities::from_ids(&given);
    let ids = a.ids();
    assert_eq!(ids.len(), 4);
    for n in numbers {
        assert!(ids.contains(&Ability::new(n)), "{n} missing from {ids:?}");
    }
    assert_eq!(HeroAbilities::from_ids(&ids), a);
}

#[test]
fn other_abilities_are_ignored() {
    // Flying (a unit ability) and a number no ability has.
    let other = [Ability::new(1), Ability::new(9999)];
    assert_eq!(HeroAbilities::from_ids(&other), HeroAbilities::default());
    assert!(!HeroAbilities::is_hero_ability(other[0]));
    assert!(HeroAbilities::is_hero_ability(Ability::new(97)));
}
