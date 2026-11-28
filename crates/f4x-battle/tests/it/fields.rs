//! What lasting battle spells do to the units they touch: webs, binding and seizing, the resistance
//! checks of destroying spells.
//!
//! Tests: this is the test file.

use f4x_battle::battle::Battle;
use f4x_battle::battle::BattleEvent;
use f4x_battle::battlemagic::field::Field;
use f4x_battle::testutil::*;
use f4x_battle::{Confusion, Fate, FieldBonus, Side, Status};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::realm::Realm;
use f4x_def::rules::Ability;
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

/// What Wrack did to spearmen (eight figures) with plenty of hit points over ten turns: the
/// points of each of its strikes.
fn wrack_strikes(resistance: i32, abilities: &[u16]) -> Vec<i32> {
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    b.units[1].force(f4x_def::rules::stats::Stat::Resistance, resistance);
    b.units[1].force(f4x_def::rules::stats::Stat::MaxHp, 1000);
    for &a in abilities {
        b.units[1].grant(Ability::new(a));
    }
    b.raise_field(Side::Attacker, Field::Wrack, spell_for(Field::Wrack));
    end_turns(&mut b, 10)
        .iter()
        .filter_map(|e| match e {
            BattleEvent::FieldStruck { unit: 1, dealt, .. } => Some(*dealt),
            _ => None,
        })
        .collect()
}

#[test]
fn light_helps_life_and_hurts_death_and_darkness_does_the_opposite() {
    let mut b = pair(177 /* angel */, 171 /* shadow_demons */);
    let (life, death) = (was(&b, 0), was(&b, 1));
    assert!(death.ranged > 0);
    b.raise_field(
        Side::Attacker,
        Field::TrueLight,
        spell_for(Field::TrueLight),
    );
    let (l, d) = (&b.units[0], &b.units[1]);
    assert_eq!(
        (
            b.now_of(l).melee(),
            b.now_of(l).defense(),
            b.now_of(l).resistance()
        ),
        (life.melee + 1, life.defense + 1, life.resistance + 1)
    );
    assert_eq!(
        (
            b.now_of(d).melee(),
            b.now_of(d).ranged(),
            b.now_of(d).defense(),
            b.now_of(d).resistance()
        ),
        (
            death.melee - 1,
            death.ranged - 1,
            death.defense - 1,
            death.resistance - 1
        )
    );
    // Darkness by the other side cancels it, whoever cast either.
    b.raise_field(Side::Defender, Field::Darkness, spell_for(Field::Darkness));
    assert_eq!(b.now(0).unwrap().melee(), life.melee);
    assert_eq!(b.now(1).unwrap().resistance(), death.resistance);
    // Darkness alone is the reverse.
    let mut b = pair(177 /* angel */, 171 /* shadow_demons */);
    b.raise_field(Side::Attacker, Field::Darkness, spell_for(Field::Darkness));
    assert_eq!(b.now(0).unwrap().defense(), life.defense - 1);
    assert_eq!(b.now(1).unwrap().defense(), death.defense + 1);
}

#[test]
fn holy_bonus_and_resistance_to_all_help_the_whole_side_and_do_not_add_up() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    let plain = was(&b, 0);
    let enemy = was(&b, 1);
    // A unit without a melee attack gets defence and resistance but no attack.
    let settlers = b.add_summoned(
        Side::Attacker,
        unit_numbered(73 /* draconian_settlers */),
        (3, 7),
    );
    let settlers_before = was(&b, settlers);
    assert_eq!(settlers_before.melee, 0);
    let arch_angel = b.add_summoned(Side::Attacker, unit_numbered(178), (3, 6));
    let level = b.units[arch_angel].special_level;
    assert!(level >= 2);
    let (p, a) = (&b.units[0], &b.units[settlers]);
    assert_eq!(
        (
            b.now_of(p).melee(),
            b.now_of(p).defense(),
            b.now_of(p).resistance()
        ),
        (
            plain.melee + level,
            plain.defense + level,
            plain.resistance + level
        )
    );
    assert_eq!(
        (b.now_of(a).melee(), b.now_of(a).defense()),
        (0, settlers_before.defense + level)
    );
    // The bearer gets it too, as it does on its own: the same numbers as alone.
    assert_eq!(
        b.now(arch_angel).unwrap().melee(),
        pair(178 /* arch_angel */, 104).now(0).unwrap().melee(),
        "the bearer gets it too"
    );
    assert_eq!(
        b.now(1).unwrap().defense(),
        enemy.defense,
        "the enemy gets nothing"
    );
    // A second, weaker holy unit (an angel) does not add, and Resistance to All (a guardian
    // spirit, level 1) adds to the resistance only.
    b.add_summoned(Side::Attacker, unit_numbered(177 /* angel */), (4, 6));
    assert_eq!(b.now(0).unwrap().defense(), plain.defense + level);
    b.add_summoned(
        Side::Attacker,
        unit_numbered(176 /* guardian_spirit */),
        (4, 7),
    );
    let p = &b.units[0];
    assert_eq!(b.now_of(p).melee(), plain.melee + level);
    assert!(b.now_of(p).resistance() >= plain.resistance + level);
    // When the bearer falls the bonus goes at the next refresh.
    b.units[arch_angel].status = Status::Dead;
    b.note_invisibility();
    assert_eq!(
        b.now(0).unwrap().defense(),
        plain.defense + 1,
        "the weaker one still stands"
    );
}

#[test]
fn a_charmed_heros_resistance_passes_every_check() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        105, /* high_men_swordsmen */
    );
    let charmed = b.now(1).unwrap().resistance() + shipped().rules.config.renown.charmed_resistance;
    b.units[1].force(f4x_def::rules::stats::Stat::Resistance, charmed);
    let fails = |b: &mut Battle, i: usize| {
        (0..40)
            .filter(|_| b.resist_fails(i, Some(Realm::Death), 0))
            .count()
    };
    assert!(fails(&mut b, 0) > 0);
    assert_eq!(fails(&mut b, 1), 0);
}

#[test]
fn refreshing_does_not_pile_the_bonuses_up_and_they_come_off_again() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    let before = b.units[0].clone();
    b.raise_field(
        Side::Attacker,
        Field::HighPrayer,
        spell_for(Field::HighPrayer),
    );
    for _ in 0..3 {
        b.note_invisibility();
    }
    let u = &b.units[0];
    assert_eq!(
        (
            b.now_of(u).melee(),
            b.now_of(u).defense(),
            b.now_of(u).resistance(),
            b.now_of(u).to_hit(),
            b.now_of(u).to_block()
        ),
        (
            before.own_melee + 2,
            before.own_defense + 2,
            before.own_resistance + 3,
            before.own_to_hit + 1,
            before.own_to_block + 1
        )
    );
    assert_eq!(b.now(1).unwrap().melee(), 1, "the enemy gets nothing");
    // Prayer is the lesser one and High Prayer wins when both are up.
    b.raise_field(Side::Attacker, Field::Prayer, spell_for(Field::Prayer));
    assert_eq!(b.now(0).unwrap().resistance(), before.own_resistance + 3);
    b.fields[Side::Attacker.index()].clear();
    b.note_invisibility();
    let u = &b.units[0];
    assert_eq!(
        (
            b.now_of(u).melee(),
            b.now_of(u).defense(),
            b.now_of(u).resistance(),
            b.now_of(u).to_hit(),
            b.now_of(u).to_block()
        ),
        (
            before.own_melee,
            before.own_defense,
            before.own_resistance,
            before.own_to_hit,
            before.own_to_block
        )
    );
}

#[test]
fn black_prayer_curses_the_enemy_and_warp_reality_every_unit_but_chaos() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        155, /* hell_hounds */
    );
    let (ours, enemy) = (b.units[0].clone(), b.units[1].clone());
    assert!(enemy.own_ranged > 0);
    b.raise_field(
        Side::Attacker,
        Field::BlackPrayer,
        spell_for(Field::BlackPrayer),
    );
    let e = &b.units[1];
    assert_eq!(
        (
            b.now_of(e).melee(),
            b.now_of(e).ranged(),
            b.now_of(e).defense(),
            b.now_of(e).resistance()
        ),
        (
            enemy.own_melee - 1,
            enemy.own_ranged - 1,
            enemy.own_defense - 1,
            enemy.own_resistance - 2
        )
    );
    assert_eq!(
        b.now(0).unwrap().melee(),
        ours.own_melee,
        "its own side is untouched"
    );
    b.raise_field(
        Side::Defender,
        Field::WarpReality,
        spell_for(Field::WarpReality),
    );
    assert_eq!(b.now(0).unwrap().to_hit(), ours.own_to_hit - 2);
    assert_eq!(
        b.now(1).unwrap().to_hit(),
        enemy.own_to_hit,
        "chaos creatures are not warped"
    );
}

/// A battlefield spell changes a magic-immune unit when the content marks it as an exception
/// and the rule `magic_immunity_exceptions` is on, and not otherwise.
#[test]
fn battlefield_enchantments_change_a_magic_immune_unit_as_the_exceptions_say() {
    // High Men paladins have magic immunity.
    for (field, exceptions) in [Field::BlackPrayer, Field::WarpReality]
        .into_iter()
        .flat_map(|f| [(f, false), (f, true)])
    {
        let mut b = pair(
            105, /* high_men_swordsmen */
            113, /* high_men_paladins */
        );
        b.magic_immunity_exceptions = exceptions;
        let marked = b
            .content
            .rules
            .spells
            .get(spell_for(field))
            .magic_immunity_exception;
        let before = was(&b, 1);
        let caster = if field == Field::WarpReality {
            Side::Defender
        } else {
            Side::Attacker
        };
        b.raise_field(caster, field, spell_for(field));
        let after = &b.units[1];
        assert_eq!(
            (
                b.now_of(after).melee(),
                b.now_of(after).defense(),
                b.now_of(after).resistance(),
                b.now_of(after).to_hit()
            ) != (
                before.melee,
                before.defense,
                before.resistance,
                before.to_hit
            ),
            marked && exceptions,
            "{field:?}, exceptions {exceptions}"
        );
    }
    // Entangle takes a move from it when the exceptions are on.
    let mut b = pair(
        105, /* high_men_swordsmen */
        113, /* high_men_paladins */
    );
    b.magic_immunity_exceptions = true;
    b.raise_field(Side::Attacker, Field::Entangle, spell_for(Field::Entangle));
    let moves = b.units[1].moves;
    end_turns(&mut b, 1);
    assert!(b.units[1].moves_left < moves, "Entangle holds it");
}

/// Task 713: Black Prayer curses the enemy, but not a unit immune to death, unless the
/// original's exceptions are on.
#[test]
fn black_prayer_spares_the_immune_to_death_unless_the_original_exceptions_are_on() {
    for (exceptions, cursed) in [(false, false), (true, true)] {
        let mut b = pair(105 /* high_men_swordsmen */, 165 /* skeletons */);
        b.death_immunity_exceptions = exceptions;
        let before = b.now(1).unwrap().defense();
        b.raise_field(
            Side::Attacker,
            Field::BlackPrayer,
            spell_for(Field::BlackPrayer),
        );
        assert_eq!(
            b.now(1).unwrap().defense() < before,
            cursed,
            "exceptions {exceptions}"
        );
    }
}

/// Task 713: Terror, the same way.
#[test]
fn terror_spares_the_immune_to_death_unless_the_original_exceptions_are_on() {
    for exceptions in [false, true] {
        let mut soft = pair(105 /* high_men_swordsmen */, 165 /* skeletons */);
        soft.death_immunity_exceptions = exceptions;
        soft.units[1].force(f4x_def::rules::stats::Stat::Resistance, 0);
        soft.raise_field(Side::Attacker, Field::Terror, spell_for(Field::Terror));
        let mut frozen = 0;
        for seed in 0..40 {
            let mut b = reseeded(&soft, seed);
            end_turns(&mut b, 1);
            if b.units[1].attacked && b.units[1].moves_left == 0 {
                frozen += 1;
            }
        }
        if exceptions {
            assert!(frozen > 0, "{frozen} of 40 froze");
        } else {
            assert_eq!(frozen, 0, "death immunity stops Terror");
        }
    }
}

#[test]
fn a_loss_never_takes_a_stat_below_nothing_and_comes_back_whole() {
    // Settlers have no melee attack at all.
    let mut b = pair(
        105, /* high_men_swordsmen */
        73,  /* draconian_settlers */
    );
    assert_eq!(b.now(1).unwrap().melee(), 0);
    b.raise_field(
        Side::Attacker,
        Field::BlackPrayer,
        spell_for(Field::BlackPrayer),
    );
    assert_eq!(b.now(1).unwrap().melee(), 0);
    b.fields[0].clear();
    b.note_invisibility();
    assert_eq!(b.now(1).unwrap().melee(), 0, "not -1 + 1");
}

#[test]
fn entangle_costs_a_move_and_terror_can_freeze_the_enemy() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    b.raise_field(Side::Attacker, Field::Entangle, spell_for(Field::Entangle));
    let moves = b.units[1].moves;
    end_turns(&mut b, 1);
    assert_eq!(b.active, Side::Defender);
    assert_eq!(
        b.units[1].moves_left,
        moves - HalfInt::from_int(1),
        "one whole move less"
    );
    // Fliers (pegasai) and the non-corporeal (wraiths) are not held.
    for free in [103, 170] {
        let mut b = pair(105 /* high_men_swordsmen */, free);
        b.raise_field(Side::Attacker, Field::Entangle, spell_for(Field::Entangle));
        end_turns(&mut b, 1);
        assert_eq!(b.units[1].moves_left, b.units[1].moves, "{free}");
    }
    // Terror: a unit with no resistance never saves.
    let mut soft = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    soft.units[1].force(f4x_def::rules::stats::Stat::Resistance, 0);
    soft.raise_field(Side::Attacker, Field::Terror, spell_for(Field::Terror));
    let mut frozen = 0;
    for seed in 0..40 {
        let mut b = reseeded(&soft, seed);
        end_turns(&mut b, 1);
        if b.units[1].attacked && b.units[1].moves_left == 0 {
            frozen += 1;
        }
    }
    assert!(frozen > 30, "{frozen} of 40 froze");
    // Death immunity (skeletons) is proof against it.
    let mut deathless = pair(105 /* high_men_swordsmen */, 165 /* skeletons */);
    deathless.units[1].force(f4x_def::rules::stats::Stat::Resistance, 0);
    deathless.raise_field(Side::Attacker, Field::Terror, spell_for(Field::Terror));
    for seed in 0..20 {
        let mut b = reseeded(&deathless, seed);
        end_turns(&mut b, 1);
        assert!(!b.units[1].attacked, "seed {seed}");
    }
}

#[test]
fn wrack_and_call_lightning_hurt_the_casters_enemies_at_the_start_of_each_turn() {
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    b.units[1].force(f4x_def::rules::stats::Stat::Resistance, 0);
    b.raise_field(Side::Attacker, Field::Wrack, spell_for(Field::Wrack));
    end_turns(&mut b, 2);
    assert_eq!(b.turn, 2);
    let hurt = b.units[1].max_hit_points() - b.units[1].hit_points();
    assert!(hurt >= 1, "a figure with no resistance feels it");
    assert_eq!(b.units[0].hit_points(), b.units[0].max_hit_points());
    // Wraith Form is no shelter from it.
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    b.units[1].force(f4x_def::rules::stats::Stat::Resistance, 0);
    b.units[1].put_enchantment(SpellId::new(181 /* wraith_form */));
    b.raise_field(Side::Attacker, Field::Wrack, spell_for(Field::Wrack));
    end_turns(&mut b, 4);
    assert!(b.units[1].hit_points() < b.units[1].max_hit_points());
    // Call Lightning: bolts of strength eight on the enemy only.
    let mut b = pair(
        105, /* high_men_swordsmen */
        104, /* high_men_spearmen */
    );
    b.units[1].force(f4x_def::rules::stats::Stat::Defense, 0);
    b.units[1].force(f4x_def::rules::stats::Stat::MaxHp, 100);
    b.raise_field(
        Side::Attacker,
        Field::CallLightning,
        spell_for(Field::CallLightning),
    );
    let events = end_turns(&mut b, 2);
    let bolts = events
        .iter()
        .filter(|e| matches!(e, BattleEvent::SpellDamaged { unit: 1, .. }))
        .count();
    assert!(bolts <= 5, "{bolts}");
    assert_eq!(b.units[0].hit_points(), b.units[0].max_hit_points());
    // Each bolt is an event of its own, on the unit it fell on, ahead of what it did.
    let struck: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            BattleEvent::LightningStruck { unit } => Some(*unit),
            _ => None,
        })
        .collect();
    assert!(struck.iter().all(|&u| u == 1), "{struck:?}");
    assert!(
        (3..=10).contains(&struck.len()),
        "two turns of 3 to 5: {struck:?}"
    );
}

#[test]
fn wrack_deals_one_point_for_each_figure_that_fails_its_roll_and_not_the_square_of_that() {
    // The original deals n x n points when n figures fail (the wiki lists it as a bug).
    let strikes = wrack_strikes(0, &[]);
    assert!(strikes.len() >= 5, "{strikes:?}");
    assert!(strikes.iter().all(|&d| (1..=8).contains(&d)), "{strikes:?}");
}

#[test]
fn wrack_spares_a_resistance_of_nine_and_the_immune_and_hurts_a_resistance_of_eight() {
    assert!(
        wrack_strikes(9, &[]).is_empty(),
        "nine and the +1 cannot fail"
    );
    assert!(wrack_strikes(0, &[16 /* death_immunity */]).is_empty());
    assert!(wrack_strikes(0, &[40 /* magic_immunity */]).is_empty());
    assert!(
        !wrack_strikes(8, &[]).is_empty(),
        "a roll of ten still beats eight and the +1"
    );
}

#[test]
fn mana_leak_drains_the_enemy_wizard_and_its_spell_shots() -> TestResult {
    // Beastmen magicians shoot spells; their wizard has a reserve.
    let mut b = pair(
        105, /* high_men_swordsmen */
        51,  /* beastmen_magicians */
    );
    let ammo = b.units[1].ammo;
    assert!(ammo > 0);
    b.set_caster(
        Side::Defender,
        f4x_battle::battlemagic::caster::BattleCaster::new(20, 40, 10),
    );
    b.raise_field(Side::Attacker, Field::ManaLeak, spell_for(Field::ManaLeak));
    end_turns(&mut b, 2);
    assert_eq!(
        b.caster(Side::Defender).ok_or("caster is missing")?.mana,
        40 - rules().mana_leak_drain
    );
    assert_eq!(b.units[1].ammo, ammo - 1);
    Ok(())
}

#[test]
fn blur_takes_one_off_the_chance_to_hit_unless_the_attacker_sees_through_illusions() {
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (5, 5))],
        &[(104 /* high_men_spearmen */, (9, 5))],
        3,
    );
    assert_eq!(b.blur_penalty(0, 1), 0);
    b.raise_field(Side::Defender, Field::Blur, spell_for(Field::Blur));
    assert_eq!(b.blur_penalty(0, 1), 1, "whatever the distance");
    assert_eq!(
        b.blur_penalty(1, 0),
        0,
        "only the blurred side's units are harder to hit"
    );
    // Skeletons see through illusions.
    let mut b = real_battle(
        &[(165 /* skeletons */, (5, 5))],
        &[(104 /* high_men_spearmen */, (9, 5))],
        3,
    );
    b.raise_field(Side::Defender, Field::Blur, spell_for(Field::Blur));
    assert_eq!(b.blur_penalty(0, 1), 0);
}

#[test]
fn an_invisible_creature_is_harder_to_hit_unless_the_attacker_sees_through_illusions() {
    // A night stalker is invisible.
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (5, 5))],
        &[(167 /* night_stalker */, (6, 5))],
        3,
    );
    assert_eq!(b.invisibility_penalty(0, 1), 1, "adjacent");
    assert_eq!(b.invisibility_penalty(1, 0), 0);
    b.units[0].cell = (1, 5);
    assert_eq!(b.invisibility_penalty(0, 1), 5, "one for each square");
    let b = real_battle(
        &[(165 /* skeletons */, (5, 5))],
        &[(167 /* night_stalker */, (6, 5))],
        3,
    );
    assert_eq!(b.invisibility_penalty(0, 1), 0, "skeletons see through it");
    // Mass Invisibility does the same for the caster's whole side.
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (5, 5))],
        &[(104 /* high_men_spearmen */, (6, 5))],
        3,
    );
    b.raise_field(
        Side::Defender,
        Field::MassInvisibility,
        spell_for(Field::MassInvisibility),
    );
    assert_eq!(b.invisibility_penalty(0, 1), 1);
}
