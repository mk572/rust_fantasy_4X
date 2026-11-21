//! What lasting battle spells do to the units they touch: webs, binding and seizing, the resistance
//! checks of destroying spells.
//!
//! Tests: this is the test file.

use f4x_battle::battle::BattleEvent;
use f4x_battle::testutil::*;
use f4x_battle::{Confusion, Fate, FieldBonus, Side, Status};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::SpellId;
use f4x_def::testdir::{shipped, TestResult};

#[test]
fn what_a_unit_brings_to_the_web_follows_its_attacks() {
    // The strength of unit `number` as it ships; `ammo` of 0 for a unit that has shot its last.
    let web = |number: u16, ammo: Option<i32>| {
        let mut u = unit_numbered(number);
        if let Some(ammo) = ammo {
            u.set_ammo(ammo);
        }
        (
            u.web_strength(&shipped().rules, FieldBonus::default()),
            u.own_melee,
            u.own_ranged,
        )
    };
    let (w, melee, _) = web(105 /* high_men_swordsmen */, None);
    assert_eq!(w, melee, "melee alone");
    // A bow, a sling or a magical shot is the better of the two, and needs a shot left.
    let (w, melee, ranged) = web(101 /* high_elf_longbowmen */, None);
    assert_eq!(w, melee.max(ranged));
    let (w, melee, ranged) = web(103 /* high_elf_pegasai */, None);
    assert_eq!(w, melee.max(ranged), "the better of the two");
    let (w, melee, _) = web(101, Some(0));
    assert_eq!(w, melee, "no arrows left");
    let (w, melee, ranged) = web(94 /* halfling_slingers */, None);
    assert_eq!(w, melee.max(ranged));
    let (w, melee, ranged) = web(51 /* beastmen_magicians, fireball */, None);
    assert_eq!(w, melee.max(ranged), "a magical attack");
    // A boulder is bodily strength like melee: it does not count.
    let (w, melee, _) = web(37 /* catapult */, None);
    assert_eq!(w, melee);
    // Thrown weapons, breath and gazes add to melee.
    for number in [
        40,  // barbarian_swordsmen, thrown
        74,  // draconian_doom_drakes, fire breath
        196, // sky_drake, lightning breath
        182, // basilisk, stoning gaze
    ] {
        let (w, melee, ranged) = web(number, None);
        assert_eq!(w, melee + ranged, "unit {number}");
    }
    // Immolation burns the web as well.
    let (w, melee, _) = web(161 /* doom_bat */, None);
    assert_eq!(w, melee + rules().immolation_strength);
}

#[test]
fn a_bound_unit_fights_for_the_caster_and_goes_home_when_the_battle_ends() {
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (3, 5))],
        &[
            (104 /* high_men_spearmen */, (17, 5)),
            (104 /* high_men_spearmen */, (17, 7)),
        ],
        1,
    );
    b.change_sides(1, Side::Attacker);
    let u = &b.units[1];
    assert_eq!(
        (u.side, u.bound_from),
        (Side::Attacker, Some(Side::Defender))
    );
    assert!(
        u.attacked && u.moves_left == 0,
        "it acts from the next turn"
    );
    // A new turn does not hand it back, as confusion does.
    end_turns(&mut b, 2);
    assert_eq!(b.units[1].side, Side::Attacker);
    // The last of the rival's own units falls: the caster wins, and the unit goes home.
    b.units[2].status = Status::Dead;
    b.check_winner();
    assert_eq!(b.winner, Some(Side::Attacker));
    assert_eq!(b.units[1].side, Side::Defender);
    assert_eq!(b.units[1].bound_from, None);
}

#[test]
fn a_creature_still_bound_when_the_battle_ends_is_destroyed() {
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (3, 5))],
        &[
            (104 /* high_men_spearmen */, (17, 5)),
            (104 /* high_men_spearmen */, (17, 7)),
        ],
        1,
    );
    b.units[1].put_enchantment(SpellId::new(73 /* creature_binding */));
    b.change_sides(1, Side::Attacker);
    b.units[2].status = Status::Dead;
    b.check_winner();
    let u = &b.units[1];
    assert_eq!((u.status, u.side), (Status::Dead, Side::Defender));
    assert_eq!(u.fate(), Fate::Gone);
}

#[test]
fn a_seized_unit_acts_for_the_enemy_for_a_turn_and_goes_home_when_its_side_has_others() {
    let mut seen = 0;
    for seed in 0..200 {
        let mut b = real_battle(
            &[(105 /* high_men_swordsmen */, (3, 5))],
            &[
                (104 /* high_men_spearmen */, (8, 5)),
                (104 /* high_men_spearmen */, (9, 5)),
            ],
            seed,
        );
        b.units[1].put_enchantment(SpellId::new(46 /* confusion */));
        let events = end_turns(&mut b, 1);
        let seized = events.iter().any(|e| {
            matches!(
                e,
                BattleEvent::Confused {
                    unit: 1,
                    act: Confusion::Seized
                }
            )
        });
        if !seized {
            continue;
        }
        seen += 1;
        assert!(b.winner.is_none(), "a side with another unit goes on");
        assert!(b.units[1].attacked && b.units[1].seized);
        // The enemy commands it on their turn, and it goes home after.
        end_turns(&mut b, 1);
        assert_eq!(b.units[1].side, Side::Attacker);
        assert!(b.units[1].moves_left > 0 && !b.units[1].attacked);
        end_turns(&mut b, 1);
        assert_eq!(b.units[1].side, Side::Defender);
    }
    assert!(seen > 10, "{seen}");
}

#[test]
fn figures_that_fail_their_resistance_are_destroyed() -> TestResult {
    let slay = |foe: u16, resistance: i32| {
        let mut b = real_battle(
            &[(105 /* high_men_swordsmen */, (5, 5))],
            &[(foe, (6, 5))],
            3,
        );
        b.units[1].force(f4x_def::rules::stats::Stat::Resistance, resistance);
        b
    };
    let mut b = slay(104 /* high_men_spearmen */, 0);
    let figures = b.units[1].figures;
    let dmg = b
        .spell_slay(1, Realm::Death, -9, &[], None)
        .ok_or("spell_slay is missing")?;
    assert_eq!(dmg.kills, figures, "no chance against a penalty of nine");
    assert!(!b.units[1].is_active());
    // A unit with the right immunity (skeletons) takes no harm; one with enough resistance
    // resists.
    let mut b = slay(165 /* skeletons */, 30);
    assert!(b
        .spell_slay(
            1,
            Realm::Death,
            -2,
            &[f4x_def::rules::damage_type::DamageType::Death],
            None
        )
        .is_none());
    assert_eq!(
        b.spell_slay(1, Realm::Death, -2, &[], None)
            .ok_or("spell_slay is missing")?
            .kills,
        0
    );
    // Disintegrate: resistance under ten and the whole unit is gone.
    let mut b = slay(104 /* high_men_spearmen */, 9);
    b.spell_slay(1, Realm::Chaos, 0, &[], Some(10))
        .ok_or("spell_slay is missing")?;
    assert!(!b.units[1].is_active());
    let mut b = slay(104 /* high_men_spearmen */, 10);
    b.spell_slay(1, Realm::Chaos, 0, &[], Some(10))
        .ok_or("spell_slay is missing")?;
    assert!(b.units[1].is_active());
    Ok(())
}

/// The defence of unit `i` as the battle has it now.
fn defense_of(b: &f4x_battle::battle::Battle, i: usize) -> i32 {
    b.now(i).expect("the unit is there").defense()
}

#[test]
fn a_holy_bonus_goes_at_once_when_its_unit_dies_and_comes_with_a_summon() {
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (3, 5))],
        &[(104 /* high_men_spearmen */, (17, 5))],
        1,
    );
    let (plain, enemy) = (defense_of(&b, 0), defense_of(&b, 1));
    let angel = b.add_summoned(Side::Attacker, unit_numbered(178 /* arch_angel */), (3, 6));
    let level = b.units[angel].special_level;
    assert!(level > 0);
    // Nothing was refreshed: the bonus is there as the summon is.
    assert_eq!(defense_of(&b, 0), plain + level);
    assert_eq!(defense_of(&b, 1), enemy, "the enemy gets none");
    // Nothing is called after the death: the bonus is gone.
    b.units[angel].status = Status::Dead;
    assert_eq!(defense_of(&b, 0), plain);
}

#[test]
fn a_field_taken_down_takes_its_numbers_with_it_at_once() {
    let mut b = real_battle(
        &[(177 /* angel */, (3, 5))],
        &[(171 /* shadow_demons */, (17, 5))],
        3,
    );
    let (life, death) = (defense_of(&b, 0), defense_of(&b, 1));
    let dark = b
        .content
        .rules
        .spells
        .iter()
        .find(|(_, d)| {
            f4x_battle::battlemagic::field::Field::of(d)
                == Some(f4x_battle::battlemagic::field::Field::Darkness)
        })
        .map(|(id, _)| id)
        .expect("a shipped spell raises Darkness");
    b.raise_field(
        Side::Attacker,
        f4x_battle::battlemagic::field::Field::Darkness,
        dark,
    );
    assert_eq!(
        (defense_of(&b, 0), defense_of(&b, 1)),
        (life - 1, death + 1)
    );
    b.take_down_field(
        Side::Attacker,
        f4x_battle::battlemagic::field::Field::Darkness,
    );
    assert_eq!((defense_of(&b, 0), defense_of(&b, 1)), (life, death));
}
