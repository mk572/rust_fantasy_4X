//! Spells that hit and heal, on real units: the strength of a bolt, warp lightning, magic immunity
//! and righteousness, healing, and where a summoned creature may appear.
//!
//! Tests: this is the test file.

use f4x_battle::attack::{spell_attack, Volley};
use f4x_battle::attack::{Combatant, DefenderState};
use f4x_battle::testutil::*;
use f4x_battle::Side;
use f4x_def::rules::attack_nature::AttackNature;
use f4x_def::rules::damage_type::DamageType;
use f4x_def::rules::effects::DamageTraits;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::SpellId;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn mean(volley: Volley, spell: &Combatant, def: &DefenderState, trials: u64) -> f64 {
    let total: i32 = (0..trials)
        .map(|seed| {
            let mut state = def.clone();
            let mut rng = Pcg64Mcg::seed_from_u64(seed);
            spell_attack(&rules(), spell, volley, &mut state, &mut rng).points
        })
        .sum();
    total as f64 / trials as f64
}

#[test]
fn a_stronger_bolt_does_more_damage_and_defence_blocks_some_of_it() {
    let soft = target(1, 100, 0);
    let (weak, strong) = (
        mean(Volley::Bolt, &bolt(5), &soft, 2000),
        mean(Volley::Bolt, &bolt(15), &soft, 2000),
    );
    // A die hits three times in ten.
    assert!((weak - 1.5).abs() < 0.2, "{weak}");
    assert!((strong - 4.5).abs() < 0.4, "{strong}");
    let armoured = mean(Volley::Bolt, &bolt(15), &target(1, 100, 6), 2000);
    assert!(armoured < strong - 1.0, "{armoured} against {strong}");
    // Armour piercing halves the defence; doom ignores it.
    let piercing = Combatant {
        nature: AttackNature {
            armor_piercing: true,
            ..Default::default()
        },
        ..bolt(15)
    };
    let doom = {
        let mut doom = bolt(15);
        doom.nature.types.insert(DamageType::Doom);
        doom
    };
    let wall = target(1, 100, 6);
    assert!(mean(Volley::Bolt, &piercing, &wall, 2000) > armoured + 0.5);
    assert_eq!(mean(Volley::Bolt, &doom, &wall, 10), 15.0);
}

#[test]
fn warp_lightning_is_a_chain_of_weaker_and_weaker_bolts() {
    let chain = {
        let mut c = bolt(4);
        c.nature.types.insert(DamageType::Doom);
        c
    };
    let mut state = target(10, 5, 0);
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    let dmg = spell_attack(&rules(), &chain, Volley::Chain, &mut state, &mut rng);
    assert_eq!(dmg.points, 4 + 3 + 2 + 1);
}

#[test]
fn magic_immunity_and_righteousness_turn_spells_away() {
    // A spell of the realm hits the unit `foe` of the content, blessed or not.
    let strike = |foe: u16, blessed: bool, realm| {
        let mut b = real_battle(
            &[(104 /* high_men_spearmen */, (3, 5))],
            &[(foe, (8, 5))],
            1,
        );
        if blessed {
            b.units[1].put_enchantment(SpellId::new(144));
        }
        let nature = AttackNature::magical(realm, &[]);
        b.spell_damage(1, 10, &DamageTraits::default(), nature)
    };
    let paladins = 113; // high_men_paladins: magic immunity
    assert!(strike(104 /* high_men_spearmen */, false, Realm::Chaos).is_some());
    assert!(strike(paladins, false, Realm::Chaos).is_none());
    assert!(strike(104 /* high_men_spearmen */, true, Realm::Death).is_none());
    assert!(
        strike(104 /* high_men_spearmen */, true, Realm::Nature).is_some(),
        "only chaos and death"
    );
}

#[test]
fn healing_brings_figures_back_and_a_new_creature_arrives_ready_to_act() {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 5))],
        &[(104 /* high_men_spearmen */, (8, 5))],
        1,
    );
    let hits = b.units[0].hits;
    b.units[0].set_figures(1);
    b.units[0].set_front_damage(0);
    assert_eq!(b.units[0].hit_points(), hits);
    // Each figure has `hits` hit points: five figures' worth heal five figures.
    assert_eq!(b.spell_heal(0, 5 * hits), 5 * hits);
    assert_eq!(b.units[0].figures, 6);
    b.units[0].end_moves();
    let fresh = unit_of(&b, 104 /* high_men_spearmen */);
    let id = b.add_summoned(Side::Attacker, fresh, (4, 5));
    assert_eq!(id, 2);
    assert_eq!(b.units[2].moves_left, b.units[2].moves);
    assert!(b.units[2].is_active());
}

#[test]
fn creatures_appear_only_on_a_free_square_of_the_casters_half() {
    let b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 5))],
        &[(104 /* high_men_spearmen */, (17, 5))],
        1,
    );
    assert!(b.is_summon_square(Side::Attacker, (4, 5)));
    assert!(!b.is_summon_square(Side::Attacker, (3, 5)), "occupied");
    assert!(
        !b.is_summon_square(Side::Attacker, (15, 5)),
        "the other half"
    );
    assert!(b.is_summon_square(Side::Defender, (15, 5)));
    assert!(b
        .first_summon_square(Side::Attacker)
        .is_some_and(|c| c.0 == 0));
}
