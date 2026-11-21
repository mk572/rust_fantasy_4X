//! The battle's own rules, fought on real units of the shipped content on an open field: moving,
//! attacking, turns, fleeing, merging, teleporting, deployment, nodes, gazes, life steal, webs,
//! binding and seizing, vortices, invisibility and the legal commands.
//!
//! Tests: this is the test file; the units come from `f4x_battle::testutil`.

use f4x_battle::battle::{Battle, BattleCommand, BattleError, BattleEvent};
use f4x_battle::battlemagic::caster::BattleCaster;
use f4x_battle::battlemagic::field::Field;
use f4x_battle::map::{Cell, Pos, GRID_H};
use f4x_battle::special::SpecialAttacks;
use f4x_battle::testutil::*;
use f4x_battle::{BattleUnit, Side, Status};
use f4x_def::half_int::HalfInt;
use f4x_def::rules::damage_type::{DamageType, DamageTypes, RangedKind};
use f4x_def::rules::realm::Realm;
use f4x_def::rules::{Ability, SpellId};
use f4x_def::testdir::{shipped, TestResult};

/// The unit of the content `number` as a battle makes it (taken from a battle that has one).
#[test]
fn a_new_battle_starts_with_the_attacker_and_full_moves() -> TestResult {
    let b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    assert_eq!(b.active, Side::Attacker);
    assert_eq!(b.turn, 1);
    assert_eq!(b.winner, None);
    assert_eq!(b.unit(0).ok_or("unit is missing")?.id, 0);
    assert_eq!(b.unit(1).ok_or("unit is missing")?.id, 1);
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.moves_left,
        b.unit(0).ok_or("unit is missing")?.moves
    );
    assert_eq!(b.player_of(Side::Attacker), A);
    assert_eq!(b.player_of(Side::Defender), D);
    assert_eq!(b.active_units(Side::Attacker).count(), 1);
    assert_eq!(b.unit_at((17, 10)).ok_or("unit_at is missing")?.id, 1);
    assert!(b.unit_at((9, 9)).is_none());
    Ok(())
}

#[test]
fn units_walk_within_their_moves_and_not_through_others() -> TestResult {
    let mut b = real_battle(
        &[(107 /* high_men_cavalry */, (3, 10))],
        &[(104 /* high_men_spearmen */, (5, 10))],
        1,
    );
    // Cavalry has two moves: four half-moves, so two squares.
    let reach = b.reachable(0);
    assert_eq!(
        reach.get(&(5, 11)),
        Some(&HalfInt::from_raw(5)),
        "one diagonal step costs half a move more"
    );
    assert!(!reach.contains_key(&(5, 10)), "occupied");
    assert!(!reach.contains_key(&(6, 10)));
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: (5, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(
        &events[0],
        BattleEvent::Moved { unit: 0, path } if path.last() == Some(&(5, 11)) && path.len() == 2
    ));
    assert_eq!(b.unit(0).ok_or("unit is missing")?.cell, (5, 11));
    assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (6, 11)
        }),
        Err(BattleError::NoMovesLeft)
    );
    Ok(())
}

#[test]
fn bad_orders_are_refused() {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 1,
            to: (16, 10)
        }),
        Err(BattleError::NotYours)
    );
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 7,
            to: (4, 10)
        }),
        Err(BattleError::NoSuchUnit)
    );
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (10, 10)
        }),
        Err(BattleError::Unreachable)
    );
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::NotAdjacent)
    );
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 0 }),
        Err(BattleError::BadTarget)
    );
    assert_eq!(
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 }),
        Err(BattleError::NoRangedAttack)
    );
}

#[test]
fn a_melee_attack_hurts_both_sides_and_costs_half_the_allowance() -> TestResult {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (9, 10))],
        &[(104 /* high_men_spearmen */, (10, 10))],
        3,
    );
    let (ours, theirs) = (b.units[0].hit_points(), b.units[1].hit_points());
    let events = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let BattleEvent::Attacked { dealt, taken, .. } = events[0] else {
        return Err(format!("{events:?}").into());
    };
    assert_eq!(
        b.unit(1).ok_or("unit is missing")?.hit_points(),
        theirs - dealt
    );
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.hit_points(),
        ours - taken
    );
    // The blow costs half the allowance; what is left may strike again, and with none left
    // there is no attack (ReMoM `Battle_Unit_Attack`, wiki "Melee Attack").
    let me = b.unit(0).ok_or("unit is missing")?;
    assert!(!me.attacked);
    assert_eq!(me.moves_left, me.moves - me.moves.half());
    while b.unit(0).ok_or("unit is missing")?.moves_left > 0 {
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        if !b.unit(1).ok_or("unit is missing")?.is_active() {
            return Ok(());
        }
    }
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::NoMovesLeft)
    );
    Ok(())
}

#[test]
fn killing_the_last_enemy_wins_and_ends_the_battle() -> TestResult {
    // A golem against settlers, who cannot strike back.
    let mut b = real_battle(
        &[(81 /* dwarven_golem */, (9, 10))],
        &[(111 /* high_men_settlers */, (10, 10))],
        5,
    );
    let mut ended = None;
    for _ in 0..30 {
        let r = b.apply(BattleCommand::Attack { unit: 0, target: 1 });
        match r {
            Ok(events) => {
                if let Some(BattleEvent::Ended { winner }) = events.last() {
                    ended = Some(*winner);
                    break;
                }
                b.apply(BattleCommand::EndTurn)
                    .map_err(|e| format!("{e:?}"))?;
                b.apply(BattleCommand::EndTurn)
                    .map_err(|e| format!("{e:?}"))?;
            }
            Err(e) => return Err(format!("{e:?}").into()),
        }
    }
    assert_eq!(ended, Some(Side::Attacker));
    assert_eq!(b.winner, Some(Side::Attacker));
    assert!(!b.unit(1).ok_or("unit is missing")?.is_active());
    assert_eq!(b.apply(BattleCommand::EndTurn), Err(BattleError::Over));
    Ok(())
}

#[test]
fn turns_alternate_and_moves_come_back() -> TestResult {
    let mut b = real_battle(
        &[(107 /* high_men_cavalry */, (3, 10))],
        &[(107 /* high_men_cavalry */, (17, 10))],
        1,
    );
    b.apply(BattleCommand::Move {
        unit: 0,
        to: (5, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    let e = b
        .apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        e,
        vec![BattleEvent::TurnStarted {
            side: Side::Defender,
            turn: 1
        }]
    );
    assert_eq!(b.active, Side::Defender);
    b.apply(BattleCommand::Move {
        unit: 1,
        to: (15, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    let e = b
        .apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        e,
        vec![BattleEvent::TurnStarted {
            side: Side::Attacker,
            turn: 2
        }]
    );
    assert_eq!(b.turn, 2);
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.moves_left,
        b.unit(0).ok_or("unit is missing")?.moves,
        "refreshed"
    );
    Ok(())
}

#[test]
fn fleeing_hands_the_battle_to_the_other_side() -> TestResult {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    let e = b.apply(BattleCommand::Flee).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        e[0],
        BattleEvent::Fled {
            side: Side::Attacker
        }
    );
    assert_eq!(
        e.last(),
        Some(&BattleEvent::Ended {
            winner: Side::Defender
        })
    );
    assert_eq!(b.unit(0).ok_or("unit is missing")?.status, Status::Fled);
    Ok(())
}

/// 1.50 note 747: a ranged attack uses up all the unit's movement, as a melee attack does.
#[test]
fn a_ranged_attack_uses_up_the_rest_of_the_movement() -> TestResult {
    let mut b = real_battle(
        &[(101 /* high_elf_longbowmen */, (3, 10))],
        &[(151 /* troll_settlers */, (8, 10))],
        1,
    );
    assert!(b.unit(0).ok_or("no unit")?.moves_left > 0);
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(0).ok_or("no unit")?.moves_left, 0);
    Ok(())
}

#[test]
fn a_battle_replays_exactly_from_the_same_seed() {
    let start = real_battle(
        &[(104 /* high_men_spearmen */, (9, 10))],
        &[(104 /* high_men_spearmen */, (10, 10))],
        0,
    );
    let run = |seed| {
        let mut b = reseeded(&start, seed);
        let mut log = Vec::new();
        for _ in 0..10 {
            for cmd in [
                BattleCommand::Attack { unit: 0, target: 1 },
                BattleCommand::EndTurn,
                BattleCommand::Attack { unit: 1, target: 0 },
                BattleCommand::EndTurn,
            ] {
                if let Ok(e) = b.apply(cmd) {
                    log.extend(e)
                }
            }
            if b.winner.is_some() {
                break;
            }
        }
        log
    };
    assert_eq!(run(11), run(11));
    assert_ne!(run(11), run(12));
}

#[test]
fn obstacles_shape_where_units_can_go() {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (9, 10))],
        &[(104 /* high_men_spearmen */, (12, 10))],
        1,
    );
    for y in 0..GRID_H {
        b.map.set((10, y), Cell::Blocked);
    }
    assert!(
        !b.reachable(0).contains_key(&(11, 10)),
        "the wall of rock is in the way"
    );
    assert!(b
        .apply(BattleCommand::Move {
            unit: 0,
            to: (11, 10)
        })
        .is_err());
}

#[test]
fn merging_needs_the_ability_and_half_a_move() -> TestResult {
    let mut b = merging_battle();
    assert_eq!(
        b.apply(BattleCommand::Merge { unit: 1 }),
        Err(BattleError::CannotMerge),
        "no Merging"
    );
    b.apply(BattleCommand::Done { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        b.apply(BattleCommand::Merge { unit: 0 }),
        Err(BattleError::CannotMerge),
        "no move left"
    );
    // Half a move is enough.
    let mut b = merging_battle();
    b.units[0].set_moves_left(HalfInt::from_raw(1));
    assert!(b.units[0].moves_left > 0);
    b.apply(BattleCommand::Merge { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn a_side_with_only_merged_units_loses_and_they_get_away_whole() -> TestResult {
    let mut b = real_battle(
        &[(189 /* great_wyrm */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    b.apply(BattleCommand::Merge { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        b.winner,
        Some(Side::Defender),
        "nothing of the side is on the field"
    );
    let u = b.unit(0).ok_or("no unit")?;
    assert_eq!(u.status, Status::Fled);
    assert!(u.slipped, "flight without the risk");
    Ok(())
}

#[test]
fn merged_units_of_the_winner_come_up_when_the_battle_is_won() -> TestResult {
    // The wyrm goes under; the hammerhands beside the lone spearman win the battle.
    let mut b = real_battle(
        &[
            (189 /* great_wyrm */, (3, 10)),
            (79 /* dwarven_hammerhands */, (3, 12)),
        ],
        &[(104 /* high_men_spearmen */, (4, 12))],
        1,
    );
    b.apply(BattleCommand::Merge { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    for _ in 0..30 {
        if b.winner.is_some() {
            break;
        }
        b.apply(BattleCommand::Attack { unit: 1, target: 2 })
            .map_err(|e| format!("{e:?}"))?;
        for _ in 0..2 {
            if b.winner.is_none() {
                b.apply(BattleCommand::EndTurn)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
    }
    assert_eq!(b.winner, Some(Side::Attacker));
    assert_eq!(b.unit(0).ok_or("no unit")?.status, Status::Active);
    Ok(())
}

#[test]
fn teleporting_units_go_anywhere_free_for_one_move() -> TestResult {
    let mut b = real_battle(
        &[(175 /* unicorns */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    b.map.set((10, 10), Cell::Blocked);
    let reach = b.reachable(0);
    assert_eq!(
        reach.get(&(15, 3)),
        Some(&HalfInt::from_int(1)),
        "far away, one move"
    );
    assert!(!reach.contains_key(&(10, 10)), "not into rock");
    assert!(!reach.contains_key(&(17, 10)), "not onto a unit");
    assert!(!reach.contains_key(&(3, 10)), "not where it stands");
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: (16, 10),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(&events[0], BattleEvent::Moved { path, .. } if path == &vec![(16, 10)]));
    assert_eq!(b.unit(0).ok_or("unit is missing")?.cell, (16, 10));
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.moves_left,
        HalfInt::from_int(1),
        "one of two moves used"
    );
    // It may teleport again with what is left, and then has nothing left.
    b.apply(BattleCommand::Move {
        unit: 0,
        to: (16, 5),
    })
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (3, 3)
        }),
        Err(BattleError::NoMovesLeft)
    );
    Ok(())
}

#[test]
fn a_teleporter_cannot_go_into_rock_and_a_walker_cannot_teleport() {
    let mut b = real_battle(
        &[(175 /* unicorns */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    b.map.set((10, 10), Cell::Blocked);
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (10, 10)
        }),
        Err(BattleError::Unreachable)
    );
    let mut w = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    assert_eq!(
        w.apply(BattleCommand::Move {
            unit: 0,
            to: (15, 3)
        }),
        Err(BattleError::Unreachable)
    );
}

#[test]
fn units_deploy_melee_first_and_shooters_behind_them() {
    use f4x_battle::map::deployment_order;
    // Spearmen and bowmen, alternately.
    let all = real_battle(
        &[
            (104 /* high_men_spearmen */, (3, 10)),
            (106 /* high_men_bowmen */, (3, 11)),
        ],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    let (melee, shooter) = (
        unit_of(&all, 104 /* high_men_spearmen */),
        unit_of(&all, 106),
    );
    let units: Vec<BattleUnit> = (0..8)
        .map(|i| {
            if i % 2 == 0 {
                shooter.clone()
            } else {
                melee.clone()
            }
        })
        .collect();
    assert_eq!(deployment_order(&units), vec![1, 3, 5, 7, 0, 2, 4, 6]);
    // Six shooters and one melee unit: the two strongest in melee move up front. The shooters
    // are bowmen of growing strength in melee: 1, 2, 3, 4, 8 and 10.
    let shooters = [
        106, // high_men_bowmen
        49,  // beastmen_bowmen
        86,  // gnoll_bowmen
        123, // lizardman_javelineers
        65,  // dark_elf_nightmares
        126, // lizardman_dragon_turtle
    ];
    let mut army: Vec<(u16, Pos)> = vec![(104 /* high_men_spearmen */, (3, 9))];
    army.extend(
        shooters
            .iter()
            .enumerate()
            .map(|(i, &n)| (n, (3, 10 + i as i32))),
    );
    let all = real_battle(&army, &[(104 /* high_men_spearmen */, (17, 10))], 1);
    let mut units = vec![unit_of(&all, 104 /* high_men_spearmen */)];
    units.extend(shooters.iter().map(|&n| unit_of(&all, n)));
    let melee: Vec<i32> = units.iter().map(|u| u.melee).collect();
    assert!(melee[1..].windows(2).all(|w| w[0] < w[1]), "{melee:?}");
    assert_eq!(deployment_order(&units), vec![0, 6, 5, 1, 2, 3, 4]);
}

#[test]
fn a_nodes_aura_helps_its_realms_creatures_on_both_sides() {
    // A chaos creature with a ranged attack against a nature creature.
    let mut b = real_battle(
        &[(159 /* chaos_spawn */, (3, 10))],
        &[(180 /* sprites */, (17, 10))],
        4,
    );
    let (melee, ranged, defense, resist) = (
        b.units[0].melee,
        b.units[0].ranged,
        b.units[0].defense,
        b.units[0].resistance,
    );
    let theirs = (b.units[1].melee, b.units[1].defense);
    b.node_aura = Some(Realm::Chaos);
    b.refresh_fields();
    assert_eq!(b.units[0].melee, melee + rules().node_aura_bonus);
    assert_eq!(b.units[0].ranged, ranged + rules().node_aura_bonus);
    assert_eq!(b.units[0].defense, defense + rules().node_aura_bonus);
    assert_eq!(b.units[0].resistance, resist + rules().node_aura_bonus);
    assert_eq!((b.units[1].melee, b.units[1].defense), theirs);
    // Gone with the aura.
    b.node_aura = None;
    b.refresh_fields();
    assert_eq!(b.units[0].melee, melee);
}

#[test]
fn a_node_stops_spells_of_other_realms_but_not_its_own() {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        4,
    );
    assert!(
        !b.node_stops(Some(Realm::Life), 1000),
        "no node, no counter"
    );
    b.node_site = Some(Realm::Sorcery);
    assert!(!b.node_stops(Some(Realm::Sorcery), 1000));
    let stopped = (0..100)
        .filter(|_| b.node_stops(Some(Realm::Life), 50))
        .count();
    assert!(
        (30..=70).contains(&stopped),
        "strength 50 against 50: {stopped}"
    );
    assert!(b.node_stops(Some(Realm::Life), 0));
}

#[test]
fn the_log_names_the_immunity_that_stopped_a_shot() {
    // Longbowmen against a necromancer: the arrow is stopped by missile immunity (a missile).
    let proof = real_battle(
        &[(101 /* high_elf_longbowmen */, (3, 10))],
        &[(32 /* necromancer */, (5, 10))],
        1,
    );
    let mut b = reseeded(&proof, 1);
    let events = b
        .apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .expect("apply succeeds");
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Unharmed {
                attacker: 0,
                defender: 1,
                by: DamageType::Missile
            }
        )),
        "{events:?}"
    );
    // Without the immunity nothing is reported as stopped.
    let open = without(&proof, 1, 44 /* missile_immunity */);
    let events = reseeded(&open, 1)
        .apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .expect("apply succeeds");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::Unharmed { .. })),
        "{events:?}"
    );
}

#[test]
fn a_defender_with_a_gaze_stones_the_attackers_before_they_strike() -> TestResult {
    // Spearmen attack a basilisk.
    let start = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(182 /* basilisk */, (4, 10))],
        1,
    );
    let figures = start.units[0].figures;
    let plain = without(&start, 1, 64 /* stoning_gaze */);
    let (mut lost, mut gazeless_lost) = (0, 0);
    for seed in 0..100 {
        for (battle, total) in [(&start, &mut lost), (&plain, &mut gazeless_lost)] {
            let mut b = reseeded(battle, seed);
            b.apply(BattleCommand::Attack { unit: 0, target: 1 })
                .map_err(|e| format!("{e:?}"))?;
            *total += figures - b.unit(0).ok_or("unit is missing")?.figures;
        }
    }
    // The basilisk's blows kill figures too; the gaze adds to them.
    assert!(lost * 2 > gazeless_lost * 3, "{lost} vs {gazeless_lost}");
    Ok(())
}

#[test]
fn a_gaze_works_when_the_attack_strength_is_down_to_zero() -> TestResult {
    // A basilisk under Weakness or Mind Storm has no ranged strength left, but its gaze does
    // not need any (1.50 note 153).
    let mut start = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(182 /* basilisk */, (4, 10))],
        1,
    );
    start.units[1].force(f4x_def::rules::stats::Stat::Ranged, 0);
    let mut plain = without(&start, 1, 64 /* stoning_gaze */);
    plain.units[1].force(f4x_def::rules::stats::Stat::Ranged, 0);
    let figures = start.units[0].figures;
    let (mut lost, mut gazeless_lost) = (0, 0);
    for seed in 0..100 {
        for (battle, total) in [(&start, &mut lost), (&plain, &mut gazeless_lost)] {
            let mut b = reseeded(battle, seed);
            b.apply(BattleCommand::Attack { unit: 0, target: 1 })
                .map_err(|e| format!("{e:?}"))?;
            *total += figures - b.unit(0).ok_or("unit is missing")?.figures;
        }
    }
    assert!(lost * 2 > gazeless_lost * 3, "{lost} vs {gazeless_lost}");
    Ok(())
}

#[test]
fn a_life_stealing_shot_heals_the_shooter() -> TestResult {
    // A demon lord, hurt, shoots a spearman.
    let mut start = real_battle(
        &[(173 /* demon_lord */, (3, 10))],
        &[(104 /* high_men_spearmen */, (5, 10))],
        1,
    );
    start.units[0].set_front_damage(8);
    let mut healed = 0;
    for seed in 0..50 {
        let mut b = reseeded(&start, seed);
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        healed += 8 - b.unit(0).ok_or("unit is missing")?.front_damage;
    }
    assert!(healed > 0);
    Ok(())
}

#[test]
fn a_spell_save_makes_the_resistance_checks_of_the_cast_harder() {
    // War bears have a resistance of 6.
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (1, 1))],
        &[(179 /* war_bears */, (5, 5))],
        3,
    );
    let resisted = |b: &mut Battle| (0..200).filter(|_| !b.resist_fails(1, None, 0)).count();
    assert!(resisted(&mut b) > 50);
    b.save_penalty = 10;
    assert_eq!(resisted(&mut b), 0);
}

#[test]
fn the_legal_commands_are_exactly_what_apply_accepts() -> TestResult {
    // An open field: a swordsman next to an enemy, two archers, a unit with no ammunition.
    let longbowmen = 101;
    let mut b = real_battle(
        &[
            (105 /* high_men_swordsmen */, (3, 10)),
            (longbowmen, (3, 12)),
            (longbowmen, (4, 14)),
        ],
        &[
            (104 /* high_men_spearmen */, (4, 10)),
            (longbowmen, (9, 12)),
        ],
        1,
    );
    b.units[2].set_ammo(0);
    check_legal_list(&b);
    // After the attacker's turn, the defender's.
    b.apply(BattleCommand::Done { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    check_legal_list(&b);
    Ok(())
}

#[test]
fn a_spell_caster_shoots_with_its_mana_and_mana_leak_costs_a_shot() -> TestResult {
    // An efreet casts fireballs: twenty mana, three a shot.
    let mut b = real_battle(
        &[(162 /* efreet */, (3, 10))],
        &[(104 /* high_men_spearmen */, (4, 10))],
        1,
    );
    let (mana, ammo) = (b.units[0].mana, b.units[0].ammo);
    assert!(mana >= 20 && ammo > 2);
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let u = b.unit(0).ok_or("unit is missing")?;
    let (shot, leak) = (rules().shot_mana, rules().mana_leak_drain);
    assert_eq!(
        (u.mana, u.ammo),
        (mana - shot, ammo - 1),
        "a shot costs mana"
    );
    // Mana leak takes mana, and the shots follow it.
    b.units[0].leak_mana(leak, &shipped().rules);
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!(u.mana, mana - shot - leak);
    assert_eq!(u.ammo, (ammo - 1).min((mana - shot - leak) / shot));
    // With no mana left it cannot shoot.
    b.units[0].leak_mana(1000, &shipped().rules);
    assert_eq!(
        (
            b.unit(0).ok_or("unit is missing")?.mana,
            b.unit(0).ok_or("unit is missing")?.ammo
        ),
        (0, 0)
    );
    // A unit with spell shots but no mana of its own (the magicians) simply loses one.
    let mut plain = unit_numbered(51 /* beastmen_magicians */);
    let shots = plain.ammo;
    assert_eq!(plain.mana, 0);
    plain.leak_mana(leak, &shipped().rules);
    assert_eq!((plain.mana, plain.ammo), (0, shots - 1));
    plain.put_ranged_kind(RangedKind::Missile);
    plain.leak_mana(leak, &shipped().rules);
    assert_eq!(plain.ammo, shots - 1, "arrows are not spell shots");
    Ok(())
}

#[test]
fn healing_cannot_bring_back_what_was_destroyed() {
    // Four troll figures.
    let mut u = unit_numbered(147 /* troll_spearmen */);
    let (figures, h) = (u.figures_start, u.hits);
    assert_eq!(u.figures, figures);
    // One figure lost to destruction, one to ordinary blows.
    u.set_figures(figures - 2);
    u.record(f4x_battle::attack::Damage {
        points: 2 * h,
        kills: 2,
        gone: h,
        ..Default::default()
    });
    assert_eq!(u.healable(), h);
    assert_eq!(
        u.heal_points(20 * h, false),
        h,
        "only the ordinary damage comes back"
    );
    assert_eq!(u.figures, figures - 1);
    assert_eq!(u.healable(), 0);
    assert_eq!(u.damage_kinds, [0, 0, h]);
    // Life steal can overheal, but not past the figures it has: the surplus becomes extra hits.
    let gained = u.heal_points(h + figures as i32, true);
    // One figure comes back, the rest is one more hit point for each of the figures.
    assert_eq!(
        (gained, u.figures, u.extra_hits()),
        (h + figures as i32, figures, 1)
    );
    assert_eq!(u.healable(), 0, "the destroyed damage is still there");
}

#[test]
fn extra_hits_do_not_count_as_the_units_real_strength() {
    // War bears: two figures.
    let mut u = unit_numbered(179 /* war_bears */);
    let max = u.max_hit_points();
    assert_eq!(u.figures_start, 2);
    u.heal_points(4, true);
    assert_eq!(u.extra_hits(), 2);
    assert_eq!(u.hit_points(), max + 4);
    assert_eq!(u.max_hit_points(), max, "the real maximum stays");
}

#[test]
fn a_vortex_crushes_the_unit_it_lands_on_and_is_steered_once_a_turn_by_its_owner() -> TestResult {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (11, 10))],
        5,
    );
    let before = b.unit(1).ok_or("unit is missing")?.hit_points();
    b.make_vortex(Side::Attacker, (10, 10));
    assert!(b.is_occupied((10, 10)));
    assert!(!b.can_make_vortex((10, 10)), "not on another vortex");
    assert!(!b.can_make_vortex((11, 10)), "not on a unit");
    // Only a square next to it, and only its owner.
    assert_eq!(
        b.apply(BattleCommand::MoveVortex {
            index: 0,
            to: (13, 10)
        }),
        Err(BattleError::Unreachable)
    );
    let events = b
        .apply(BattleCommand::MoveVortex {
            index: 0,
            to: (11, 10),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, BattleEvent::VortexMoved { to: (11, 10), .. })));
    let after = b.unit(1).map_or(0, |u| u.hit_points());
    assert!(
        after < before || !b.unit(1).ok_or("unit is missing")?.is_active(),
        "{before} -> {after}"
    );
    // Once a turn.
    assert_eq!(
        b.apply(BattleCommand::MoveVortex {
            index: 0,
            to: (11, 11)
        }),
        Err(BattleError::NotYours)
    );
    Ok(())
}

/// Task 711: the Magic Vortex does nothing to a magic-immune unit it lands on or passes.
#[test]
fn a_vortex_does_nothing_to_a_magic_immune_unit() -> TestResult {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(113 /* high_men_paladins */, (11, 10))],
        5,
    );
    let before = b.unit(1).ok_or("unit is missing")?.hit_points();
    b.make_vortex(Side::Attacker, (10, 10));
    b.apply(BattleCommand::MoveVortex {
        index: 0,
        to: (11, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    let after = b.unit(1).ok_or("unit is missing")?.hit_points();
    assert_eq!(after, before, "magic immunity turns the vortex away");
    Ok(())
}

#[test]
fn an_invisible_unit_shows_after_it_attacks_until_its_next_turn_and_to_those_who_see_through(
) -> TestResult {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(167 /* night_stalker */, (5, 10))],
        1,
    );
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    // The night stalker walks up to the spearman and strikes it.
    b.apply(BattleCommand::Move {
        unit: 1,
        to: (4, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    b.apply(BattleCommand::Attack { unit: 1, target: 0 })
        .map_err(|e| format!("{e:?}"))?;
    assert!(b.units[1].revealed);
    b.units[0].cell = (3, 3); // out of reach of its square, so only the strike shows it
    assert!(b.seen_by(Side::Attacker, 1), "seen after attacking");
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert!(b.seen_by(Side::Attacker, 1), "still seen on the enemy turn");
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert!(
        !b.units[1].revealed,
        "hidden again when its own turn begins"
    );
    assert!(!b.seen_by(Side::Attacker, 1));
    // Skeletons are immune to illusions: they see it from the start.
    let b = real_battle(
        &[(165 /* skeletons */, (3, 10))],
        &[(167 /* night_stalker */, (17, 10))],
        1,
    );
    assert!(
        b.seen_by(Side::Attacker, 1),
        "a unit immune to illusions sees it"
    );
    Ok(())
}

#[test]
fn invisibility_that_begins_in_battle_hides_the_unit_at_once() {
    let mut b = real_battle(
        &[(104 /* high_men_spearmen */, (3, 10))],
        &[(104 /* high_men_spearmen */, (17, 10))],
        1,
    );
    assert!(b.seen_by(Side::Attacker, 1));
    b.units[1].revealed = true;
    b.raise_field(
        Side::Defender,
        Field::MassInvisibility,
        SpellId::new(74 /* mass_invisibility */),
    );
    b.refresh_fields();
    assert!(!b.units[1].revealed);
    assert!(!b.seen_by(Side::Attacker, 1));
}

/// Patch 1.50, note 177: a unit that resists Call Chaos's Disintegrate is not destroyed. The spell
/// is Disintegrate's own effect (the whole unit goes when its resistance is under 10), and a Great
/// Drake (resistance 12) shrugs it off while spearmen do not.
#[test]
fn disintegrate_as_call_chaos_throws_it_spares_the_unit_that_resists() -> TestResult {
    let mut b = real_battle(
        &[(164, (0, 0))],
        &[(104 /* high_men_spearmen */, (20, 20))],
        1,
    );
    let drake = b
        .units
        .iter()
        .position(|u| u.side == Side::Attacker)
        .ok_or("no drake")?;
    let spearmen = b
        .units
        .iter()
        .position(|u| u.side == Side::Defender)
        .ok_or("no spearmen")?;
    let spared = b
        .spell_slay(drake, Realm::Chaos, 0, &[], Some(10))
        .ok_or("immune")?;
    assert_eq!(spared.points, 0, "the drake resists");
    assert!(b.units[drake].is_active());
    let slain = b
        .spell_slay(spearmen, Realm::Chaos, 0, &[], Some(10))
        .ok_or("immune")?;
    assert!(slain.points > 0 && !b.units[spearmen].is_active());
    Ok(())
}

/// 1.50 note 720: a reserve that holds exactly what a spell costs, halves included (x0.5, x1.5,
/// x2.5 make them), casts it, and one half point less does not.
#[test]
fn a_reserve_that_exactly_pays_a_spell_casts_it_at_any_distance() {
    let half = HalfInt::from_raw(1);
    for range in [5, 10, 15, 20, 25, 30] {
        for cost in 1..=20 {
            let paying = BattleCaster::new(100, 0, range).mana_for(cost);
            assert_eq!(
                paying.raw(),
                cost * range / 5,
                "x{range}: {cost} costs exactly"
            );
            let caster = BattleCaster::with_mana(100, paying, range);
            assert!(
                caster.affordable() >= cost,
                "x{range}: {paying} pays {cost}"
            );
            let short = BattleCaster::with_mana(100, paying - half, range);
            assert!(
                short.affordable() < cost,
                "x{range}: less than {paying} pays {cost}"
            );
        }
    }
}

#[test]
fn flame_blade_and_the_flaming_power_add_fire_dice() {
    use f4x_def::rules::effects::Effect;
    use f4x_def::rules::stats::{ModOp, Stat};
    let rules = &shipped().rules;
    // Flame Blade: the dice it adds to the blow and to the shot are fire.
    let spell = rules
        .spells
        .iter()
        .find(|(_, s)| s.id == "flame_blade")
        .map(|(_, s)| s)
        .expect("the Flame Blade spell");
    let fire = DamageTypes::of(&[DamageType::Fire]);
    let added = |stat| {
        spell
            .effects
            .iter()
            .filter_map(|e| match &e.effect {
                Effect::Modify {
                    stat: s,
                    op: ModOp::Add,
                    value,
                    types,
                } if *s == stat => Some((value.floor(), *types)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for stat in [Stat::Melee, Stat::Ranged] {
        let dice = added(stat);
        assert!(!dice.is_empty() && dice.iter().all(|(n, t)| *n > 0 && *t == fire));
    }
    // Flaming: its whole bonus is fire.
    let flaming = rules
        .item_powers
        .iter()
        .find(|(_, p)| p.id == "flaming")
        .map(|(_, p)| p)
        .expect("the Flaming power");
    assert_eq!(flaming.bonus_types, vec![DamageType::Fire]);
    assert!(flaming.melee_bonus > 0);
}

/// A Vampiric weapon (life steal given by an item) costs the target no more resistance than the
/// unit's own level does, as in the original and on the wiki.
#[test]
fn an_item_gives_life_steal_no_extra_penalty() {
    let life_steal = Ability::new(35 /* life_steal */);
    let level = 3;
    let innate = SpecialAttacks::of_unit(&[life_steal], &[], level);
    let given = SpecialAttacks::of_unit(&[life_steal], &[life_steal], level);
    assert_eq!(given.penalty, innate.penalty, "the same penalty");
    assert_eq!(given.penalty, level, "the unit's own level, nothing more");
}
