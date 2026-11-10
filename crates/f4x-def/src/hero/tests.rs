use super::*;
use crate::rules::stats::{Modifier, Stat};
use crate::rules::Ability;

#[test]
fn the_formulas_follow_the_table() {
    // Level 5 (the sixth): x = 6.
    let (n, s) = (Tier::Normal, Tier::Super);
    assert_eq!((leadership(n, 5), leadership(s, 5)), (2, 3));
    assert_eq!((blademaster(n, 5), blademaster(s, 5)), (3, 4));
    assert_eq!((constitution(n, 5), constitution(s, 5)), (6, 9));
    assert_eq!((might(n, 5), might(s, 5)), (6, 9));
    assert_eq!((agility(n, 5), agility(s, 5)), (6, 9));
    assert_eq!((arcane_power(n, 5), arcane_power(s, 5)), (6, 9));
    assert_eq!((prayermaster(n, 5), prayermaster(s, 5)), (6, 9));
    assert_eq!((legendary(n, 5), legendary(s, 5)), (18, 27));
    assert_eq!((armsmaster(n, 5), armsmaster(s, 5)), (12, 18));
    // Sage goes by the level itself.
    assert_eq!((sage(n, 5), sage(s, 5)), (15, 22));
    assert_eq!(might(Tier::None, 5), 0);
    // Rounded down: x = 2.
    assert_eq!((blademaster(s, 1), might(s, 1), legendary(s, 1)), (1, 3, 9));
}

#[test]
fn arcane_power_needs_a_magic_attack() {
    let a = HeroAbilities::from_ids(&[
        Ability::new(87), /* arcane_power */
        Ability::new(86), /* super_might */
    ]);
    let find = |mods: &[Modifier], stat| mods.iter().find(|m| m.stat == stat).map(|m| m.value);
    assert_eq!(find(&a.modifiers(1, false), Stat::Ranged), None);
    assert_eq!(
        find(&a.modifiers(1, true), Stat::Ranged),
        Some(HalfInt::from_int(2))
    );
    assert_eq!(
        find(&a.modifiers(1, true), Stat::Melee),
        Some(HalfInt::from_int(3))
    );
    assert_eq!(find(&a.modifiers(1, true), Stat::Defense), None);
    let charmed = HeroAbilities::from_ids(&[Ability::new(95) /* charmed */]);
    assert_eq!(
        find(&charmed.modifiers(1, false), Stat::Resistance),
        Some(HalfInt::from_int(30))
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
