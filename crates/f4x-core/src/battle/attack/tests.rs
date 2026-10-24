use super::*;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn rng(seed: u64) -> Pcg64Mcg {
    Pcg64Mcg::seed_from_u64(seed)
}

fn def(figures: u32, hits: i32, defense: i32) -> DefenderState {
    DefenderState {
        hits,
        defense,
        to_block: 0,
        figures,
        front_damage: 0,
        ..DefenderState::default()
    }
}

fn combatant(strength: i32, to_hit: i32) -> Combatant {
    Combatant {
        strength,
        to_hit,
        ..Combatant::default()
    }
}

#[test]
fn a_die_hits_thirty_percent_and_each_plus_adds_ten() {
    let mut r = rng(1);
    let n = 20_000;
    for (pluses, expected) in [(0, 30), (1, 40), (3, 60), (-1, 20), (-5, 10), (9, 100)] {
        let hits = roll_successes(n, pluses, &mut r);
        let pct = f64::from(hits) / f64::from(n) * 100.0;
        assert!(
            (pct - f64::from(expected)).abs() < 1.5,
            "{pluses} pluses: {pct:.1}% vs {expected}%"
        );
    }
    assert_eq!(roll_successes(0, 0, &mut r), 0);
    assert_eq!(roll_successes(-3, 0, &mut r), 0);
}

#[test]
fn defense_blocks_hits() {
    // Strong attackers against a strong defense average less than against none.
    let mut r = rng(2);
    let mut open = 0;
    let mut armoured = 0;
    for _ in 0..2_000 {
        let mut a = def(100, 100, 0);
        let mut b = def(100, 100, 6);
        open += process_attack(&combatant(5, 0), 4, &mut a, &mut r).points;
        armoured += process_attack(&combatant(5, 0), 4, &mut b, &mut r).points;
    }
    assert!(open > armoured * 2, "{open} vs {armoured}");
    // Four figures with five dice at 30 percent hit six times each on average.
    let per_attack = f64::from(open) / 2_000.0;
    assert!((per_attack - 6.0).abs() < 0.5, "{per_attack}");
}

#[test]
fn damage_goes_to_the_front_figure_and_overflows_to_the_next() {
    let mut r = rng(3);
    // Unblockable, certain hits (strength 10 at +7 always rolls 1+ ... use doom damage).
    let doom = Combatant {
        strength: 5,
        ignores_defense: true,
        ..Combatant::default()
    };
    let mut d = def(3, 2, 9);
    let dmg = process_attack(&doom, 1, &mut d, &mut r);
    // Five damage on 2-hit figures: two figures dead, one point on the third.
    assert_eq!(
        dmg,
        Damage {
            points: 5,
            kills: 2,
            healed: 0,
            ..Damage::default()
        }
    );
    assert_eq!((d.figures, d.front_damage), (1, 1));
    assert_eq!(d.hit_points(), 1);
    // The wounded front figure needs only one more.
    let dmg = process_attack(&doom, 1, &mut d, &mut r);
    assert_eq!(
        dmg,
        Damage {
            points: 1,
            kills: 1,
            healed: 0,
            ..Damage::default()
        }
    );
    assert_eq!(d.figures, 0);
    assert_eq!(d.hit_points(), 0);
    // Nothing left to hit.
    assert_eq!(process_attack(&doom, 3, &mut d, &mut r), Damage::default());
}

#[test]
fn damage_never_exceeds_what_the_defender_has() {
    let mut r = rng(4);
    for seed in 0..200 {
        let mut r2 = rng(seed);
        let mut d = def(3, 4, 2);
        let before = d.hit_points();
        let dmg = process_attack(&combatant(9, 3), 6, &mut d, &mut r2);
        assert_eq!(before - dmg.points, d.hit_points(), "seed {seed}");
        assert!(d.hit_points() >= 0);
        assert!(dmg.kills <= 3);
    }
    let _ = &mut r;
}

#[test]
fn the_same_seed_gives_the_same_battle() {
    let fight = |seed| {
        let mut r = rng(seed);
        let mut a = Fighter {
            melee: combatant(4, 0),
            thrown: None,
            first_strike: false,
            negate_first_strike: false,
            state: def(4, 3, 3),
        };
        let mut b = a.clone();
        let ex = melee_exchange(&mut a, &mut b, &mut r);
        (ex, a.state, b.state)
    };
    assert_eq!(fight(9), fight(9));
    let outcomes: std::collections::HashSet<_> = (0..40).map(|s| fight(s).0.dealt.points).collect();
    assert!(outcomes.len() > 3, "varied across seeds");
}

fn doomed(strength: i32) -> Combatant {
    Combatant {
        strength,
        ignores_defense: true,
        ..Combatant::default()
    }
}

fn fighter(figures: u32, hits: i32, melee: Combatant) -> Fighter {
    Fighter {
        melee,
        thrown: None,
        first_strike: false,
        negate_first_strike: false,
        state: def(figures, hits, 0),
    }
}

#[test]
fn the_defender_strikes_back_before_an_ordinary_attacker_swings() {
    let mut r = rng(5);
    // 3 attackers (1 hp each, melee 1 doom), 3 defenders (1 hp each, melee 1 doom).
    // The defender's counter kills all three attackers first, so nobody swings back.
    let mut att = fighter(3, 1, doomed(1));
    let mut def_ = fighter(3, 1, doomed(1));
    let ex = melee_exchange(&mut att, &mut def_, &mut r);
    assert_eq!(att.state.figures, 0);
    assert_eq!(ex.taken.kills, 3);
    assert_eq!(
        ex.dealt.kills, 0,
        "the attacker lost its figures before swinging"
    );
    assert_eq!(def_.state.figures, 3);
}

#[test]
fn first_strike_lets_the_attacker_hit_before_the_reply() {
    let mut r = rng(6);
    let mut att = fighter(3, 1, doomed(1));
    att.first_strike = true;
    let mut def_ = fighter(3, 1, doomed(1));
    let ex = melee_exchange(&mut att, &mut def_, &mut r);
    assert_eq!(
        def_.state.figures, 0,
        "the defenders fell before they could strike"
    );
    assert_eq!(ex.dealt.kills, 3);
    assert_eq!(ex.taken.kills, 0);
    assert_eq!(att.state.figures, 3);
    // Negate first strike takes the advantage away.
    let mut att = fighter(3, 1, doomed(1));
    att.first_strike = true;
    let mut def_ = fighter(3, 1, doomed(1));
    def_.negate_first_strike = true;
    melee_exchange(&mut att, &mut def_, &mut r);
    assert_eq!(
        att.state.figures, 0,
        "ordinary order again: the defender hit first"
    );
}

#[test]
fn thrown_attacks_open_the_exchange() {
    let mut r = rng(7);
    let mut att = fighter(2, 1, doomed(0));
    att.thrown = Some(doomed(2));
    let mut def_ = fighter(4, 1, doomed(1));
    let ex = melee_exchange(&mut att, &mut def_, &mut r);
    // Two figures throw two points each: all four defenders fall before they can answer.
    assert_eq!(ex.dealt.kills, 4);
    assert_eq!(att.state.figures, 2);
    assert_eq!(ex.taken, Damage::default());
}

#[test]
fn shooting_draws_no_reply_and_range_costs_accuracy() {
    let mut r = rng(8);
    let mut d = def(4, 2, 0);
    let shot = ranged_attack(&doomed(3), 2, &mut d, &mut r);
    assert_eq!(shot.points, 6);
    assert_eq!(range_penalty(0, false), 0);
    assert_eq!(range_penalty(2, false), 0);
    assert_eq!(range_penalty(3, false), 1);
    assert_eq!(range_penalty(9, false), 3);
    assert_eq!(range_penalty(9, true), 1, "long range caps the penalty");
    assert_eq!(range_penalty(2, true), 0);
}

#[test]
fn a_defender_rolls_the_defense_its_immunities_give_it() {
    use crate::battle::immunity::{AttackNature, Immunities};
    let golem = Immunities {
        weapon: true,
        ..Immunities::default()
    };
    let armoured = |immunities| DefenderState {
        immunities,
        ..def(100, 100, 0)
    };
    // Normal swords: strength 6, no defense of its own, but weapon immunity gives it ten.
    let sword = Combatant {
        strength: 6,
        mode: Mode::Melee,
        nature: AttackNature::of_unit_attack(Mode::Melee, &[], false),
        ..Combatant::default()
    };
    let magic_sword = Combatant {
        nature: AttackNature::of_unit_attack(Mode::Melee, &[], true),
        ..sword.clone()
    };
    let (mut plain, mut stone, mut stone_vs_magic) = (0, 0, 0);
    let mut r = rng(11);
    for _ in 0..1_000 {
        plain += process_attack(&sword, 4, &mut armoured(Immunities::default()), &mut r).points;
        stone += process_attack(&sword, 4, &mut armoured(golem), &mut r).points;
        stone_vs_magic += process_attack(&magic_sword, 4, &mut armoured(golem), &mut r).points;
    }
    assert!(stone * 4 < plain, "{stone} vs {plain}");
    assert!((f64::from(stone_vs_magic) / f64::from(plain) - 1.0).abs() < 0.1);
    // Fifty dice of defense stop all but a die in a thousand: arrows against missile
    // immunity do no damage at all over a thousand volleys.
    let arrows = Combatant {
        strength: 6,
        mode: Mode::Ranged,
        nature: AttackNature::of_unit_attack(Mode::Ranged, &["bow".to_string()], false),
        ..Combatant::default()
    };
    let archer_proof = Immunities {
        missiles: true,
        ..Immunities::default()
    };
    let mut through = 0;
    for _ in 0..1_000 {
        through += process_attack(&arrows, 4, &mut armoured(archer_proof), &mut r).points;
    }
    assert_eq!(through, 0);
    // Doom damage ignores even that.
    let doom = Combatant {
        ignores_defense: true,
        ..arrows
    };
    assert_eq!(
        process_attack(&doom, 1, &mut armoured(archer_proof), &mut r).points,
        6
    );
}

/// A defender that nothing blocks (defense 0) with the given resistance.
fn soft(figures: u32, hits: i32, resistance: i32) -> DefenderState {
    DefenderState {
        resistance,
        figures_max: figures,
        ..def(figures, hits, 0)
    }
}

/// A weak attacker (one die) with the given special attacks.
fn special(special: SpecialAttacks, mode: Mode) -> Combatant {
    Combatant {
        strength: 1,
        mode,
        special,
        ..Combatant::default()
    }
}

/// Average kills over many attacks.
fn mean_kills(
    attacker: &Combatant,
    figures: u32,
    mut target: impl FnMut() -> DefenderState,
) -> f64 {
    let mut r = rng(21);
    let n = 3_000;
    let total: u32 = (0..n)
        .map(|_| process_attack(attacker, figures, &mut target(), &mut r).kills)
        .sum();
    f64::from(total) / f64::from(n)
}

#[test]
fn a_touch_that_is_not_resisted_kills_a_figure_for_each_attacker() {
    let touch = |s: SpecialAttacks| special(s, Mode::Melee);
    let stoner = SpecialAttacks {
        stoning_touch: true,
        penalty: 3,
        ..SpecialAttacks::default()
    };
    // Resistance 4 at -3 fails on 2 to 10: nine times in ten; six attackers.
    let k = mean_kills(&touch(stoner), 6, || soft(40, 100, 4));
    assert!((k - 5.4).abs() < 0.3, "{k}");
    // Stoning immunity and magic immunity both stop it.
    let stone_proof = || DefenderState {
        immunities: Immunities {
            stoning: true,
            ..Immunities::default()
        },
        ..soft(40, 100, 4)
    };
    let warded = || DefenderState {
        immunities: Immunities {
            magic: true,
            ..Immunities::default()
        },
        ..soft(40, 100, 4)
    };
    assert!(mean_kills(&touch(stoner), 6, stone_proof) < 1.0);
    assert!(mean_kills(&touch(stoner), 6, warded) < 1.0);
    // A higher resistance resists more often.
    assert!(mean_kills(&touch(stoner), 6, || soft(40, 100, 12)) < k / 3.0);
    let death = SpecialAttacks {
        death_touch: true,
        penalty: 3,
        ..SpecialAttacks::default()
    };
    let d = mean_kills(&touch(death), 6, || soft(40, 100, 4));
    assert!((d - 5.4).abs() < 0.3, "{d}");
    // From an item or a spell the touches are weaker: 3 more for death, 1 more for stoning, so
    // a resistance of 4 at -6 (or -4) never holds.
    let by_item = SpecialAttacks {
        death_extra: 3,
        ..death
    };
    let g = mean_kills(&touch(by_item), 6, || soft(40, 100, 4));
    assert!(g > 5.95, "{g}");
    let stone_item = SpecialAttacks {
        stoning_extra: 1,
        ..stoner
    };
    let g = mean_kills(&touch(stone_item), 6, || soft(40, 100, 4));
    assert!(g > 5.95, "{g}");
    let destroyer = SpecialAttacks {
        destruction: true,
        ..SpecialAttacks::default()
    };
    // Destruction has no penalty: resistance 4 fails six times in ten.
    let x = mean_kills(&touch(destroyer), 6, || soft(40, 100, 4));
    assert!((x - 3.6).abs() < 0.3, "{x}");
}

#[test]
fn dispel_evil_works_on_creatures_of_chaos_and_death_only() {
    let angel = special(
        SpecialAttacks {
            dispel_evil: true,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let creature = |realm, undead| {
        move || DefenderState {
            realm,
            undead,
            ..soft(40, 100, 8)
        }
    };
    // -4 on resistance 8 fails six times in ten, -9 always (the die cannot roll under 0).
    let evil = mean_kills(&angel, 6, creature(Some(Realm::Death), false));
    assert!((evil - 3.6).abs() < 0.3, "{evil}");
    let dead = mean_kills(&angel, 6, creature(Some(Realm::Chaos), true));
    assert!((dead - 6.0).abs() < 0.2, "{dead}");
    assert!(mean_kills(&angel, 6, creature(Some(Realm::Life), false)) < 1.0);
    assert!(mean_kills(&angel, 6, creature(None, false)) < 1.0);
}

#[test]
fn poison_makes_each_point_of_strength_resisted_or_hurt() {
    let spider = special(
        SpecialAttacks {
            poison: 6,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let armoured = |immunities| DefenderState {
        immunities,
        defense: 50,
        ..soft(200, 100, 4)
    };
    let mut r = rng(5);
    let n = 2_000;
    let hurt: i32 = (0..n)
        .map(|_| process_attack(&spider, 1, &mut armoured(Immunities::default()), &mut r).points)
        .sum();
    // Six checks at resistance 4: six in ten fail, so 3.6 points a time.
    let mean = f64::from(hurt) / f64::from(n);
    assert!((mean - 3.6).abs() < 0.2, "{mean}");
    let proof = Immunities {
        poison: true,
        ..Immunities::default()
    };
    let none: i32 = (0..200)
        .map(|_| process_attack(&spider, 1, &mut armoured(proof), &mut r).points)
        .sum();
    assert_eq!(none, 0);
    // Magic immunity does not stop poison.
    let warded = Immunities {
        magic: true,
        ..Immunities::default()
    };
    let still: i32 = (0..200)
        .map(|_| process_attack(&spider, 1, &mut armoured(warded), &mut r).points)
        .sum();
    assert!(still > 400, "{still}");
}

#[test]
fn life_steal_hurts_past_armour_and_heals_the_thief() {
    let wraith = special(
        SpecialAttacks {
            life_steal: true,
            penalty: 3,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let target = || DefenderState {
        defense: 50,
        ..soft(200, 100, 4)
    };
    let mut r = rng(8);
    let n = 3_000;
    let (mut dealt, mut healed) = (0, 0);
    for _ in 0..n {
        let d = process_attack(&wraith, 1, &mut target(), &mut r);
        dealt += d.points;
        healed += d.healed;
    }
    // Resistance 1: a die of 2 to 10 fails by 1 to 9, 5 on average: 4.5 a time.
    let mean = f64::from(healed) / f64::from(n);
    assert!((mean - 4.5).abs() < 0.3, "{mean}");
    assert_eq!(dealt, healed, "what it steals it takes from the target");
    // Death immunity protects against it.
    let proof = DefenderState {
        immunities: Immunities {
            death: true,
            ..Immunities::default()
        },
        ..target()
    };
    let d = process_attack(&wraith, 1, &mut proof.clone(), &mut r);
    assert_eq!((d.points, d.healed), (0, 0));
}

#[test]
fn damage_is_sorted_by_what_dealt_it_so_the_dead_know_how_they_fell() {
    let target = || DefenderState {
        defense: 0,
        ..soft(200, 100, 4)
    };
    let mut r = rng(11);
    // Life steal and the blows of a creator of undead drain; destruction destroys.
    let wraith = special(
        SpecialAttacks {
            life_steal: true,
            penalty: 3,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let d = process_attack(&wraith, 3, &mut target(), &mut r);
    assert!(d.points > 0 && d.drained > 0 && d.drained <= d.points);
    let ghoul = Combatant {
        strength: 4,
        to_hit: 3,
        ..special(
            SpecialAttacks {
                create_undead: true,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        )
    };
    let d = process_attack(&ghoul, 4, &mut target(), &mut r);
    assert!(d.points > 0, "{d:?}");
    assert_eq!((d.drained, d.gone), (d.points, 0));
    // Magic immunity keeps the victim from being raised.
    let warded = DefenderState {
        immunities: Immunities {
            magic: true,
            ..Immunities::default()
        },
        ..target()
    };
    let d = process_attack(&ghoul, 4, &mut warded.clone(), &mut r);
    assert_eq!(d.drained, 0);
    let destroyer = special(
        SpecialAttacks {
            destruction: true,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let d = process_attack(
        &destroyer,
        4,
        &mut DefenderState {
            resistance: 0,
            ..target()
        },
        &mut r,
    );
    assert!(d.gone > 0 && d.drained == 0, "{d:?}");
}

#[test]
fn a_melee_exchange_heals_a_life_stealer() {
    let thief = Fighter {
        melee: special(
            SpecialAttacks {
                life_steal: true,
                penalty: 3,
                ..SpecialAttacks::default()
            },
            Mode::Melee,
        ),
        thrown: None,
        first_strike: false,
        negate_first_strike: false,
        state: DefenderState {
            front_damage: 3,
            ..soft(1, 8, 4)
        },
    };
    let victim = Fighter {
        melee: Combatant::default(),
        thrown: None,
        first_strike: false,
        negate_first_strike: false,
        state: soft(50, 100, 4),
    };
    let mut healed_any = false;
    for seed in 0..40 {
        let (mut a, mut b) = (thief.clone(), victim.clone());
        melee_exchange(&mut a, &mut b, &mut rng(seed));
        assert!(a.state.front_damage <= 3);
        healed_any |= a.state.front_damage < 3;
    }
    assert!(healed_any);
}

#[test]
fn a_gaze_makes_each_figure_resist_in_the_thrown_step_only() {
    let gazer = |gaze, mode| {
        special(
            SpecialAttacks {
                gaze,
                penalty: 2,
                ..SpecialAttacks::default()
            },
            mode,
        )
    };
    // Resistance 4 at -2: fails on 3 to 10, eight in ten of ten figures.
    let k = mean_kills(&gazer(Gaze::Stoning, Mode::ThrownOrBreath), 1, || {
        soft(10, 100, 4)
    });
    assert!((k - 8.0).abs() < 0.4, "{k}");
    // The same unit's melee blows do not gaze.
    assert!(mean_kills(&gazer(Gaze::Stoning, Mode::Melee), 1, || soft(10, 100, 4)) < 1.0);
    // A multiple gaze stones and kills: about 1 - 0.2 * 0.2 of the figures.
    let multi = mean_kills(&gazer(Gaze::Multi, Mode::ThrownOrBreath), 1, || {
        soft(10, 100, 4)
    });
    assert!((multi - 9.6).abs() < 0.4, "{multi}");
    let death = mean_kills(&gazer(Gaze::Death, Mode::ThrownOrBreath), 1, || {
        DefenderState {
            immunities: Immunities {
                death: true,
                ..Immunities::default()
            },
            ..soft(10, 100, 4)
        }
    });
    assert!(death < 1.0, "{death}");
}

#[test]
fn healing_mends_the_front_figure_then_brings_the_dead_back() {
    let mut d = DefenderState {
        front_damage: 3,
        figures_max: 4,
        ..def(2, 5, 0)
    };
    d.heal(2);
    assert_eq!((d.figures, d.front_damage), (2, 1));
    // Past the front figure's damage, a dead figure returns, the rest wounding it.
    d.heal(3);
    assert_eq!((d.figures, d.front_damage), (3, 3));
    d.heal(100);
    assert_eq!((d.figures, d.front_damage), (4, 0));
    // The dead cannot be healed, and nothing is healed for nothing.
    let mut gone = DefenderState {
        figures_max: 4,
        ..def(0, 5, 0)
    };
    gone.heal(10);
    assert_eq!(gone.figures, 0);
    d.heal(-4);
    assert_eq!((d.figures, d.front_damage), (4, 0));
}

#[test]
fn invulnerability_blocks_two_more_points_of_each_hit() {
    let mut r = rng(7);
    let (mut plain, mut invulnerable) = (0, 0);
    for _ in 0..2_000 {
        let mut a = def(100, 100, 0);
        let mut b = def(100, 100, 0);
        b.enchantments.push("invulnerability".into());
        plain += process_attack(&combatant(5, 0), 4, &mut a, &mut r).points;
        invulnerable += process_attack(&combatant(5, 0), 4, &mut b, &mut r).points;
    }
    // Four figures hitting about 1.5 each: two off every volley leaves a seventh or so.
    assert!(plain > 5 * invulnerable.max(1), "{plain} vs {invulnerable}");
}

#[test]
fn righteousness_and_bonuses_change_the_defense_a_bolt_meets() {
    let bolt = Combatant {
        nature: AttackNature {
            realm: Some(Realm::Chaos),
            ..AttackNature::default()
        },
        mode: Mode::Ranged,
        ..combatant(5, 0)
    };
    let mut d = def(1, 10, 2);
    assert_eq!(d.defense_versus(&bolt), 2);
    d.enchantments.push("elemental_armor".into());
    assert_eq!(d.defense_versus(&bolt), 12);
    d.enchantments.push("righteousness".into());
    assert_eq!(d.defense_versus(&bolt), 50);
}

#[test]
fn a_sleeper_takes_every_blow_at_full_strength() {
    let mut r = rng(5);
    let mut d = def(1, 100, 100);
    d.enchantments.push("black_sleep".into());
    d.figures = 1;
    d.figures_max = 1;
    let hit = process_attack(&combatant(5, 0), 1, &mut d, &mut r);
    assert_eq!(hit.points, 5);
}

#[test]
fn berserk_leaves_no_defense_whatever_the_bonuses() {
    let bolt = Combatant {
        nature: AttackNature {
            realm: Some(Realm::Chaos),
            ..AttackNature::default()
        },
        mode: Mode::Ranged,
        ..combatant(5, 0)
    };
    let mut d = def(1, 10, 4);
    d.enchantments.push("elemental_armor".into());
    d.enchantments.push("berserk".into());
    assert_eq!(d.defense_versus(&bolt), 0);
}

#[test]
fn eldritch_weapon_takes_a_plus_off_the_defenders_blocks() {
    let mut r = rng(11);
    let (mut plain, mut eldritch) = (0, 0);
    for _ in 0..3_000 {
        let mut a = def(100, 100, 6);
        let mut b = def(100, 100, 6);
        a.to_block = 1;
        b.to_block = 1;
        let weapon = Combatant {
            eldritch: true,
            ..combatant(5, 0)
        };
        plain += process_attack(&combatant(5, 0), 4, &mut a, &mut r).points;
        eldritch += process_attack(&weapon, 4, &mut b, &mut r).points;
    }
    assert!(eldritch > plain + plain / 5, "{eldritch} vs {plain}");
}

#[test]
fn immolation_burns_the_defender_on_top_of_the_blows() {
    let mut r = rng(12);
    let burning = Combatant {
        immolation: true,
        ..combatant(0, 0)
    };
    let mut d = def(4, 10, 0);
    let dealt = process_attack(&burning, 1, &mut d, &mut r);
    // No melee dice, so every point comes from the strength-four fire.
    assert!(dealt.points > 0, "the fire hurt");
    let mut immune = def(4, 10, 0);
    immune.immunities.fire = true;
    assert_eq!(process_attack(&burning, 1, &mut immune, &mut r).points, 0);
    let mut warded = def(4, 10, 0);
    warded.immunities.magic = true;
    assert_eq!(process_attack(&burning, 1, &mut warded, &mut r).points, 0);
}

#[test]
fn fear_keeps_figures_from_striking_back() {
    let fighter = |fear: bool, melee: i32| Fighter {
        melee: Combatant {
            fear,
            ..combatant(melee, 0)
        },
        thrown: None,
        first_strike: false,
        negate_first_strike: false,
        state: def(6, 100, 0),
    };
    let mut r = rng(13);
    let (mut calm, mut frightened) = (0, 0);
    for _ in 0..2_000 {
        let (mut a, mut b) = (fighter(false, 0), fighter(false, 4));
        calm += melee_exchange(&mut a, &mut b, &mut r).taken.points;
        let (mut a, mut b) = (fighter(true, 0), fighter(false, 4));
        frightened += melee_exchange(&mut a, &mut b, &mut r).taken.points;
    }
    assert!(calm > 3 * frightened, "{calm} vs {frightened}");
    // A defender immune to death is not frightened.
    let mut r = rng(14);
    let mut steady = fighter(false, 4);
    steady.state.immunities.death = true;
    let scary = fighter(true, 0);
    let total: i32 = (0..500)
        .map(|_| {
            let mut s = steady.clone();
            let mut f = scary.clone();
            melee_exchange(&mut f, &mut s, &mut r).taken.points
        })
        .sum();
    assert!(total > 0);
}

#[test]
fn a_fearsome_defender_frightens_the_attackers_figures_too() {
    let fighter = |fear: bool, melee: i32| Fighter {
        melee: Combatant {
            fear,
            ..combatant(melee, 0)
        },
        thrown: None,
        first_strike: false,
        negate_first_strike: false,
        state: def(6, 100, 0),
    };
    let mut r = rng(15);
    let (mut calm, mut frightened) = (0, 0);
    for _ in 0..2_000 {
        let (mut a, mut b) = (fighter(false, 4), fighter(false, 0));
        calm += melee_exchange(&mut a, &mut b, &mut r).dealt.points;
        let (mut a, mut b) = (fighter(false, 4), fighter(true, 0));
        frightened += melee_exchange(&mut a, &mut b, &mut r).dealt.points;
    }
    assert!(calm > 3 * frightened, "{calm} vs {frightened}");
}

#[test]
fn overhealing_adds_a_hit_point_to_every_figure_for_each_figures_worth() {
    let mut whole = def(3, 5, 0);
    whole.figures_max = 3;
    let mut plain = whole.clone();
    plain.heal(9);
    assert_eq!(
        (plain.hits, plain.extra_hits),
        (5, 0),
        "ordinary healing stops at whole"
    );
    whole.heal_overflow(7);
    assert_eq!(
        (whole.hits, whole.extra_hits),
        (7, 2),
        "7 over 3 figures is 2 each"
    );
    // A wounded unit heals first: 4 on the front figure, the rest is surplus.
    let mut hurt = def(3, 5, 0);
    hurt.figures_max = 3;
    hurt.front_damage = 4;
    hurt.heal_overflow(6);
    assert_eq!((hurt.front_damage, hurt.hits, hurt.extra_hits), (0, 5, 0));
}

#[test]
fn stoning_and_dispel_evil_leave_nothing_to_heal() {
    let mut r = rng(31);
    let stoner = special(
        SpecialAttacks {
            stoning_touch: true,
            penalty: 9,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let d = process_attack(&stoner, 6, &mut soft(40, 100, 1), &mut r);
    assert!(d.kills > 0);
    assert_eq!(d.gone, d.kills as i32 * 100, "each stoned figure is gone");
    let death = special(
        SpecialAttacks {
            death_touch: true,
            penalty: 9,
            ..SpecialAttacks::default()
        },
        Mode::Melee,
    );
    let d = process_attack(&death, 6, &mut soft(40, 100, 1), &mut r);
    assert!(d.kills > 0);
    assert_eq!(d.gone, 0, "death can be healed back");
}
