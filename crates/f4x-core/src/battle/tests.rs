use super::game::BattleStartError;
use super::map::{distance, Cell, GRID_H, GRID_W};
use super::spell::SpellSource;
use super::*;
use crate::game::save::Snapshot;
use crate::game::testutil::{num, shipped};
use crate::game::{Command, Event, Game, Rejection};
use crate::map::{Grid, Map, Plane};
use crate::player::PlayerId;
use crate::spell::target_error::TargetError;
use crate::testdir::TestResult;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

const A: PlayerId = PlayerId(0);
const D: PlayerId = PlayerId(1);

fn spearman(side: Side, cell: Pos) -> BattleUnit {
    BattleUnit::simple(side, "spearman", 3, 2, 3, 2).at(cell)
}

fn duel(att: BattleUnit, def: BattleUnit, seed: u64) -> Battle {
    Battle::new(A, D, BattleMap::open(), vec![att, def], seed)
}

#[test]
fn a_new_battle_starts_with_the_attacker_and_full_moves() -> TestResult {
    let b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (5, 10)),
        1,
    );
    // Two moves: four half-moves, so two squares.
    let reach = b.reachable(0);
    assert_eq!(reach.get(&(5, 11)), Some(&4));
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
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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
fn a_melee_attack_hurts_both_sides_and_uses_up_the_turn() -> TestResult {
    let mut b = duel(
        spearman(Side::Attacker, (9, 10)),
        spearman(Side::Defender, (10, 10)),
        3,
    );
    let events = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let BattleEvent::Attacked { dealt, taken, .. } = events[0] else {
        return Err(format!("{events:?}").into());
    };
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), 6 - dealt);
    assert_eq!(b.unit(0).ok_or("unit is missing")?.hit_points(), 6 - taken);
    assert!(b.unit(0).ok_or("unit is missing")?.attacked);
    assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::AlreadyAttacked)
    );
    Ok(())
}

#[test]
fn killing_the_last_enemy_wins_and_ends_the_battle() -> TestResult {
    let mut weak = BattleUnit::simple(Side::Defender, "peasant", 1, 1, 0, 0).at((10, 10));
    weak.melee = 0;
    let strong = BattleUnit::simple(Side::Attacker, "giant", 2, 20, 10, 5).at((9, 10));
    let mut b = duel(strong, weak, 5);
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
            Err(e) => panic!("{e:?}"),
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
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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

#[test]
fn archers_shoot_with_ammunition_and_range_costs_accuracy() -> TestResult {
    let archer = |cell| {
        let mut u = BattleUnit::simple(Side::Attacker, "archer", 4, 1, 1, 0).at(cell);
        u.ranged = 4;
        u.ranged_kind = RangedKind::Missile;
        u.ammo = 2;
        u
    };
    let target = |cell| BattleUnit::simple(Side::Defender, "target", 40, 1, 0, 0).at(cell);
    // Average damage at point blank against far away over many seeds.
    let mut near_total = 0;
    let mut far_total = 0;
    for seed in 0..200 {
        let mut near = Battle::new(
            A,
            D,
            BattleMap::open(),
            vec![archer((3, 10)), target((4, 10))],
            seed,
        );
        let mut far = Battle::new(
            A,
            D,
            BattleMap::open(),
            vec![archer((3, 10)), target((12, 10))],
            seed,
        );
        let BattleEvent::Shot { dealt: n, .. } = near
            .apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?[0]
        else {
            panic!()
        };
        let BattleEvent::Shot { dealt: f, .. } = far
            .apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?[0]
        else {
            panic!()
        };
        near_total += n;
        far_total += f;
    }
    assert!(near_total > far_total, "{near_total} vs {far_total}");
    // Ammunition runs out.
    let mut b = Battle::new(
        A,
        D,
        BattleMap::open(),
        vec![archer((3, 10)), target((8, 10))],
        1,
    );
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(0).ok_or("unit is missing")?.ammo, 1);
    assert_eq!(
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 }),
        Err(BattleError::AlreadyAttacked)
    );
    b.units[0].ammo = 0;
    b.units[0].attacked = false;
    assert_eq!(
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 }),
        Err(BattleError::OutOfAmmo)
    );
    Ok(())
}

#[test]
fn a_walkers_melee_against_a_flier_follows_the_setting_and_the_flier_hits_back() -> TestResult {
    use crate::game::setting::MeleeVsFliers;
    let walker = BattleUnit::simple(Side::Attacker, "high_men_cavalry", 4, 2, 6, 1).at((9, 10));
    let mut flier = BattleUnit::simple(Side::Defender, "bat", 4, 2, 6, 1).at((10, 10));
    flier.flies = true;
    let mut b = duel(walker, flier, 8);
    let full = b.fighter(0, false).melee.strength;
    assert!(full > 1);
    for (rule, strength) in [
        (MeleeVsFliers::Zero, 0),
        (MeleeVsFliers::Half, (full + 1) / 2),
        (MeleeVsFliers::Full, full),
    ] {
        b.melee_vs_fliers = rule;
        assert_eq!(b.fighter(0, true).melee.strength, strength, "{rule:?}");
    }
    // The original's rule: the walker deals nothing, and the flier answers at full strength.
    b.melee_vs_fliers = MeleeVsFliers::Zero;
    let e = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let BattleEvent::Attacked { dealt, .. } = e[0] else {
        panic!()
    };
    assert_eq!(dealt, 0);
    Ok(())
}

#[test]
fn a_battle_replays_exactly_from_the_same_seed() {
    let run = |seed| {
        let mut b = duel(
            spearman(Side::Attacker, (9, 10)),
            spearman(Side::Defender, (10, 10)),
            seed,
        );
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
fn a_stalemate_ends_at_the_turn_limit_in_the_defenders_favour() -> TestResult {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        1,
    );
    let mut last = Vec::new();
    for _ in 0..(2 * TURN_LIMIT + 2) {
        match b.apply(BattleCommand::EndTurn) {
            Ok(e) => last = e,
            Err(_) => break,
        }
    }
    assert_eq!(b.winner, Some(Side::Defender));
    assert!(last.iter().any(|e| matches!(
        e,
        BattleEvent::Ended {
            winner: Side::Defender
        }
    )));
    // The scroll says both armies withdrew exhausted, and the event comes before the end.
    let at = |f: fn(&BattleEvent) -> bool| last.iter().position(f);
    let exhausted = at(|e| matches!(e, BattleEvent::Exhausted)).expect("exhausted event");
    assert!(exhausted < at(|e| matches!(e, BattleEvent::Ended { .. })).ok_or("at is missing")?);
    Ok(())
}

/// Both sides walk at the nearest enemy and attack: every such battle ends.
fn play_out(seed: u64, a: Vec<BattleUnit>, d: Vec<BattleUnit>) -> Battle {
    let mut units = a;
    units.extend(d);
    let mut b = Battle::new(A, D, BattleMap::open(), units, seed);
    while b.winner.is_none() {
        let side = b.active;
        let ids: Vec<usize> = b.active_units(side).map(|u| u.id).collect();
        for id in ids {
            let me = b.unit(id).expect("unit succeeds").clone();
            let Some(enemy) = b
                .active_units(side.other())
                .min_by_key(|e| distance(me.cell, e.cell))
                .map(|e| e.id)
            else {
                break;
            };
            if distance(me.cell, b.unit(enemy).expect("unit succeeds").cell) > 1 {
                // Step to the reachable square nearest the enemy.
                let target = b.unit(enemy).expect("unit succeeds").cell;
                if let Some((&to, _)) = b
                    .reachable(id)
                    .iter()
                    .min_by_key(|(&p, _)| distance(p, target))
                {
                    let _ = b.apply(BattleCommand::Move { unit: id, to });
                }
            }
            let enemy_cell = b.unit(enemy).map(|e| e.cell);
            if let Some(cell) = enemy_cell {
                if b.unit(enemy).expect("unit succeeds").is_active()
                    && distance(b.unit(id).expect("unit succeeds").cell, cell) == 1
                {
                    let _ = b.apply(BattleCommand::Attack {
                        unit: id,
                        target: enemy,
                    });
                }
            }
            if b.winner.is_some() {
                break;
            }
        }
        if b.winner.is_none() {
            b.apply(BattleCommand::EndTurn).expect("apply succeeds");
        }
    }
    b
}

#[test]
fn scripted_battles_always_finish() {
    for seed in 0..40 {
        let a = vec![
            spearman(Side::Attacker, (3, 9)),
            spearman(Side::Attacker, (3, 11)),
            spearman(Side::Attacker, (3, 10)),
        ];
        let d = vec![
            BattleUnit::simple(Side::Defender, "swordsman", 3, 2, 4, 3).at((17, 10)),
            BattleUnit::simple(Side::Defender, "swordsman", 3, 2, 4, 3).at((17, 11)),
        ];
        let b = play_out(seed, a, d);
        assert!(b.winner.is_some(), "seed {seed}");
        assert!(b.turn < TURN_LIMIT, "seed {seed}: took {} turns", b.turn);
    }
}

#[test]
fn the_stronger_side_usually_wins() {
    let mut strong_wins = 0;
    for seed in 0..60 {
        let a = vec![BattleUnit::simple(Side::Attacker, "strong", 6, 3, 5, 3).at((3, 10))];
        let d = vec![BattleUnit::simple(Side::Defender, "weak", 3, 2, 2, 1).at((17, 10))];
        if play_out(seed, a, d).winner == Some(Side::Attacker) {
            strong_wins += 1;
        }
    }
    assert!(strong_wins > 50, "{strong_wins} of 60");
}

#[test]
fn obstacles_shape_where_units_can_go() {
    let mut map = BattleMap::open();
    for y in 0..GRID_H {
        map.set((10, y), Cell::Blocked);
    }
    let mut b = Battle::new(
        A,
        D,
        map,
        vec![
            spearman(Side::Attacker, (9, 10)),
            spearman(Side::Defender, (12, 10)),
        ],
        1,
    );
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
    assert_eq!(GRID_W, 21);
}

// ---- joining the game ------------------------------------------------------------------

fn game() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(5);
    g.content = c;
    g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
    for id in [A, D] {
        g.add_player(id, "");
    }
    g
}

fn at(x: u32, y: u32) -> crate::map::Position {
    crate::map::Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

#[test]
fn a_game_battle_copies_the_units_and_writes_the_results_back() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let a2 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    let events = g.apply(Command::BeginBattle {
        attackers: vec![a1, a2],
        defenders: vec![d1],
    });
    assert!(
        matches!(events[0], Event::BattleStarted { attacker, defender, .. } if attacker == A && defender == D)
    );
    assert!(g.in_battle());
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(b.units.len(), 3);
    // Statistics come from the overland unit: High Men Cavalry have melee 4, defence 2, 3 hit
    // points a figure and 4 figures.
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!((u.melee, u.defense, u.hits, u.figures), (4, 2, 3, 4));
    assert_eq!(u.moves, g.units().unit_max_moves(a1));
    assert_eq!(u.cell, (6, 12));
    assert_eq!(b.unit(2).ok_or("unit is missing")?.cell, (12, 12));
    // A second battle is refused while this one runs.
    let again = g.apply(Command::BeginBattle {
        attackers: vec![a1],
        defenders: vec![d1],
    });
    assert_eq!(
        again,
        vec![Event::Rejected(Rejection::BattleStart(
            BattleStartError::InProgress
        ))]
    );
    // Fight to the finish by commands.
    let mut ended = false;
    for _ in 0..200 {
        let (side, ids): (Side, Vec<usize>) = {
            let b = g.battle().ok_or("battle is missing")?;
            (b.active, b.active_units(b.active).map(|u| u.id).collect())
        };
        for id in ids {
            let target = {
                let b = g.battle().ok_or("battle is missing")?;
                b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            };
            let Some((t, cell)) = target else { break };
            // Walk towards the target, then attack.
            let step = {
                let b = g.battle().ok_or("battle is missing")?;
                b.reachable(id)
                    .into_iter()
                    .min_by_key(|(p, _)| distance(*p, cell))
                    .map(|(p, _)| p)
            };
            if let Some(to) = step {
                g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
            }
            let evs = g.apply(Command::Battle(BattleCommand::Attack {
                unit: id,
                target: t,
            }));
            if evs.iter().any(|e| matches!(e, Event::BattleEnded { .. })) {
                ended = true;
                break;
            }
        }
        if ended {
            break;
        }
        g.apply(Command::Battle(BattleCommand::EndTurn));
    }
    assert!(ended, "the battle finished");
    assert!(!g.in_battle());
    // Whoever lost everything is gone; survivors carry their wounds.
    let alive = |u| g.world.get::<&crate::unit::Unit>(u).is_ok();
    assert!(alive(d1) != (alive(a1) || alive(a2)) || (alive(d1) && (alive(a1) || alive(a2))));
    for u in [a1, a2, d1] {
        if alive(u) {
            let (hp, max) = g.units().unit_hits(u);
            assert!(hp >= 1 && hp <= max, "hit points {hp}/{max}");
        }
    }
    Ok(())
}

#[test]
fn battles_need_two_sides_of_opposing_players_standing_together() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let a_far = g.spawn_unit(A, at(8, 8), walker);
    let mine = g.spawn_unit(A, at(5, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    let reject = |g: &mut Game, att: Vec<hecs::Entity>, def: Vec<hecs::Entity>| {
        let events = g.apply(Command::BeginBattle {
            attackers: att,
            defenders: def,
        });
        // Meeting is no part of this question.
        events
            .into_iter()
            .filter(|e| !matches!(e, Event::Diplomacy(_)))
            .collect::<Vec<_>>()
    };
    let bad = vec![Event::Rejected(Rejection::BattleStart(
        BattleStartError::BadSides,
    ))];
    assert_eq!(reject(&mut g, vec![], vec![d]), bad);
    assert_eq!(reject(&mut g, vec![a], vec![]), bad);
    assert_eq!(
        reject(&mut g, vec![a, a_far], vec![d]),
        bad,
        "not on one square"
    );
    assert_eq!(reject(&mut g, vec![a], vec![mine]), bad, "the same player");
    assert!(!g.in_battle());
    Ok(())
}

#[test]
fn wounded_units_enter_a_battle_wounded() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    // 4 figures x 3 hit points: eleven damage leaves one figure with one hit point.
    g.world.insert_one(a, crate::unit::Hurt(11))?;
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    let u = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    assert_eq!((u.figures, u.hit_points()), (1, 1));
    // Fleeing ends it at once, and the wound is still there afterwards (unless the unit was
    // one of the half that is lost on the way).
    let ev = g.apply(Command::Battle(BattleCommand::Flee));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BattleEnded { winner, .. } if *winner == D)));
    assert!(!g.world.contains(a) || g.units().unit_hits(a).0 == 1);
    Ok(())
}

/// A fight of `n` walkers against one, at the given difficulty, with `held` enchantments put
/// on the first attackers before the attacker flees. Returns the attackers (alive or not).
fn flee_with(difficulty: u8, n: usize, held: &[&str]) -> (Game, Vec<hecs::Entity>) {
    let mut g = game();
    g.settings =
        crate::game::setting::GameSettings::for_difficulty(&g.content.settings, difficulty);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .expect("id_of succeeds");
    let att: Vec<_> = (0..n).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let d = g.spawn_unit(D, at(5, 4), walker);
    g.apply(Command::BeginBattle {
        attackers: att.clone(),
        defenders: vec![d],
    });
    for (i, enchantment) in held.iter().enumerate() {
        let battle = &mut g.battle.as_mut().expect("as_mut succeeds").battle;
        let unit = battle
            .units
            .iter_mut()
            .filter(|u| u.side == Side::Attacker)
            .nth(i)
            .expect("value succeeds");
        unit.enchantments.push((*enchantment).to_string());
    }
    g.apply(Command::Battle(BattleCommand::Flee));
    (g, att)
}

#[test]
fn on_intro_and_easy_the_attacker_loses_nobody_by_fleeing() {
    for difficulty in [0, 1] {
        let (g, att) = flee_with(difficulty, 9, &[]);
        assert!(
            att.iter().all(|&u| g.world.contains(u)),
            "level {difficulty}"
        );
    }
}

#[test]
fn units_asleep_confused_or_webbed_are_always_lost_when_their_side_flees() {
    // On Intro nobody else is lost, so exactly the three held units go.
    let (g, att) = flee_with(0, 6, &["black_sleep", "confusion", "web"]);
    let alive: Vec<bool> = att.iter().map(|&u| g.world.contains(u)).collect();
    assert_eq!(alive, [false, false, false, true, true, true]);
    // On Average every unit rolls, but all three held ones are certain to go.
    let (g, att) = flee_with(2, 3, &["black_sleep", "confusion", "web"]);
    assert!(att.iter().all(|&u| !g.world.contains(u)));
}

#[test]
fn ranged_abilities_map_to_battle_kinds() {
    use super::game::ranged_kind_of;
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(ranged_kind_of(&v(&["bow"])), RangedKind::Missile);
    assert_eq!(ranged_kind_of(&v(&["rock_throwing"])), RangedKind::Missile);
    assert_eq!(ranged_kind_of(&v(&["thrown"])), RangedKind::Short);
    assert_eq!(
        ranged_kind_of(&v(&["fire_breath", "flying"])),
        RangedKind::Short
    );
    assert_eq!(ranged_kind_of(&v(&["ice_bolt"])), RangedKind::Magic);
    assert_eq!(ranged_kind_of(&v(&["forester"])), RangedKind::None);
}

fn found(g: &mut Game, owner: PlayerId, pos: crate::map::Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("high_elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 3,
    });
    g.economy().city_at(pos).expect("city_at succeeds")
}

#[test]
fn auto_resolve_fights_to_the_end_and_the_winner_takes_the_square() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attackers: Vec<_> = (0..4).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let defender = g.spawn_unit(D, at(5, 4), walker);
    let events = g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers: attackers.clone(),
        defenders: vec![defender],
    });
    assert!(!g.in_battle(), "{events:?}");
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::BattleEnded { winner, .. } if *winner == A)));
    assert!(!g.world.contains(defender));
    let survivors: Vec<_> = attackers.iter().filter(|&&u| g.world.contains(u)).collect();
    assert!(!survivors.is_empty());
    for &&u in &survivors {
        assert_eq!(
            g.units().unit_position(u),
            Some(at(5, 4)),
            "moved onto the square"
        );
    }
    // Another battle can start afterwards.
    assert!(g.battle().is_none());
    Ok(())
}

#[test]
fn a_battle_won_at_a_city_captures_it() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let city = found(&mut g, D, at(5, 4));
    let defender = g.spawn_unit(D, at(5, 4), walker);
    let attackers: Vec<_> = (0..5).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let events = g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers,
        defenders: vec![defender],
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == A)),
        "{events:?}"
    );
    assert_eq!(g.economy().city_owner(city), Some(A));
    Ok(())
}

#[test]
fn an_empty_enemy_city_is_taken_by_walking_in() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let city = found(&mut g, D, at(5, 4));
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let events = g.apply(Command::MoveStack {
        units: vec![unit],
        to: at(5, 4),
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CityCaptured { .. })),
        "{events:?}"
    );
    assert_eq!(g.economy().city_owner(city), Some(A));
    assert_eq!(g.units().unit_position(unit), Some(at(5, 4)));
    Ok(())
}

#[test]
fn a_losing_attacker_stays_where_it_was() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attacker = g.spawn_unit(A, at(4, 4), walker);
    let defenders: Vec<_> = (0..6).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers: vec![attacker],
        defenders: defenders.clone(),
    });
    assert!(!g.world.contains(attacker));
    assert!(defenders.iter().any(|&d| g.world.contains(d)));
    for &d in defenders.iter().filter(|&&d| g.world.contains(d)) {
        assert_eq!(g.units().unit_position(d), Some(at(5, 4)));
    }
    Ok(())
}

#[test]
fn auto_resolve_needs_an_adjacent_target() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(2, 4), walker);
    let d = g.spawn_unit(D, at(6, 4), walker);
    let events = g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers: vec![a],
        defenders: vec![d],
    });
    assert_eq!(
        events,
        vec![Event::Rejected(Rejection::BattleStart(
            BattleStartError::NotAdjacent
        ))]
    );
    assert!(!g.in_battle());
    Ok(())
}

fn instant_unit(side: Side, cell: Pos, ability: &str) -> BattleUnit {
    let mut u = spearman(side, cell);
    u.abilities = vec![ability.into()];
    u
}

#[test]
fn teleporting_and_merging_units_go_anywhere_free_for_one_move() -> TestResult {
    for ability in ["teleport", "merging"] {
        let mut map = BattleMap::open();
        map.set((10, 10), Cell::Blocked);
        let mut b = Battle::new(
            A,
            D,
            map,
            vec![
                instant_unit(Side::Attacker, (3, 10), ability),
                spearman(Side::Defender, (17, 10)),
            ],
            1,
        );
        let reach = b.reachable(0);
        assert_eq!(
            reach.get(&(15, 3)),
            Some(&2),
            "{ability}: far away, one move"
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
            2,
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
    }
    Ok(())
}

#[test]
fn a_teleporter_cannot_go_into_rock_and_a_walker_cannot_teleport() {
    let mut map = BattleMap::open();
    map.set((10, 10), Cell::Blocked);
    let mut b = Battle::new(
        A,
        D,
        map,
        vec![
            instant_unit(Side::Attacker, (3, 10), "teleport"),
            spearman(Side::Defender, (17, 10)),
        ],
        1,
    );
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (10, 10)
        }),
        Err(BattleError::Unreachable)
    );
    let mut w = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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
    use super::game::deployment_order;
    let mut units: Vec<BattleUnit> = (0..8)
        .map(|_| BattleUnit::simple(Side::Attacker, "u", 1, 10, 2, 0))
        .collect();
    for (i, u) in units.iter_mut().enumerate() {
        if i % 2 == 0 {
            u.ranged = 3;
            u.ammo = 4;
        }
    }
    assert_eq!(deployment_order(&units), vec![1, 3, 5, 7, 0, 2, 4, 6]);
    // Six shooters and one melee unit: the two strongest in melee move up front.
    let mut units: Vec<BattleUnit> = (0..7)
        .map(|_| BattleUnit::simple(Side::Attacker, "u", 1, 10, 2, 0))
        .collect();
    for (i, u) in units.iter_mut().enumerate().skip(1) {
        u.ranged = 3;
        u.ammo = 4;
        u.melee = i as i32;
    }
    assert_eq!(deployment_order(&units), vec![0, 6, 5, 1, 2, 3, 4]);
}

#[test]
fn a_nodes_aura_helps_its_realms_creatures_on_both_sides() {
    let mut chaos = spearman(Side::Attacker, (3, 10));
    chaos.realm = Some(Realm::Chaos);
    chaos.ranged = 3;
    let mut nature = spearman(Side::Defender, (17, 10));
    nature.realm = Some(Realm::Nature);
    let mut b = duel(chaos, nature, 4);
    let (melee, defense, resist) = (b.units[0].melee, b.units[0].defense, b.units[0].resistance);
    b.node_aura = Some(Realm::Chaos);
    b.refresh_fields();
    assert_eq!(b.units[0].melee, melee + 2);
    assert_eq!(b.units[0].ranged, 5);
    assert_eq!(b.units[0].defense, defense + 2);
    assert_eq!(b.units[0].resistance, resist + 2);
    assert_eq!(
        b.units[1].defense,
        spearman(Side::Defender, (17, 10)).defense
    );
    // Gone with the aura.
    b.node_aura = None;
    b.refresh_fields();
    assert_eq!(b.units[0].melee, melee);
}

#[test]
fn a_node_stops_spells_of_other_realms_but_not_its_own() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
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
fn counter_magic_resists_by_its_strength_and_loses_five_per_attempt() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        4,
    );
    // No reserve: nothing is stopped, nothing drains.
    assert!(!b.counter_magic_stops(Side::Attacker, 10));
    assert_eq!(b.counter_magic, [0, 0]);
    // The defender holds a reserve of 100; only the attacker's spells face it.
    b.cast_counter_magic(Side::Defender, 100);
    assert_eq!(b.counter_magic, [0, 100]);
    assert!(
        !b.counter_magic_stops(Side::Defender, 10),
        "the holder's own spells pass"
    );
    assert_eq!(b.counter_magic, [0, 100]);
    // Against difficulty 100 the reserve of 100 stops about half, and every try costs five.
    let mut stopped = 0;
    for _ in 0..10 {
        if b.counter_magic_stops(Side::Attacker, 100) {
            stopped += 1;
        }
    }
    assert_eq!(b.counter_magic, [0, 50], "ten attempts, five each");
    assert!((1..=9).contains(&stopped), "stopped {stopped} of 10");
    // A drained reserve stops nothing.
    for _ in 0..10 {
        b.counter_magic_stops(Side::Attacker, 1);
    }
    assert_eq!(b.counter_magic, [0, 0]);
    assert!(!b.counter_magic_stops(Side::Attacker, 1));
}

/// Hit points a unit lost over many seeded one-round fights: `attacker` strikes (or shoots) at
/// `target`, which is put next to it.
fn damage_over_seeds(
    attacker: impl Fn() -> BattleUnit,
    target: impl Fn() -> BattleUnit,
    shoot: bool,
) -> i32 {
    let mut total = 0;
    for seed in 0..300 {
        let mut b = duel(attacker(), target(), seed);
        let command = if shoot {
            BattleCommand::Shoot { unit: 0, target: 1 }
        } else {
            BattleCommand::Attack { unit: 0, target: 1 }
        };
        b.apply(command).expect("apply succeeds");
        total += b.unit(1).expect("unit succeeds").max_hit_points()
            - b.unit(1).expect("unit succeeds").hit_points();
    }
    total
}

fn with_abilities(mut u: BattleUnit, names: &[&str]) -> BattleUnit {
    u.abilities = names.iter().map(|s| s.to_string()).collect();
    u
}

fn dummy(abilities: &[&str]) -> BattleUnit {
    with_abilities(
        BattleUnit::simple(Side::Defender, "dummy", 60, 20, 0, 2).at((4, 10)),
        abilities,
    )
}

#[test]
fn weapon_immunity_blunts_normal_blows_but_not_magical_ones() {
    let swordsman = || BattleUnit::simple(Side::Attacker, "swordsman", 6, 2, 4, 2).at((3, 10));
    let magical = || BattleUnit {
        magical_weapon: true,
        ..swordsman()
    };
    let open = damage_over_seeds(swordsman, || dummy(&[]), false);
    let stone = damage_over_seeds(swordsman, || dummy(&["weapon_immunity"]), false);
    let stone_vs_magic = damage_over_seeds(magical, || dummy(&["weapon_immunity"]), false);
    assert!(stone * 3 < open, "{stone} vs {open}");
    let ratio = f64::from(stone_vs_magic) / f64::from(open);
    assert!((ratio - 1.0).abs() < 0.1, "{ratio}");
}

#[test]
fn missile_immunity_stops_arrows_completely() {
    let archer = || {
        let mut u = BattleUnit::simple(Side::Attacker, "archer", 6, 1, 1, 1).at((3, 10));
        u.ranged = 5;
        u.ammo = 4;
        u.ranged_kind = RangedKind::Missile;
        with_abilities(u, &["bow"])
    };
    let open = damage_over_seeds(archer, || dummy(&[]), true);
    let proof = damage_over_seeds(archer, || dummy(&["missile_immunity"]), true);
    assert!(open > 20, "{open}");
    assert_eq!(proof, 0);
}

#[test]
fn magic_immunity_stops_a_bolt_that_a_fire_immunity_does_not() {
    let mage = || {
        let mut u = BattleUnit::simple(Side::Attacker, "wind_mage", 3, 1, 1, 1).at((3, 10));
        u.ranged = 6;
        u.ammo = 4;
        u.ranged_kind = RangedKind::Magic;
        with_abilities(u, &["fireball_attack"])
    };
    let open = damage_over_seeds(mage, || dummy(&[]), true);
    let warded = damage_over_seeds(mage, || dummy(&["magic_immunity"]), true);
    let fireproof = damage_over_seeds(mage, || dummy(&["fire_immunity"]), true);
    assert!(open > 20, "{open}");
    assert_eq!(warded, 0);
    // The bolt is magic, not fire (only fire breath is stopped by fire immunity).
    assert!(fireproof * 10 > open * 9, "{fireproof} vs {open}");
}

#[test]
fn fire_breath_is_stopped_by_fire_immunity_and_magic_immunity() {
    let dragon = || {
        let mut u = BattleUnit::simple(Side::Attacker, "dragon", 1, 20, 1, 1).at((3, 10));
        u.ranged = 9;
        u.ranged_kind = RangedKind::Short;
        with_abilities(u, &["fire_breath"])
    };
    let open = damage_over_seeds(dragon, || dummy(&[]), false);
    assert!(open > 0);
    for ability in ["fire_immunity", "magic_immunity"] {
        // The melee blow that follows the breath still lands, so compare with a unit that
        // takes only that: the breath's share is gone.
        let shielded = damage_over_seeds(dragon, || dummy(&[ability]), false);
        assert!(shielded * 2 < open, "{ability}: {shielded} vs {open}");
    }
}

#[test]
fn an_illusion_ignores_armor_unless_the_defender_sees_through_it() {
    let thief = || {
        with_abilities(
            BattleUnit::simple(Side::Attacker, "nightblade", 4, 2, 4, 1).at((3, 10)),
            &["illusion_attack"],
        )
    };
    let armoured = |names: &[&str]| {
        let mut u = dummy(names);
        u.defense = 8;
        u
    };
    let plain = damage_over_seeds(thief, || armoured(&[]), false);
    let seen = damage_over_seeds(thief, || armoured(&["illusion_immunity"]), false);
    assert!(plain > seen * 2, "{plain} vs {seen}");
}

#[test]
fn a_defender_with_a_gaze_stones_the_attackers_before_they_strike() -> TestResult {
    let basilisk = || {
        let mut u = BattleUnit::simple(Side::Defender, "basilisk", 1, 30, 0, 3).at((4, 10));
        u.ranged = 1;
        u.ranged_kind = RangedKind::Short;
        u.special_level = 3;
        with_abilities(u, &["stoning_gaze"])
    };
    let mut lost = 0;
    let mut gazeless_lost = 0;
    for seed in 0..100 {
        let squad = || {
            let mut u = BattleUnit::simple(Side::Attacker, "squad", 10, 1, 1, 0).at((3, 10));
            u.resistance = 3;
            u
        };
        let mut b = duel(squad(), basilisk(), seed);
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        lost += 10 - b.unit(0).ok_or("unit is missing")?.figures;
        let mut plain = basilisk();
        plain.abilities.clear();
        let mut b = duel(squad(), plain, seed);
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        gazeless_lost += 10 - b.unit(0).ok_or("unit is missing")?.figures;
    }
    // Resistance 3 at -3 fails nine times in ten: about nine of the ten figures turn to stone.
    assert!(lost > 700, "{lost}");
    assert!(gazeless_lost < 100, "{gazeless_lost}");
    Ok(())
}

#[test]
fn a_life_stealing_shot_heals_the_shooter() -> TestResult {
    let lord = || {
        let mut u = BattleUnit::simple(Side::Attacker, "lord", 2, 10, 1, 1).at((3, 10));
        u.ranged = 4;
        u.ammo = 4;
        u.ranged_kind = RangedKind::Missile;
        u.special_level = 3;
        u.front_damage = 8;
        with_abilities(u, &["bow", "life_steal"])
    };
    let mut healed = 0;
    for seed in 0..50 {
        let mut b = duel(lord(), dummy(&[]), seed);
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        healed += 8 - b.unit(0).ok_or("unit is missing")?.front_damage;
    }
    assert!(healed > 0);
    Ok(())
}

#[test]
fn a_power_drain_shot_takes_two_to_twenty_mana_from_a_caster_that_fails_its_check() -> TestResult {
    let drainer = || {
        let mut u = BattleUnit::simple(Side::Attacker, "hero", 2, 10, 1, 1).at((3, 10));
        u.ranged = 6;
        u.ammo = 6;
        u.ranged_kind = RangedKind::Missile;
        with_abilities(u, &["bow", "power_drain"])
    };
    let caster = || {
        let mut u = dummy(&[]);
        u.mana = 40;
        u.resistance = 2;
        u
    };
    let (mut drained, mut tries) = (0, 0);
    for seed in 0..60 {
        let mut b = duel(drainer(), caster(), seed);
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        let lost = 40 - b.unit(1).ok_or("unit is missing")?.mana;
        assert!((0..=20).contains(&lost) && lost != 1, "{lost}");
        tries += 1;
        drained += i32::from(lost > 0);
    }
    assert!(drained > 5 && drained < tries, "{drained} of {tries}");
    Ok(())
}

fn crusher(side: Side, cell: Pos, ranged: bool) -> BattleUnit {
    let mut u = with_abilities(
        BattleUnit::simple(side, "giant", 1, 20, 8, 3).at(cell),
        &["wall_crusher"],
    );
    if ranged {
        u.ranged = 6;
        u.ammo = 4;
        u.ranged_kind = RangedKind::Missile;
        u.abilities.push("rock_throwing".into());
    }
    u
}

fn siege(units: Vec<BattleUnit>, walled: bool, seed: u64) -> Battle {
    Battle::new(A, D, BattleMap::open().with_city(walled), units, seed)
}

fn guard(cell: Pos) -> BattleUnit {
    BattleUnit::simple(Side::Defender, "guard", 4, 3, 3, 3).at(cell)
}

#[test]
fn a_wall_crusher_brings_a_wall_down_half_the_time_in_melee() -> TestResult {
    let mut fell = 0;
    for seed in 0..400 {
        let mut b = siege(
            vec![crusher(Side::Attacker, (11, 11), false), guard((14, 11))],
            true,
            seed,
        );
        let events = b
            .apply(BattleCommand::AttackWall {
                unit: 0,
                cell: (12, 11),
            })
            .map_err(|e| format!("{e:?}"))?;
        let BattleEvent::WallAttacked { destroyed, .. } = events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(destroyed, !b.map.has_wall((12, 11)));
        fell += i32::from(destroyed);
        // The blow takes the unit's turn.
        assert!(b.unit(0).ok_or("unit is missing")?.attacked);
        assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    }
    assert!((150..250).contains(&fell), "{fell}");
    Ok(())
}

#[test]
fn shooting_a_wall_works_a_quarter_of_the_time_and_costs_a_shot() -> TestResult {
    let mut fell = 0;
    for seed in 0..400 {
        let mut b = siege(
            vec![crusher(Side::Attacker, (8, 11), true), guard((14, 11))],
            true,
            seed,
        );
        b.apply(BattleCommand::ShootWall {
            unit: 0,
            cell: (12, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(b.unit(0).ok_or("unit is missing")?.ammo, 3);
        fell += i32::from(!b.map.has_wall((12, 11)));
    }
    assert!((60..140).contains(&fell), "{fell}");
    Ok(())
}

#[test]
fn only_attacking_wall_crushers_may_strike_a_standing_wall() -> TestResult {
    let mut b = siege(
        vec![
            BattleUnit::simple(Side::Attacker, "spearman", 3, 2, 3, 2).at((11, 11)),
            crusher(Side::Attacker, (11, 13), false),
            guard((14, 11)),
        ],
        true,
        1,
    );
    let cell = (12, 11);
    assert_eq!(
        b.apply(BattleCommand::AttackWall { unit: 0, cell }),
        Err(BattleError::NotWallCrusher)
    );
    // Not next to the wall, and no wall there.
    assert_eq!(
        b.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: (12, 10)
        }),
        Err(BattleError::NotAdjacent)
    );
    assert_eq!(
        b.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: (11, 12)
        }),
        Err(BattleError::NoWall)
    );
    assert_eq!(
        b.apply(BattleCommand::ShootWall {
            unit: 1,
            cell: (12, 12)
        }),
        Err(BattleError::NoRangedAttack)
    );
    // Once the defenders' turn comes, a defending crusher is no wall breaker.
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    let mut b2 = siege(
        vec![
            BattleUnit::simple(Side::Attacker, "x", 1, 1, 1, 1).at((3, 3)),
            crusher(Side::Defender, (13, 11), false),
        ],
        true,
        1,
    );
    b2.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        b2.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: (12, 11)
        }),
        Err(BattleError::NotWallCrusher)
    );
    // And a city without walls has none to strike.
    let mut open = siege(
        vec![crusher(Side::Attacker, (11, 11), false), guard((14, 11))],
        false,
        1,
    );
    assert_eq!(
        open.apply(BattleCommand::AttackWall {
            unit: 0,
            cell: (12, 11)
        }),
        Err(BattleError::NoWall)
    );
    Ok(())
}

#[test]
fn a_wall_crusher_hitting_a_unit_on_the_wall_may_bring_the_wall_down_too() -> TestResult {
    let mut fell = 0;
    for seed in 0..300 {
        let mut b = siege(
            vec![crusher(Side::Attacker, (11, 11), false), guard((12, 11))],
            true,
            seed,
        );
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        fell += i32::from(!b.map.has_wall((12, 11)));
    }
    assert!((110..190).contains(&fell), "{fell}");
    Ok(())
}

#[test]
fn walls_add_three_to_the_defense_on_the_wall_and_one_behind_it_against_attackers_outside(
) -> TestResult {
    // How many hit points a target loses over many fights, with its defense of 0.
    let lost = |target: Pos, walled: bool, from_inside: bool| {
        let mut total = 0;
        for seed in 0..300 {
            let from = if from_inside {
                (13, 11)
            } else {
                (11, target.1)
            };
            let attacker = BattleUnit::simple(Side::Attacker, "a", 6, 2, 4, 0).at(from);
            let mut target_unit = BattleUnit::simple(Side::Defender, "d", 60, 30, 0, 0).at(target);
            target_unit.defense = 2;
            let mut b = siege(vec![attacker, target_unit], walled, seed);
            if from_inside {
                // Put the attacker beside the target and let it strike.
                b.units[0].cell = (14, 11);
            }
            let _ = b.apply(BattleCommand::Attack { unit: 0, target: 1 });
            total += b.unit(1).expect("unit succeeds").max_hit_points()
                - b.unit(1).expect("unit succeeds").hit_points();
        }
        total
    };
    let bare = lost((13, 11), false, false);
    let behind = lost((13, 11), true, false);
    // The target is on a wall square: (12, 11) is a ring square; the attacker is next to it.
    let on_wall = lost((12, 11), true, false);
    let on_open_ring = lost((12, 11), false, false);
    // Against an attacker already inside the city area, the walls give nothing.
    let inside_attacker = lost((13, 11), true, true);
    // (13, 11) is two squares from (11, 11), so the first run never reached it: compare the
    // ring squares, where the attacker is adjacent, and the inside ones fought from inside.
    assert!(
        on_wall * 3 < on_open_ring * 2,
        "{on_wall} vs {on_open_ring}"
    );
    assert!(
        bare == 0 && behind == 0,
        "an attacker two squares away cannot strike"
    );
    assert!(inside_attacker > 0);
    Ok(())
}

#[test]
fn attackers_inside_the_city_hurt_it_each_turn() -> TestResult {
    let mut b = siege(
        vec![
            BattleUnit::simple(Side::Attacker, "in", 2, 2, 2, 2).at((13, 11)),
            BattleUnit::simple(Side::Attacker, "out", 2, 2, 2, 2).at((9, 11)),
            guard((15, 11)),
        ],
        true,
        1,
    );
    // The first turn counts the attacker that starts inside.
    assert_eq!(b.city_damage, 1);
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.city_damage, 1, "the defenders' turn adds nothing");
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.city_damage, 2);
    // A battle in the open has no city to hurt.
    let b = duel(
        spearman(Side::Attacker, (13, 11)),
        spearman(Side::Defender, (15, 11)),
        1,
    );
    assert_eq!(b.city_damage, 0);
    Ok(())
}

#[test]
fn the_battle_ai_takes_a_walled_city() -> TestResult {
    // Attackers with a crusher against a few guards behind the wall: the fight ends, and the
    // crusher has opened the walls or the army has come through the gate.
    let mut results = (0, 0);
    for seed in 0..12 {
        let attackers = [
            crusher(Side::Attacker, (6, 12), false),
            BattleUnit::simple(Side::Attacker, "a1", 4, 4, 4, 3).at((6, 11)),
            BattleUnit::simple(Side::Attacker, "a2", 4, 4, 4, 3).at((6, 13)),
            BattleUnit::simple(Side::Attacker, "a3", 4, 4, 4, 3).at((5, 12)),
        ];
        let defenders = [guard(GATE_SPOT), guard((13, 11)), guard((13, 12))];
        let mut b = siege(attackers.into_iter().chain(defenders).collect(), true, seed);
        for _ in 0..5_000 {
            if b.winner.is_some() {
                break;
            }
            let c = ai::next_command(&b);
            b.apply(c).map_err(|e| format!("{e:?}"))?;
        }
        assert!(b.winner.is_some(), "seed {seed} never ended");
        match b.winner {
            Some(Side::Attacker) => results.0 += 1,
            _ => results.1 += 1,
        }
    }
    assert!(results.0 > 0, "the attackers never get in: {results:?}");
    Ok(())
}

const GATE_SPOT: Pos = (12, 12);

// ---- spells -----------------------------------------------------------------

mod casting {
    use super::*;
    use crate::battle::attack::{spell_attack, Combatant, Mode, Volley};
    use crate::battle::casting::{range_percent, BattleCaster};
    use crate::battle::immunity::{AttackNature, Immunities, Realm};
    use crate::rules::effects::DamageTraits;
    use crate::rules::stats::{ModOp, Modifier, Source, Stat};
    use crate::testdir::TestResult;

    #[test]
    fn casting_costs_more_the_further_from_the_fortress() {
        // At the fortress half; within five squares as is; then half as much again for each
        // five more, up to three times; another plane is as far as it gets.
        let table = [
            (Some(0), 5),
            (Some(1), 10),
            (Some(5), 10),
            (Some(6), 15),
            (Some(10), 15),
            (Some(11), 20),
            (Some(16), 25),
            (Some(21), 30),
            (Some(40), 30),
            (None, 30),
        ];
        for (distance, percent) in table {
            assert_eq!(range_percent(distance, false), percent, "{distance:?}");
        }
        // A channeler is two squares away wherever they are, except at the fortress.
        assert_eq!(range_percent(Some(25), true), 10);
        assert_eq!(range_percent(None, true), 10);
        assert_eq!(range_percent(Some(0), true), 5);
    }

    #[test]
    fn a_mana_budget_caps_what_the_wizard_can_afford_and_is_used_up_by_casting() -> TestResult {
        let mut near = BattleCaster::new(20, 100, 10);
        assert_eq!(near.affordable(), 20);
        near.budget = Some(8);
        assert_eq!(near.affordable(), 8);
        // Two mana for each point of skill: a budget of 8 pays for 4.
        let mut far = BattleCaster::new(20, 100, 20);
        far.budget = Some(8);
        assert_eq!(far.affordable(), 4);
        // Casting spends it.
        let mut b = duel(
            spearman(Side::Attacker, (3, 10)),
            spearman(Side::Defender, (17, 10)),
            1,
        );
        b.set_caster(Side::Attacker, BattleCaster::new(20, 100, 10));
        b.set_mana_budget(Side::Attacker, Some(8));
        assert_eq!(b.pay_for_spell(5), 5);
        let c = b.caster(Side::Attacker).ok_or("caster is missing")?;
        assert_eq!((c.budget, c.mana, c.skill_left), (Some(3), 95, 15));
        assert_eq!(c.affordable(), 3);
        // A side without a wizard has nothing to limit; no budget means no limit.
        b.set_mana_budget(Side::Defender, Some(1));
        assert!(b.caster(Side::Defender).is_none());
        b.set_mana_budget(Side::Attacker, None);
        assert_eq!(
            b.caster(Side::Attacker)
                .ok_or("caster is missing")?
                .affordable(),
            15
        );
        Ok(())
    }

    #[test]
    fn what_a_wizard_can_afford_is_the_lesser_of_skill_and_what_the_reserve_pays_for() {
        let near = BattleCaster::new(20, 100, 10);
        assert_eq!(near.affordable(), 20);
        let far = BattleCaster::new(20, 30, 30);
        assert_eq!(far.affordable(), 10, "30 mana at three times pays for 10");
        assert_eq!(far.mana_for(10), 30);
        assert_eq!(
            BattleCaster::new(20, 100, 5).mana_for(7),
            3,
            "halved, rounded down"
        );
    }

    fn target(figures: u32, hits: i32, defense: i32) -> DefenderState {
        DefenderState {
            hits,
            defense,
            figures,
            figures_max: figures,
            ..Default::default()
        }
    }

    fn bolt(strength: i32) -> Combatant {
        Combatant {
            strength,
            to_hit: 0,
            mode: Mode::Ranged,
            ..Default::default()
        }
    }

    fn mean(volley: Volley, spell: &Combatant, def: &DefenderState, trials: u64) -> f64 {
        let total: i32 = (0..trials)
            .map(|seed| {
                let mut state = def.clone();
                let mut rng = Pcg64Mcg::seed_from_u64(seed);
                spell_attack(spell, volley, &mut state, &mut rng).points
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
        let doom = Combatant {
            ignores_defense: true,
            ..bolt(15)
        };
        let wall = target(1, 100, 6);
        assert!(mean(Volley::Bolt, &piercing, &wall, 2000) > armoured + 0.5);
        assert_eq!(mean(Volley::Bolt, &doom, &wall, 10), 15.0);
    }

    #[test]
    fn a_fireball_hits_each_figure_and_none_loses_more_than_one_figure() {
        let squad = target(6, 2, 0);
        let ball = Combatant {
            ignores_defense: true,
            ..bolt(5)
        };
        let mut state = squad.clone();
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        let dmg = spell_attack(&ball, Volley::Area, &mut state, &mut rng);
        // Five points a figure, capped at its two hit points: a whole squad of six dies.
        assert_eq!((dmg.points, dmg.kills, state.figures), (12, 6, 0));
        let mut state = squad;
        let single = spell_attack(&ball, Volley::Bolt, &mut state, &mut rng);
        assert_eq!(
            single.kills, 2,
            "a bolt of five kills two of them and no more"
        );
    }

    #[test]
    fn warp_lightning_is_a_chain_of_weaker_and_weaker_bolts() {
        let chain = Combatant {
            ignores_defense: true,
            ..bolt(4)
        };
        let mut state = target(10, 5, 0);
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        let dmg = spell_attack(&chain, Volley::Chain, &mut state, &mut rng);
        assert_eq!(dmg.points, 4 + 3 + 2 + 1);
    }

    fn unit_with(abilities: &[&str]) -> BattleUnit {
        let mut u = BattleUnit::simple(Side::Defender, "t", 1, 20, 3, 0);
        u.abilities = abilities.iter().map(|s| s.to_string()).collect();
        u
    }

    #[test]
    fn magic_immunity_and_righteousness_turn_spells_away() {
        let strike = |unit: BattleUnit, realm| {
            let mut b = duel(spearman(Side::Attacker, (3, 5)), unit.at((8, 5)), 1);
            let nature = AttackNature {
                stopped_by: Immunities::default(),
                realm: Some(realm),
                armor_piercing: false,
            };
            b.spell_damage(1, 10, &DamageTraits::default(), nature)
        };
        assert!(strike(unit_with(&[]), Realm::Chaos).is_some());
        assert!(strike(unit_with(&["magic_immunity"]), Realm::Chaos).is_none());
        let mut blessed = unit_with(&[]);
        blessed.enchantments.push("righteousness".into());
        assert!(strike(blessed.clone(), Realm::Death).is_none());
        assert!(
            strike(blessed, Realm::Nature).is_some(),
            "only chaos and death"
        );
    }

    #[test]
    fn a_lasting_spell_changes_the_numbers_and_a_rebuild_takes_it_off_again() -> TestResult {
        let mut b = duel(
            spearman(Side::Attacker, (3, 5)),
            spearman(Side::Defender, (8, 5)),
            1,
        );
        let m = |stat, op, value| Modifier::new(stat, op, value, Source::Other);
        let spell = crate::rules::SpellId::new(1);
        let before = b.units[0].clone();
        b.enchant_unit(
            0,
            spell,
            "haste",
            A,
            &[
                m(Stat::Moves, ModOp::Percent, 100),
                m(Stat::Melee, ModOp::Add, 1),
            ],
            &["first_strike".to_string()],
        );
        let u = &b.units[0];
        assert_eq!(u.moves, before.moves * 2);
        assert_eq!(
            u.moves_left,
            before.moves_left * 2,
            "the extra moves come at once"
        );
        assert_eq!(u.melee, before.melee + 1);
        assert!(u.first_strike && u.has("first_strike"));
        assert_eq!(u.combat_spells, vec![(spell, A)]);
        // A dispel: back to the base, with no spells on.
        let base = u.base.as_deref().ok_or("as_deref is missing")?.clone();
        b.rebuild_unit(0, base, Vec::new(), &[]);
        let u = &b.units[0];
        assert_eq!(
            (u.moves, u.melee, u.first_strike),
            (before.moves, before.melee, false)
        );
        assert!(u.combat_spells.is_empty() && u.base.is_none());
        assert!(u.abilities.is_empty() && u.enchantments.is_empty());
        Ok(())
    }

    #[test]
    fn healing_brings_figures_back_and_a_new_creature_arrives_ready_to_act() {
        let mut hurt = spearman(Side::Attacker, (3, 5));
        hurt.figures = 1;
        hurt.front_damage = 1;
        let mut b = duel(hurt, spearman(Side::Defender, (8, 5)), 1);
        assert_eq!(b.units[0].hit_points(), 1);
        // Two hit points a figure, three figures: five points heal the lot.
        assert_eq!(b.spell_heal(0, 5), 5);
        assert_eq!(b.units[0].figures, 3);
        b.units[0].moves_left = 0;
        let fresh = BattleUnit::simple(Side::Attacker, "p", 1, 5, 2, 0);
        let id = b.add_summoned(Side::Attacker, fresh, (4, 5));
        assert_eq!(id, 2);
        assert_eq!(b.units[2].moves_left, b.units[2].moves);
        assert!(b.units[2].is_active());
    }

    #[test]
    fn creatures_appear_only_on_a_free_square_of_the_casters_half() {
        let b = duel(
            spearman(Side::Attacker, (3, 5)),
            spearman(Side::Defender, (17, 5)),
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
}

// ---- battlefield enchantments -----------------------------------------------

mod fields {
    use super::*;
    use crate::battle::field::{Field, MANA_LEAK_DRAIN};
    use crate::battle::immunity::Realm;
    use crate::rules::stats::{ModOp, Modifier, Source, Stat};
    use crate::rules::SpellId;
    use crate::testdir::TestResult;

    fn spell() -> SpellId {
        SpellId::new(1)
    }

    fn of(side: Side, realm: Option<Realm>, cell: Pos) -> BattleUnit {
        let mut u = BattleUnit::simple(side, "u", 2, 4, 3, 2).at(cell);
        u.ranged = 2;
        u.resistance = 5;
        u.realm = realm;
        u
    }

    fn pair(a: BattleUnit, d: BattleUnit) -> Battle {
        duel(a, d, 3)
    }

    #[test]
    fn light_helps_life_and_hurts_death_and_darkness_does_the_opposite() {
        let mut b = pair(
            of(Side::Attacker, Some(Realm::Life), (3, 5)),
            of(Side::Defender, Some(Realm::Death), (17, 5)),
        );
        let (life, death) = (b.units[0].clone(), b.units[1].clone());
        b.raise_field(Side::Attacker, Field::TrueLight, spell());
        let (l, d) = (&b.units[0], &b.units[1]);
        assert_eq!(
            (l.melee, l.ranged, l.defense, l.resistance),
            (
                life.melee + 1,
                life.ranged + 1,
                life.defense + 1,
                life.resistance + 1
            )
        );
        assert_eq!(
            (d.melee, d.ranged, d.defense, d.resistance),
            (
                death.melee - 1,
                death.ranged - 1,
                death.defense - 1,
                death.resistance - 1
            )
        );
        // Darkness by the other side cancels it, whoever cast either.
        b.raise_field(Side::Defender, Field::Darkness, spell());
        assert_eq!(b.units[0].melee, life.melee);
        assert_eq!(b.units[1].resistance, death.resistance);
        // Darkness alone is the reverse.
        let mut b = pair(
            of(Side::Attacker, Some(Realm::Life), (3, 5)),
            of(Side::Defender, Some(Realm::Death), (17, 5)),
        );
        b.raise_field(Side::Attacker, Field::Darkness, spell());
        assert_eq!(b.units[0].defense, life.defense - 1);
        assert_eq!(b.units[1].defense, death.defense + 1);
    }

    #[test]
    fn holy_bonus_and_resistance_to_all_help_the_whole_side_and_do_not_add_up() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        let plain = b.units[0].clone();
        let enemy = b.units[1].clone();
        let with = |ability: &str, level: i32| {
            let mut u = of(Side::Attacker, None, (3, 6));
            u.abilities.push(ability.to_string());
            u.special_level = level;
            u
        };
        // An archer without melee gets defence and resistance but no attack.
        let mut archer = of(Side::Attacker, None, (3, 7));
        archer.melee = 0;
        let archer_id = b.add_summoned(Side::Attacker, archer, (3, 7));
        let angel = b.add_summoned(Side::Attacker, with("holy_bonus", 2), (3, 6));
        let (p, a) = (&b.units[0], &b.units[archer_id]);
        assert_eq!(
            (p.melee, p.defense, p.resistance),
            (plain.melee + 2, plain.defense + 2, plain.resistance + 2)
        );
        assert_eq!((a.melee, a.defense), (0, plain.defense + 2));
        assert_eq!(
            b.units[angel].melee,
            plain.melee + 2,
            "the bearer gets it too"
        );
        assert_eq!(b.units[1].defense, enemy.defense, "the enemy gets nothing");
        // A second, weaker holy unit does not add, and Resistance to All adds to resistance only.
        b.add_summoned(Side::Attacker, with("holy_bonus", 1), (4, 6));
        assert_eq!(b.units[0].defense, plain.defense + 2);
        b.add_summoned(Side::Attacker, with("resist_all", 1), (4, 7));
        let p = &b.units[0];
        assert_eq!(
            (p.melee, p.resistance),
            (plain.melee + 2, plain.resistance + 3)
        );
        // When the bearer falls the bonus goes at the next refresh.
        b.units[angel].status = Status::Dead;
        b.refresh_fields();
        assert_eq!(
            b.units[0].defense,
            plain.defense + 1,
            "the weaker one still stands"
        );
    }

    #[test]
    fn a_heros_leadership_and_prayermaster_help_the_side_and_only_the_best_counts() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        let plain = b.units[0].clone();
        let enemy = b.units[1].clone();
        let mut archer = of(Side::Attacker, None, (3, 7));
        archer.ranged = 3;
        archer.ranged_kind = RangedKind::Missile;
        let archer = b.add_summoned(Side::Attacker, archer, (3, 7));
        let mut mage = of(Side::Attacker, None, (3, 8));
        mage.ranged = 3;
        mage.ranged_kind = RangedKind::Magic;
        let mage = b.add_summoned(Side::Attacker, mage, (3, 8));
        let imp = b.add_summoned(
            Side::Attacker,
            of(Side::Attacker, Some(Realm::Chaos), (3, 9)),
            (3, 9),
        );
        let mut hero = of(Side::Attacker, None, (3, 6));
        hero.leadership = 3;
        hero.prayermaster = 2;
        let hero = b.add_summoned(Side::Attacker, hero, (3, 6));
        let (p, a) = (&b.units[0], &b.units[archer]);
        assert_eq!(p.melee, plain.melee + 3);
        assert_eq!(p.resistance, plain.resistance + 2);
        assert_eq!(a.ranged, 3 + 1, "half of it, rounded down, for a shot");
        assert_eq!(b.units[mage].ranged, 3, "magic shots get none");
        assert_eq!(b.units[imp].melee, plain.melee, "nor does a creature");
        assert_eq!(b.units[hero].melee, plain.melee + 3, "the hero too");
        assert_eq!(b.units[1].melee, enemy.melee, "the enemy gets nothing");
        // A second hero with less, and Resistance to All below the Prayermaster, add nothing.
        let mut second = of(Side::Attacker, None, (4, 6));
        second.leadership = 1;
        second.prayermaster = 1;
        second.abilities.push("resist_all".into());
        second.special_level = 1;
        b.add_summoned(Side::Attacker, second, (4, 6));
        assert_eq!(b.units[0].melee, plain.melee + 3);
        assert_eq!(b.units[0].resistance, plain.resistance + 2);
        // When the hero falls the bonus goes at the next refresh.
        b.units[hero].status = Status::Dead;
        b.refresh_fields();
        assert_eq!(b.units[0].melee, plain.melee + 1);
    }

    #[test]
    fn a_charmed_heros_resistance_passes_every_check() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        b.units[0].resistance = 2;
        b.units[1].resistance = 2;
        b.units[1].resistance += crate::hero::CHARMED_RESISTANCE;
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
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        let before = b.units[0].clone();
        b.raise_field(Side::Attacker, Field::HighPrayer, spell());
        for _ in 0..3 {
            b.refresh_fields();
        }
        let u = &b.units[0];
        assert_eq!(
            (u.melee, u.defense, u.resistance, u.to_hit, u.to_block),
            (
                before.melee + 2,
                before.defense + 2,
                before.resistance + 3,
                1,
                1
            )
        );
        assert_eq!(b.units[1].melee, before.melee, "the enemy gets nothing");
        // Prayer is the lesser one and High Prayer wins when both are up.
        b.raise_field(Side::Attacker, Field::Prayer, spell());
        assert_eq!(b.units[0].resistance, before.resistance + 3);
        b.fields[Side::Attacker.index()].clear();
        b.refresh_fields();
        let u = &b.units[0];
        assert_eq!(
            (u.melee, u.defense, u.resistance, u.to_hit, u.to_block),
            (before.melee, before.defense, before.resistance, 0, 0)
        );
    }

    #[test]
    fn black_prayer_curses_the_enemy_and_warp_reality_every_unit_but_chaos() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, Some(Realm::Chaos), (17, 5)),
        );
        let enemy = b.units[1].clone();
        b.raise_field(Side::Attacker, Field::BlackPrayer, spell());
        let e = &b.units[1];
        assert_eq!(
            (e.melee, e.ranged, e.defense, e.resistance),
            (
                enemy.melee - 1,
                enemy.ranged - 1,
                enemy.defense - 1,
                enemy.resistance - 2
            )
        );
        assert_eq!(b.units[0].melee, 3, "its own side is untouched");
        b.raise_field(Side::Defender, Field::WarpReality, spell());
        assert_eq!(b.units[0].to_hit, -2);
        assert_eq!(b.units[1].to_hit, 0, "chaos creatures are not warped");
    }

    #[test]
    fn a_loss_never_takes_a_stat_below_nothing_and_comes_back_whole() {
        let mut weak = of(Side::Defender, None, (17, 5));
        weak.melee = 0;
        let mut b = pair(of(Side::Attacker, None, (3, 5)), weak);
        b.raise_field(Side::Attacker, Field::BlackPrayer, spell());
        assert_eq!(b.units[1].melee, 0);
        b.fields[0].clear();
        b.refresh_fields();
        assert_eq!(b.units[1].melee, 0, "not -1 + 1");
    }

    #[test]
    fn metal_fires_arms_ordinary_units_only() {
        let mut ordinary = of(Side::Attacker, None, (3, 5));
        ordinary.ranged_kind = RangedKind::Missile;
        let mut fantastic = of(Side::Attacker, Some(Realm::Nature), (4, 5));
        fantastic.ranged_kind = RangedKind::Missile;
        let mut blade = of(Side::Attacker, None, (5, 5));
        blade.enchantments.push("flame_blade".into());
        let mut b = Battle::new(
            A,
            D,
            BattleMap::open(),
            vec![
                ordinary,
                fantastic,
                blade,
                of(Side::Defender, None, (17, 5)),
            ],
            1,
        );
        b.raise_field(Side::Attacker, Field::MetalFires, spell());
        assert_eq!((b.units[0].melee, b.units[0].ranged), (4, 3));
        assert!(b.units[0].magical_weapon);
        assert_eq!(b.units[1].melee, 3, "fantastic creatures already burn");
        assert_eq!(b.units[2].melee, 3, "no double with a flame blade");
    }

    fn end_turns(b: &mut Battle, n: usize) -> Vec<BattleEvent> {
        let mut all = Vec::new();
        for _ in 0..n {
            all.extend(b.apply(BattleCommand::EndTurn).expect("apply succeeds"));
        }
        all
    }

    #[test]
    fn entangle_costs_a_move_and_terror_can_freeze_the_enemy() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        b.raise_field(Side::Attacker, Field::Entangle, spell());
        let moves = b.units[1].moves;
        end_turns(&mut b, 1);
        assert_eq!(b.active, Side::Defender);
        assert_eq!(b.units[1].moves_left, moves - 2, "one whole move less");
        // Fliers and the non-corporeal are not held.
        for ability in ["flying", "non_corporeal"] {
            let mut free = of(Side::Defender, None, (17, 5));
            if ability == "flying" {
                free.flies = true;
            } else {
                free.abilities.push(ability.into());
            }
            let mut b = pair(of(Side::Attacker, None, (3, 5)), free);
            b.raise_field(Side::Attacker, Field::Entangle, spell());
            end_turns(&mut b, 1);
            assert_eq!(b.units[1].moves_left, b.units[1].moves, "{ability}");
        }
        // Terror: a unit with no resistance never saves.
        let mut soft = of(Side::Defender, None, (17, 5));
        soft.resistance = 0;
        let mut b = pair(of(Side::Attacker, None, (3, 5)), soft);
        b.raise_field(Side::Attacker, Field::Terror, spell());
        let events = end_turns(&mut b, 1);
        assert!(events.contains(&BattleEvent::Terrified { unit: 1 }) || b.units[1].attacked);
        let mut frozen = 0;
        for seed in 0..40 {
            let mut soft = of(Side::Defender, None, (17, 5));
            soft.resistance = 0;
            let mut b = duel(of(Side::Attacker, None, (3, 5)), soft, seed);
            b.raise_field(Side::Attacker, Field::Terror, spell());
            end_turns(&mut b, 1);
            if b.units[1].attacked && b.units[1].moves_left == 0 {
                frozen += 1;
            }
        }
        assert!(frozen > 30, "{frozen} of 40 froze");
        // Death immunity is proof against it.
        for seed in 0..20 {
            let mut deathless = of(Side::Defender, None, (17, 5));
            deathless.resistance = 0;
            deathless.abilities.push("death_immunity".into());
            let mut b = duel(of(Side::Attacker, None, (3, 5)), deathless, seed);
            b.raise_field(Side::Attacker, Field::Terror, spell());
            end_turns(&mut b, 1);
            assert!(!b.units[1].attacked, "seed {seed}");
        }
    }

    /// A lasting spell put on a unit the way the casting does it.
    fn put_on(b: &mut Battle, unit: usize, id: &str, mods: &[Modifier], grants: &[&str]) {
        let grants: Vec<String> = grants.iter().map(|g| g.to_string()).collect();
        b.enchant_unit(unit, spell(), id, A, mods, &grants);
    }

    #[test]
    fn black_sleep_and_web_hold_a_unit_and_ground_a_flyer() {
        let still = Modifier::new(Stat::Moves, ModOp::Set, 0, Source::Other);
        let mut flyer = of(Side::Defender, None, (17, 5));
        flyer.flies = true;
        flyer.abilities.push("flying".into());
        let mut b = pair(of(Side::Attacker, None, (3, 5)), flyer);
        put_on(&mut b, 1, "web", &[still], &["webbed"]);
        let u = &b.units[1];
        assert!(!u.flies && u.moves == 0 && u.moves_left == 0);
        assert!(!u.attacked, "a webbed unit can still fight");
        end_turns(&mut b, 1);
        assert_eq!(b.units[1].moves_left, 0);
        assert!(!b.units[1].attacked);

        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        put_on(&mut b, 1, "black_sleep", &[still], &["asleep"]);
        assert!(b.units[1].attacked);
        end_turns(&mut b, 1);
        assert!(b.units[1].attacked && b.units[1].moves_left == 0);
        assert!(b.legal_commands().iter().all(|c| !matches!(
            c,
            BattleCommand::Move { unit: 1, .. } | BattleCommand::Attack { unit: 1, .. }
        )));
    }

    #[test]
    fn a_bound_unit_fights_for_the_caster_and_goes_home_when_the_battle_ends() {
        let mut b = Battle::new(
            A,
            D,
            BattleMap::open(),
            vec![
                spearman(Side::Attacker, (3, 5)),
                spearman(Side::Defender, (17, 5)),
                spearman(Side::Defender, (17, 7)),
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
        let mut b = Battle::new(
            A,
            D,
            BattleMap::open(),
            vec![
                spearman(Side::Attacker, (3, 5)),
                spearman(Side::Defender, (17, 5)),
                spearman(Side::Defender, (17, 7)),
            ],
            1,
        );
        b.units[1].enchantments.push("creature_binding".into());
        b.change_sides(1, Side::Attacker);
        b.units[2].status = Status::Dead;
        b.check_winner();
        let u = &b.units[1];
        assert_eq!((u.status, u.side), (Status::Dead, Side::Defender));
        assert_eq!(u.fate(), Fate::Gone);
    }

    #[test]
    fn confusion_picks_one_of_four_acts_and_can_lend_the_unit_to_the_enemy() {
        let (mut idle, mut wandered, mut seized, mut free) = (0, 0, 0, 0);
        for seed in 0..200 {
            let mut b = duel(
                of(Side::Attacker, None, (3, 5)),
                of(Side::Defender, None, (17, 5)),
                seed,
            );
            put_on(&mut b, 1, "confusion", &[], &["confused"]);
            let events = end_turns(&mut b, 1);
            let act = events.iter().find_map(|e| match e {
                BattleEvent::Confused { unit: 1, act } => Some(*act),
                _ => None,
            });
            match act {
                Some(Confusion::Idle) => idle += 1,
                Some(Confusion::Wandered) => {
                    wandered += 1;
                    assert_ne!(b.units[1].cell, (17, 5));
                }
                Some(Confusion::Seized) => {
                    seized += 1;
                    assert!(b.units[1].attacked && b.units[1].seized);
                    // The enemy commands it on their turn, and it goes home after.
                    end_turns(&mut b, 1);
                    assert_eq!(b.units[1].side, Side::Attacker);
                    assert!(b.units[1].moves_left > 0 && !b.units[1].attacked);
                    end_turns(&mut b, 1);
                    assert_eq!(b.units[1].side, Side::Defender);
                }
                None => free += 1,
            }
        }
        for n in [idle, wandered, seized, free] {
            assert!((25..=75).contains(&n), "{idle} {wandered} {seized} {free}");
        }
    }

    #[test]
    fn wrack_and_call_lightning_hurt_the_casters_enemies_at_the_start_of_each_turn() {
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        b.units[1].resistance = 0;
        b.units[1].figures_start = 2;
        b.units[1].figures = 2;
        b.raise_field(Side::Attacker, Field::Wrack, spell());
        let mut hurt = 0;
        end_turns(&mut b, 2);
        assert_eq!(b.turn, 2);
        hurt += b.units[1].max_hit_points() - b.units[1].hit_points();
        assert!(hurt >= 1, "a figure with no resistance feels it");
        assert_eq!(b.units[0].hit_points(), b.units[0].max_hit_points());
        // Wraith Form is no shelter from it.
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        b.units[1].resistance = 0;
        b.units[1].figures_start = 4;
        b.units[1].figures = 4;
        b.units[1].enchantments.push("wraith_form".into());
        b.raise_field(Side::Attacker, Field::Wrack, spell());
        end_turns(&mut b, 4);
        assert!(b.units[1].hit_points() < b.units[1].max_hit_points());
        // Call Lightning: bolts of strength eight on the enemy only.
        let mut b = pair(
            of(Side::Attacker, None, (3, 5)),
            of(Side::Defender, None, (17, 5)),
        );
        b.units[1].defense = 0;
        b.units[1].hits = 100;
        b.raise_field(Side::Attacker, Field::CallLightning, spell());
        let events = end_turns(&mut b, 2);
        let bolts = events
            .iter()
            .filter(|e| matches!(e, BattleEvent::SpellDamaged { unit: 1, .. }))
            .count();
        assert!(bolts <= 5, "{bolts}");
        assert_eq!(b.units[0].hit_points(), b.units[0].max_hit_points());
    }

    #[test]
    fn mana_leak_drains_the_enemy_wizard_and_its_spell_shots() -> TestResult {
        let mut shooter = of(Side::Defender, None, (17, 5));
        shooter.ranged_kind = RangedKind::Magic;
        shooter.ammo = 3;
        let mut b = pair(of(Side::Attacker, None, (3, 5)), shooter);
        b.set_caster(Side::Defender, crate::battle::BattleCaster::new(20, 40, 10));
        b.raise_field(Side::Attacker, Field::ManaLeak, spell());
        end_turns(&mut b, 2);
        assert_eq!(
            b.caster(Side::Defender).ok_or("caster is missing")?.mana,
            40 - MANA_LEAK_DRAIN
        );
        assert_eq!(b.units[1].ammo, 2);
        Ok(())
    }

    #[test]
    fn blur_takes_one_off_the_chance_to_hit_unless_the_attacker_sees_through_illusions() {
        let mut b = pair(
            of(Side::Attacker, None, (5, 5)),
            of(Side::Defender, None, (9, 5)),
        );
        assert_eq!(b.blur_penalty(0, 1), 0);
        b.raise_field(Side::Defender, Field::Blur, spell());
        assert_eq!(b.blur_penalty(0, 1), 1, "whatever the distance");
        assert_eq!(
            b.blur_penalty(1, 0),
            0,
            "only the blurred side's units are harder to hit"
        );
        b.units[0].abilities.push("illusion_immunity".into());
        assert_eq!(b.blur_penalty(0, 1), 0);
    }

    #[test]
    fn an_invisible_creature_is_harder_to_hit_unless_the_attacker_sees_through_illusions() {
        let mut ghost = of(Side::Defender, None, (6, 5));
        ghost.abilities.push("invisibility".into());
        let mut b = pair(of(Side::Attacker, None, (5, 5)), ghost);
        assert_eq!(b.invisibility_penalty(0, 1), 1, "adjacent");
        assert_eq!(b.invisibility_penalty(1, 0), 0);
        b.units[0].cell = (1, 5);
        assert_eq!(b.invisibility_penalty(0, 1), 5, "one for each square");
        b.units[0].cell = (5, 5);
        b.units[0].abilities.push("illusion_immunity".into());
        assert_eq!(b.invisibility_penalty(0, 1), 0);
        // Mass Invisibility does the same for the caster's whole side.
        let mut b = pair(
            of(Side::Attacker, None, (5, 5)),
            of(Side::Defender, None, (6, 5)),
        );
        b.raise_field(Side::Defender, Field::MassInvisibility, spell());
        assert_eq!(b.invisibility_penalty(0, 1), 1);
    }

    #[test]
    fn figures_that_fail_their_resistance_are_destroyed() -> TestResult {
        let mut soft = of(Side::Defender, None, (6, 5));
        soft.resistance = 0;
        soft.figures_start = 6;
        soft.figures = 6;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), soft);
        let dmg = b
            .spell_slay(1, Realm::Death, -9, &[], None)
            .ok_or("spell_slay is missing")?;
        assert_eq!(dmg.kills, 6, "no chance against a penalty of nine");
        assert!(!b.units[1].is_active());
        // A unit with the right immunity takes no harm; one with enough resistance resists.
        let mut tough = of(Side::Defender, None, (6, 5));
        tough.resistance = 30;
        tough.abilities.push("death_immunity".into());
        let mut b = pair(of(Side::Attacker, None, (5, 5)), tough);
        assert!(b
            .spell_slay(1, Realm::Death, -2, &["death_immunity".into()], None)
            .is_none());
        assert_eq!(
            b.spell_slay(1, Realm::Death, -2, &[], None)
                .ok_or("spell_slay is missing")?
                .kills,
            0
        );
        // Disintegrate: resistance under ten and the whole unit is gone.
        let mut frail = of(Side::Defender, None, (6, 5));
        frail.resistance = 9;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), frail);
        b.spell_slay(1, Realm::Chaos, 0, &[], Some(10))
            .ok_or("spell_slay is missing")?;
        assert!(!b.units[1].is_active());
        let mut hardy = of(Side::Defender, None, (6, 5));
        hardy.resistance = 10;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), hardy);
        b.spell_slay(1, Realm::Chaos, 0, &[], Some(10))
            .ok_or("spell_slay is missing")?;
        assert!(b.units[1].is_active());
        Ok(())
    }
}

#[test]
fn mithril_and_adamantium_weapons_add_to_the_attack_defence_and_to_hit() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let plain = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    for (quality, extra, plus) in [(1, 0, 1), (2, 1, 1), (3, 2, 1)] {
        let _ = g
            .world
            .insert_one(unit, crate::unit::WeaponQuality(quality));
        let bu = g
            .battle_unit_from(unit, Side::Attacker, 0)
            .ok_or("battle_unit_from is missing")?;
        assert_eq!(bu.melee, plain.melee + extra, "melee, quality {quality}");
        assert_eq!(
            bu.defense,
            plain.defense + extra,
            "defence, quality {quality}"
        );
        assert_eq!(bu.to_hit + bu.weapon_plus(), plain.to_hit + plus);
    }
    Ok(())
}

#[test]
fn a_heros_items_add_to_its_attacks_defence_and_abilities_in_battle() -> TestResult {
    use crate::item::{Item, ItemBonus};
    use crate::rules::ItemKind;
    let mut g = game();
    // Only a hero carries items: the game's first hero, the Dwarf.
    let walker = g
        .content
        .rules
        .units
        .id_of("dwarf")
        .ok_or("dwarf is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let plain = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    let item = |kind, bonus: ItemBonus, powers: &[&str]| Item {
        name: "Test".into(),
        kind,
        cost: 100,
        icon: 0,
        bonus,
        powers: powers.iter().map(|p| p.to_string()).collect(),
        charges: None,
        prefab: None,
        made: None,
    };
    let sword = item(
        ItemKind::Sword,
        ItemBonus {
            attack: 2,
            to_hit: 1,
            ..ItemBonus::default()
        },
        &["vampiric", "flaming"],
    );
    let shield = item(
        ItemKind::Shield,
        ItemBonus {
            defense: 2,
            resistance: 1,
            ..ItemBonus::default()
        },
        &[],
    );
    g.arm_hero(unit, [Some(sword), Some(shield), None]);
    let armed = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    assert_eq!(armed.melee, plain.melee + 2 + 3, "attack and Flaming");
    assert_eq!(armed.melee_plus, 1);
    assert_eq!(armed.ranged_plus, 0);
    assert_eq!(armed.defense, plain.defense + 2);
    assert_eq!(armed.resistance, plain.resistance + 1);
    assert!(armed.has("life_steal"));
    assert!(armed.has("large_shield"));
    assert!(!plain.has("life_steal"));
    // The plus to hit counts for the melee blow, not for a shot.
    assert_eq!(
        armed.melee_combatant().to_hit,
        plain.melee_combatant().to_hit + 1
    );
    Ok(())
}

#[test]
fn a_hero_with_casting_skill_has_mana_that_pays_for_its_spell_shots() -> TestResult {
    use crate::hero::Hero;
    use crate::item::{Item, ItemBonus};
    use crate::rules::ItemKind;
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let plain = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    assert_eq!((plain.mana, plain.hero_caster), (0, false));
    // No casting skill, no mana, whatever the items bring.
    let unit = hero_unit(&mut g, &[], 0);
    assert_eq!(
        g.battle_unit_from(unit, Side::Attacker, 0)
            .ok_or("battle_unit_from is missing")?
            .mana,
        0
    );
    // Skill 2 at the first level: (2 + 1) * 1 * 5 / 2.
    g.world.get::<&mut Hero>(unit)?.casting_skill = 2;
    let bare = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    assert_eq!((bare.mana, bare.hero_caster), (7, true));
    // An item's spell skill adds to it.
    let staff = Item {
        name: "Staff".into(),
        kind: ItemKind::Staff,
        cost: 100,
        icon: 0,
        bonus: ItemBonus {
            spell_skill: 5,
            ..ItemBonus::default()
        },
        powers: Vec::new(),
        charges: None,
        prefab: None,
        made: None,
    };
    g.arm_hero(unit, [Some(staff), None, None]);
    let mut armed = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    assert_eq!(armed.mana, 12);
    // A magic shot costs the hero three mana, and its shots follow its mana.
    armed.ranged_kind = RangedKind::Magic;
    armed.ammo = armed.mana / SHOT_MANA;
    assert_eq!(armed.ammo, 4);
    armed.spend_shot();
    assert_eq!((armed.mana, armed.ammo), (9, 3));
    armed.leak_mana(5);
    assert_eq!((armed.mana, armed.ammo), (4, 1));
    Ok(())
}

/// A game with the game's real spells, for battle units that cast them.
fn casting_game() -> Game {
    let mut g = game();
    g.content.rules.spells = crate::game::testutil::shipped().rules.spells;
    g
}

/// A hero of player A that serves at (4, 4), made the way the game makes one: a hero type in the
/// rules (a walker that is a hero), the hero entity, and hiring it. Skill 0 and the spells it knows
/// are set after, as the rolled ones would be.
fn hero_unit(g: &mut Game, spells: &[&str], skill: u8) -> hecs::Entity {
    use crate::hero::{Hero, HeroAbilities, HeroState};
    g.make_hero(
        Hero {
            kind: 0,
            name: "Ann".into(),
            abilities: HeroAbilities::default(),
            casting_skill: skill,
            spells: spells.iter().map(|s| s.to_string()).collect(),
        },
        HeroState::Free,
        0,
    );
    g.spawn_hero(A, at(4, 4), 0).expect("spawn_hero succeeds")
}

fn begin(g: &mut Game, attacker: hecs::Entity) {
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .expect("id_of succeeds");
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    g.apply(Command::BeginBattle {
        attackers: vec![attacker],
        defenders: vec![d1],
    });
    assert!(g.in_battle());
}

#[test]
fn call_chaos_deals_every_enemy_unit_one_of_its_eight_outcomes_and_says_which() -> TestResult {
    let mut g = casting_game();
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 2000;
    g.init_wizard(A, &[("chaos", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let call = g
        .content
        .rules
        .spells
        .id_of("call_chaos")
        .ok_or("call_chaos is not in the content")?;
    {
        let mut m = g.wizard_mut(A).ok_or("wizard_mut is missing")?;
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&call);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != call);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(call);
        m.get_mut::<crate::wizard::sheet::Skill>().left = 500;
    }
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let ours = g.spawn_unit(A, at(4, 4), walker);
    let theirs: Vec<_> = (0..6).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders: theirs,
    });
    // The wizard has the skill and the mana for a spell that dear.
    g.battle
        .as_mut()
        .ok_or("as_mut is missing")?
        .battle
        .set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
    let events = g
        .battle_command(BattleCommand::Cast {
            spell: call,
            aim: SpellAim::None,
            mana: 0,
        })
        .map_err(|e| format!("{e:?}"))?;
    let battle_events: Vec<&BattleEvent> = events
        .iter()
        .filter_map(|e| match e {
            Event::Battle(b) => Some(b),
            _ => None,
        })
        .collect();
    let called: Vec<(usize, u8)> = battle_events
        .iter()
        .filter_map(|e| match e {
            BattleEvent::ChaosCalled { unit, outcome } => Some((*unit, *outcome)),
            _ => None,
        })
        .collect();
    // One outcome for each enemy unit and for no one else, each of the eight there are.
    let enemies: Vec<usize> = g
        .battle()
        .ok_or("battle is missing")?
        .units
        .iter()
        .filter(|u| u.side == Side::Defender)
        .map(|u| u.id)
        .collect();
    assert_eq!(called.iter().map(|&(u, _)| u).collect::<Vec<_>>(), enemies);
    assert!(called.iter().all(|&(_, outcome)| outcome < 8));
    // What follows is what the outcome says: a bolt hurts or is shrugged off, healing heals.
    for &(unit, outcome) in &called {
        let touched =
            |pick: fn(&BattleEvent, usize) -> bool| battle_events.iter().any(|e| pick(e, unit));
        match outcome {
            4..=6 => assert!(touched(|e, u| matches!(
                e,
                BattleEvent::SpellDamaged { unit, .. } | BattleEvent::SpellImmune { unit } if *unit == u
            ))),
            1 => assert!(touched(
                |e, u| matches!(e, BattleEvent::SpellHealed { unit, .. } if *unit == u)
            )),
            0 => assert!(!touched(|e, u| matches!(
                e,
                BattleEvent::SpellDamaged { unit, .. }
                    | BattleEvent::SpellHealed { unit, .. }
                    | BattleEvent::SpellImmune { unit } if *unit == u
            ))),
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn chaos_channels_mutates_an_own_normal_unit_in_a_battle_once() -> TestResult {
    let mut g = casting_game();
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 2000;
    g.init_wizard(A, &[("chaos", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let spell = g
        .content
        .rules
        .spells
        .id_of("chaos_channels")
        .ok_or("chaos_channels is not in the content")?;
    {
        let mut m = g.wizard_mut(A).ok_or("wizard_mut is missing")?;
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&spell);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != spell);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(spell);
        m.get_mut::<crate::wizard::sheet::Skill>().left = 500;
    }
    // A normal unit, as the game has them: the barbarians' spearmen.
    let soldier = g
        .content
        .rules
        .units
        .id_of("barbarian_spearmen")
        .ok_or("barbarian_spearmen is not in the content")?;
    let ours = g.spawn_unit(A, at(4, 4), soldier);
    let theirs = g.spawn_unit(D, at(5, 4), soldier);
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders: vec![theirs],
    });
    g.battle
        .as_mut()
        .ok_or("as_mut is missing")?
        .battle
        .set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
    let enemy = g
        .battle()
        .ok_or("battle is missing")?
        .units
        .iter()
        .find(|u| u.side == Side::Defender)
        .ok_or("find is missing")?
        .id;
    let mine = g
        .battle()
        .ok_or("battle is missing")?
        .units
        .iter()
        .find(|u| u.side == Side::Attacker)
        .ok_or("find is missing")?
        .id;
    let cast = |g: &mut Game, unit| {
        g.battle_command(BattleCommand::Cast {
            spell,
            aim: SpellAim::Unit(unit),
            mana: 0,
        })
    };
    // Only the wizard's own units can be channelled.
    assert!(cast(&mut g, enemy).is_err());
    let events = cast(&mut g, mine).map_err(|e| format!("{e:?}"))?;
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Battle(BattleEvent::SpellEnchanted { unit, .. }) if *unit == mine
    )));
    // The overland unit is mutated for good and the unit in the battle is a creature of chaos.
    assert!(g.units().unit_is_mutated(ours));
    assert_eq!(
        g.battle()
            .ok_or("battle is missing")?
            .unit(mine)
            .ok_or("unit is missing")?
            .realm,
        Some(Realm::Chaos)
    );
    // A unit takes the change once.
    assert!(cast(&mut g, mine).is_err());
    Ok(())
}

#[test]
fn a_hero_casts_an_innate_spell_with_its_own_mana_and_ends_its_turn() -> TestResult {
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &["fire_bolt", "healing"], 2);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .ok_or("fire_bolt is not in the content")?;
    let mend = g
        .content
        .rules
        .spells
        .id_of("healing")
        .ok_or("healing is not in the content")?;
    let before = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .mana;
    assert_eq!(before, 7);
    // Both spells are on the list; the hero has the mana for the Fire Bolt (5) but not for
    // Healing (15).
    let list = g.battle_unit_spells(0);
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|s| s.source == SpellSource::Mana));
    assert_eq!(
        list.iter().map(|s| s.castable).collect::<Vec<_>>(),
        [true, false]
    );
    let refused = g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: mend,
        aim: SpellAim::Unit(0),
        mana: 0,
    });
    assert_eq!(refused, Err(BattleError::Spell(SpellError::TooExpensive)));
    // With mana for more, it casts, pays and cannot cast again.
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[0].mana = 25;
    let hp = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .hit_points();
    let events = g
        .battle_command(BattleCommand::UnitCast {
            unit: 0,
            spell: zap,
            aim: SpellAim::Unit(1),
            mana: 0,
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Battle(BattleEvent::UnitSpellCast {
            unit: 0,
            mana: 5,
            ..
        })
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Battle(
            BattleEvent::SpellDamaged { unit: 1, .. } | BattleEvent::SpellImmune { unit: 1 }
        )
    )));
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(b.unit(0).ok_or("unit is missing")?.mana, 20);
    assert!(b.unit(1).ok_or("unit is missing")?.hit_points() <= hp);
    assert_eq!(b.unit(0).ok_or("unit is missing")?.moves_left, 0);
    assert!(
        b.caster(Side::Attacker).is_none_or(|c| !c.cast_this_turn),
        "the wizard did not cast"
    );
    let again = g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: zap,
        aim: SpellAim::Unit(1),
        mana: 0,
    });
    assert_eq!(again, Err(BattleError::NoMovesLeft));
    Ok(())
}

#[test]
fn an_items_spell_costs_a_charge_and_the_charges_stay_on_the_item() -> TestResult {
    use crate::item::{Item, ItemBonus};
    use crate::rules::ItemKind;
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = |slot_name: &str| Item {
        name: slot_name.into(),
        kind: ItemKind::Wand,
        cost: 100,
        icon: 0,
        bonus: ItemBonus {
            spell_save: 2,
            ..ItemBonus::default()
        },
        powers: Vec::new(),
        charges: Some(("fire_bolt".into(), 2)),
        prefab: None,
        made: None,
    };
    // The spell is in the second slot: any slot works, not only the first.
    g.arm_hero(hero, [None, Some(wand("Wand")), None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .ok_or("fire_bolt is not in the content")?;
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.mana,
        0,
        "no casting skill, no mana"
    );
    assert_eq!(b.unit(0).ok_or("unit is missing")?.casting.spell_save, 2);
    let list = g.battle_unit_spells(0);
    assert_eq!(list.len(), 1);
    let wand_entity = g
        .items()
        .hero_item_entity(hero, 1)
        .ok_or("hero_item_entity is missing")?;
    assert_eq!(list[0].source, SpellSource::Item(wand_entity));
    assert!(list[0].castable);
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: zap,
        aim: SpellAim::Unit(1),
        mana: 0,
    })
    .map_err(|e| format!("{e:?}"))?;
    let left = &g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .casting
        .item_charges;
    assert_eq!(left.len(), 1);
    assert_eq!(
        (left[0].item, left[0].spell, left[0].charges),
        (wand_entity, zap, 1)
    );
    assert_eq!(left[0].serial, g.serial_of(wand_entity));
    // The battle ends with the defender's flight; one charge is left on the item.
    g.battle_command(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    g.battle_command(BattleCommand::Flee)
        .map_err(|e| format!("{e:?}"))?;
    assert!(!g.in_battle());
    let items = g.items().hero_items(hero);
    assert_eq!(
        items.0[1].as_ref().ok_or("as_ref is missing")?.charges,
        Some(("fire_bolt".into(), 1))
    );
    Ok(())
}

#[test]
fn an_ability_spell_is_cast_once_a_battle_and_a_ward_or_a_miss_of_aim_stops_it() -> TestResult {
    let mut g = casting_game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut().filter(|d| d.id == "high_men_cavalry") {
        d.abilities.push("casts_healing".into());
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(defs);
    begin(&mut g, unit);
    let healing = g
        .content
        .rules
        .spells
        .id_of("healing")
        .ok_or("healing is not in the content")?;
    // It is cast on a friend only: the enemy is a bad target.
    let bad = g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: healing,
        aim: SpellAim::Unit(1),
        mana: 0,
    });
    assert_eq!(
        bad,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NotYourUnit
        )))
    );
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: healing,
        aim: SpellAim::Unit(0),
        mana: 0,
    })
    .map_err(|e| format!("{e:?}"))?;
    assert!(g.battle_unit_spells(0).is_empty() || !g.battle_unit_spells(0)[0].castable);
    assert!(g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .casting
        .abilities
        .is_empty());
    Ok(())
}

#[test]
fn the_twelve_units_with_a_casting_ability_cast_its_spell_once_for_no_mana() -> TestResult {
    let shipped = crate::game::testutil::shipped();
    // Every unit of the shipped rules with one of the four abilities, and the spell it casts.
    let casters: Vec<(&str, &str)> = [
        ("casts_fireball", "fireball"),
        ("casts_healing", "healing"),
        ("casts_doom_bolt", "doom_bolt"),
        ("casts_web", "web"),
    ]
    .iter()
    .flat_map(|&(ability, spell)| {
        shipped
            .rules
            .units
            .defs()
            .iter()
            .filter(move |d| d.abilities.iter().any(|a| a == ability))
            .map(move |d| (d.id.as_str(), spell))
    })
    .collect();
    assert_eq!(casters.len(), 12);
    for (unit_id, spell_id) in casters {
        let mut g = casting_game();
        g.content.rules.units = shipped.rules.units.clone();
        let id = g
            .content
            .rules
            .units
            .id_of(unit_id)
            .ok_or("the unit is not in the content")?;
        let caster = g.spawn_unit(A, at(4, 4), id);
        begin(&mut g, caster);
        let spell = g
            .content
            .rules
            .spells
            .id_of(spell_id)
            .ok_or("the spell is not in the content")?;
        let listed = g.battle_unit_spells(0);
        let own = listed
            .iter()
            .find(|s| s.spell == spell)
            .ok_or_else(|| format!("{unit_id} does not list {spell_id}"))?;
        assert_eq!(own.source, SpellSource::Ability, "{unit_id}");
        assert_eq!((own.cost, own.castable), (0, true), "{unit_id}");
        // It is a legal cast, aimed at a friend (Healing) or the enemy (the others).
        let legal = g.legal_unit_casts();
        let cast = legal
            .iter()
            .find(|(u, c)| *u == 0 && c.spell == spell)
            .map(|(_, c)| c);
        match (spell_id, cast) {
            // Nothing is hurt, so Healing has no target worth a cast.
            ("healing", None) => continue,
            (_, Some(cast)) => assert!(!cast.aims.is_empty(), "{unit_id}"),
            (_, None) => return Err(format!("{unit_id} cannot cast {spell_id}").into()),
        }
        let aim = SpellAim::Unit(if spell_id == "healing" { 0 } else { 1 });
        // The scripted unit makes the cast at the enemy (a lone, unhurt Healing has no point).
        let chosen = super::brain::unit_cast_choice(&g, g.battle().ok_or("no battle")?);
        assert!(
            spell_id == "healing"
                || matches!(chosen, Some(BattleCommand::UnitCast { spell: s, aim: SpellAim::Unit(1), .. }) if s == spell),
            "{unit_id}: {chosen:?}"
        );
        g.battle_command(BattleCommand::UnitCast {
            unit: 0,
            spell,
            aim,
            mana: 0,
        })
        .map_err(|e| format!("{unit_id}: {e:?}"))?;
        // Once a battle: the ability is spent.
        assert!(
            g.battle_unit_spells(0).iter().all(|s| s.spell != spell),
            "{unit_id} can cast {spell_id} twice"
        );
    }
    Ok(())
}

#[test]
fn a_spell_save_makes_the_resistance_checks_of_the_cast_harder() {
    let mut unit = BattleUnit::simple(Side::Defender, "t", 1, 1, 1, 0);
    unit.resistance = 6;
    let mut b = Battle::new(
        A,
        D,
        BattleMap::open(),
        vec![
            BattleUnit::simple(Side::Attacker, "a", 1, 1, 1, 0).at((1, 1)),
            unit.at((5, 5)),
        ],
        3,
    );
    let resisted = |b: &mut Battle| (0..200).filter(|_| !b.resist_fails(1, None, 0)).count();
    assert!(resisted(&mut b) > 50);
    b.save_penalty = 10;
    assert_eq!(resisted(&mut b), 0);
}

/// A game whose hero (casting skill 4, so mana for several casts) has the real spell `id`. The
/// battle is on; the enemy is battle unit 1.
fn hook_game(id: &str) -> Game {
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &[id], 4);
    begin(&mut g, hero);
    g.battle.as_mut().expect("as_mut succeeds").battle.units[0].mana = 1000;
    g
}

fn cast_unit(
    g: &mut Game,
    spell: &str,
    aim: SpellAim,
    mana: u32,
) -> Result<Vec<Event>, BattleError> {
    let spell = g.content.rules.spells.id_of(spell).expect("id_of succeeds");
    // The unit may cast again in the next test step.
    g.battle.as_mut().expect("as_mut succeeds").battle.units[0].moves_left = 4;
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell,
        aim,
        mana,
    })
}

#[test]
fn earth_to_mud_makes_open_ground_in_five_by_five_cost_a_walker_all_its_moves() -> TestResult {
    let mut g = hook_game("earth_to_mud");
    // Any playable square may be aimed at.
    let centre = (10, 11);
    let events = cast_unit(&mut g, "earth_to_mud", SpellAim::Tile(centre), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::MudMade { cell }) if *cell == centre)));
    let map = &g.battle().ok_or("battle is missing")?.map;
    for (dx, dy, mud) in [
        (0, 0, true),
        (2, 2, true),
        (-2, -2, true),
        (3, 0, false),
        (0, -3, false),
    ] {
        let at = (centre.0 + dx, centre.1 + dy);
        if map.get(at) == Some(Cell::Open) {
            assert_eq!(map.is_mud(at), mud, "{at:?}");
        }
    }
    // A walker pays all it has to enter; a flier or a bodiless unit does not notice.
    let none = |_: Pos| false;
    let walk = map.reachable_as((6, 11), 8, Mover::walker(), &none);
    assert_eq!(
        walk[&(7, 11)].0,
        OPEN_COST,
        "the square before the mud costs the usual"
    );
    let into = map.reachable_as((7, 11), 8, Mover::walker(), &none);
    assert!(into[&(8, 11)].0 >= 8, "{:?}", into[&(8, 11)]);
    let ghost = Mover {
        ignores_mud: true,
        ..Mover::walker()
    };
    assert_eq!(
        map.reachable_as((7, 11), 8, ghost, &none)[&(8, 11)].0,
        OPEN_COST
    );
    let flier = Mover {
        flying: true,
        through_walls: true,
        ..Mover::default()
    };
    assert_eq!(
        map.reachable_as((7, 11), 8, flier, &none)[&(8, 11)].0,
        OPEN_COST
    );
    Ok(())
}

#[test]
fn disrupt_destroys_one_wall_section_and_only_where_a_wall_stands() -> TestResult {
    let mut g = hook_game("disrupt");
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(true);
    let walls = g.battle().ok_or("battle is missing")?.map.standing_walls();
    assert!(walls.len() > 2);
    let bad = cast_unit(&mut g, "disrupt", SpellAim::Tile((3, 3)), 0);
    assert_eq!(
        bad,
        Err(BattleError::Spell(SpellError::Target(TargetError::NoWall)))
    );
    let target = walls[1];
    let events =
        cast_unit(&mut g, "disrupt", SpellAim::Tile(target), 0).map_err(|e| format!("{e:?}"))?;
    assert!(events.iter().any(
        |e| matches!(e, Event::Battle(BattleEvent::WallDestroyed { cell }) if *cell == target)
    ));
    let map = &g.battle().ok_or("battle is missing")?.map;
    assert!(!map.has_wall(target));
    assert_eq!(map.standing_walls().len(), walls.len() - 1);
    // The broken section cannot be struck again.
    let again = cast_unit(&mut g, "disrupt", SpellAim::Tile(target), 0);
    assert_eq!(
        again,
        Err(BattleError::Spell(SpellError::Target(TargetError::NoWall)))
    );
    Ok(())
}

#[test]
fn cracks_call_swallows_a_walker_one_time_in_four_but_not_a_flier_and_breaks_a_wall() -> TestResult
{
    let mut g = hook_game("cracks_call");
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    // A flier is never touched; a walker falls in sooner or later.
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1].flies = true;
    for _ in 0..40 {
        cast_unit(&mut g, "cracks_call", SpellAim::Tile(enemy_cell), 0)
            .map_err(|e| format!("{e:?}"))?;
        assert!(g
            .battle()
            .ok_or("battle is missing")?
            .unit(1)
            .ok_or("unit is missing")?
            .is_active());
    }
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1].flies = false;
    let mut died = false;
    for _ in 0..60 {
        let events = cast_unit(&mut g, "cracks_call", SpellAim::Tile(enemy_cell), 0)
            .map_err(|e| format!("{e:?}"))?;
        if !g.in_battle()
            || events
                .iter()
                .any(|e| matches!(e, Event::Battle(BattleEvent::UnitDied { unit: 1 })))
        {
            died = true;
            break;
        }
    }
    assert!(died, "one time in four, sixty times");
    // A wall on the square comes down.
    let mut g = hook_game("cracks_call");
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(true);
    let wall = g.battle().ok_or("battle is missing")?.map.standing_walls()[0];
    let events =
        cast_unit(&mut g, "cracks_call", SpellAim::Tile(wall), 0).map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::WallDestroyed { .. }))));
    assert!(!g.battle().ok_or("battle is missing")?.map.has_wall(wall));
    Ok(())
}

#[test]
fn life_drain_takes_hit_points_that_a_hero_keeps_and_death_creatures_are_immune() -> TestResult {
    let mut g = hook_game("life_drain");
    {
        let b = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle;
        b.units[1].resistance = 0;
        b.units[0].front_damage = 3;
    }
    let before = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .hit_points();
    let hurt = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .hit_points();
    let events =
        cast_unit(&mut g, "life_drain", SpellAim::Unit(1), 10).map_err(|e| format!("{e:?}"))?;
    let drained = events.iter().find_map(|e| match e {
        Event::Battle(BattleEvent::LifeDrained { points, .. }) => Some(*points),
        _ => None,
    });
    let points = drained.expect("a resistance 0 unit loses life");
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(
        b.unit(1).ok_or("unit is missing")?.hit_points(),
        before - points
    );
    assert!(
        b.unit(0).ok_or("unit is missing")?.hit_points() > hurt,
        "the hero took the life"
    );
    // Death creatures cannot be aimed at.
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1].realm = Some(Realm::Death);
    let refused = cast_unit(&mut g, "life_drain", SpellAim::Unit(1), 10);
    assert_eq!(
        refused,
        Err(BattleError::Spell(SpellError::Target(TargetError::Deathly)))
    );
    Ok(())
}

#[test]
fn word_of_recall_takes_any_unit_to_the_circle_where_recall_hero_wants_a_hero() -> TestResult {
    let mut g = hook_game("word_of_recall");
    let no_circle = cast_unit(&mut g, "word_of_recall", SpellAim::Unit(0), 0);
    assert_eq!(
        no_circle,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NoCircle
        )))
    );
    g.wiz_mut(0)
        .put(Some(crate::wizard::fortress::SummonAt(at(1, 1))));
    let enemy = cast_unit(&mut g, "word_of_recall", SpellAim::Unit(1), 0);
    assert_eq!(
        enemy,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NotYourUnit
        )))
    );
    let events =
        cast_unit(&mut g, "word_of_recall", SpellAim::Unit(0), 0).map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::HeroRecalled { unit: 0 }))));
    // The only unit left the field, so the battle is over and the unit stands at the circle.
    assert!(!g.in_battle());
    let unit = g.units().hero_units(A)[0];
    assert_eq!(g.units().unit_position(unit), Some(at(1, 1)));
    Ok(())
}

#[test]
fn a_unit_with_creature_summoning_calls_three_demons_a_battle_one_a_turn() -> TestResult {
    let mut g = game();
    let mut defs = g.content.rules.units.defs().to_vec();
    for d in defs.iter_mut().filter(|d| d.id == "high_men_cavalry") {
        d.abilities.push("summon_demon".into());
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(defs);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let lord = g.spawn_unit(A, at(4, 4), walker);
    begin(&mut g, lord);
    assert!(g.can_summon_demon(0));
    // Not the other side's unit, not a unit without the ability.
    assert_eq!(
        g.battle_command(BattleCommand::SummonDemon { unit: 1 }),
        Err(BattleError::NotYours)
    );
    for turn in 0..3 {
        g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[0].moves_left = 2;
        let before = g.battle().ok_or("battle is missing")?.units.len();
        let events = g
            .battle_command(BattleCommand::SummonDemon { unit: 0 })
            .map_err(|e| format!("{e:?}"))?;
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Battle(BattleEvent::Summoned { .. }))));
        let b = g.battle().ok_or("battle is missing")?;
        assert_eq!(b.units.len(), before + 1, "turn {turn}");
        let demon = b.units.last().ok_or("last is missing")?;
        assert_eq!(
            (demon.side, demon.unit_id.as_str()),
            (Side::Attacker, "demon")
        );
        assert!(demon.moves_left > 0, "it arrives ready to act");
        assert_eq!(
            b.unit(0).ok_or("unit is missing")?.moves_left,
            0,
            "the summoner's turn is over"
        );
        assert_eq!(
            g.battle_command(BattleCommand::SummonDemon { unit: 0 }),
            Err(BattleError::NoMovesLeft)
        );
    }
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[0].moves_left = 2;
    assert_eq!(
        g.battle_command(BattleCommand::SummonDemon { unit: 0 }),
        Err(BattleError::Spell(SpellError::NotKnown)),
        "three is all"
    );
    Ok(())
}

// ---- city enchantments ---------------------------------------------------------

fn city_map() -> BattleMap {
    BattleMap::open().with_city(false)
}

/// Ten figures of one hit point and no defence: a Wall of Fire's attack leaves a mark on them.
fn crowd(side: Side, cell: Pos) -> BattleUnit {
    BattleUnit::simple(side, "crowd", 10, 1, 1, 0).at(cell)
}

#[test]
fn a_wall_of_fire_burns_walkers_that_step_into_the_city_but_not_fliers() -> TestResult {
    let far = crowd(Side::Defender, (19, 2));
    for (flies, burned) in [(false, true), (true, false)] {
        let mut walker = crowd(Side::Attacker, (10, 11));
        walker.flies = flies;
        let mut b = Battle::new(A, D, city_map(), vec![walker, far.clone()], 5);
        b.wall_of_fire = true;
        let events = b
            .apply(BattleCommand::Move {
                unit: 0,
                to: (12, 11),
            })
            .map_err(|e| format!("{e:?}"))?;
        let hurt = events
            .iter()
            .any(|e| matches!(e, BattleEvent::SpellDamaged { unit: 0, .. }));
        assert_eq!(hurt, burned, "flies: {flies}");
        assert_eq!(b.unit(0).ok_or("unit is missing")?.figures < 10, burned);
    }
    // Walking about outside, or inside once there, costs nothing; so does a city without it.
    let mut b = Battle::new(
        A,
        D,
        city_map(),
        vec![crowd(Side::Attacker, (13, 11)), far.clone()],
        5,
    );
    b.wall_of_fire = true;
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: (14, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::SpellDamaged { .. })));
    let mut b = Battle::new(
        A,
        D,
        city_map(),
        vec![crowd(Side::Attacker, (10, 11)), far],
        5,
    );
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: (12, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::SpellDamaged { .. })));
    Ok(())
}

#[test]
fn striking_into_a_city_behind_a_wall_of_fire_burns_the_attacker() -> TestResult {
    let mut b = Battle::new(
        A,
        D,
        city_map(),
        vec![
            crowd(Side::Attacker, (11, 11)),
            crowd(Side::Defender, (12, 11)),
        ],
        5,
    );
    b.wall_of_fire = true;
    let events = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(
        events.first(),
        Some(BattleEvent::SpellDamaged { unit: 0, .. })
    ));
    Ok(())
}

#[test]
fn a_wall_of_darkness_stops_shots_into_the_city_unless_the_shooter_sees_through() {
    let archer = |cell, illusion_immune: bool| {
        let mut u = BattleUnit::simple(Side::Attacker, "archer", 4, 1, 1, 0).at(cell);
        u.ranged = 4;
        u.ranged_kind = RangedKind::Missile;
        u.ammo = 3;
        if illusion_immune {
            u.abilities.push("illusion_immunity".into());
        }
        u
    };
    let inside = crowd(Side::Defender, (13, 11));
    let shoot = |from: Pos, immune: bool, dark: bool| {
        let mut b = Battle::new(
            A,
            D,
            city_map(),
            vec![archer(from, immune), inside.clone()],
            5,
        );
        b.wall_of_darkness = dark;
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
    };
    assert_eq!(
        shoot((8, 11), false, true),
        Err(BattleError::WallOfDarkness)
    );
    assert!(
        shoot((8, 11), true, true).is_ok(),
        "illusion immunity sees through"
    );
    assert!(shoot((8, 11), false, false).is_ok(), "no wall, no darkness");
    assert!(
        shoot((12, 10), false, true).is_ok(),
        "shooters inside the city are not stopped"
    );
}

#[test]
fn a_flying_fortress_shuts_the_city_to_walkers_and_its_walkers_in() {
    let map = city_map().with_flying_fortress();
    let mut walker_out = crowd(Side::Attacker, (10, 11));
    walker_out.moves = 20;
    let mut flier = walker_out.clone();
    flier.flies = true;
    let mut walker_in = crowd(Side::Defender, (13, 11));
    walker_in.moves = 20;
    let mut flier_in = walker_in.clone();
    flier_in.flies = true;
    let b = Battle::new(
        A,
        D,
        map.clone(),
        vec![walker_out, flier, walker_in, flier_in],
        5,
    );
    let area =
        |reach: &std::collections::HashMap<Pos, u32>| reach.keys().any(|&p| map::in_city_area(p));
    let ring =
        |reach: &std::collections::HashMap<Pos, u32>| reach.keys().any(|&p| map::in_city_ring(p));
    // A walker outside reaches the ring but not the city; a flier gets in.
    let (out, fly) = (b.reachable(0), b.reachable(1));
    assert!(ring(&out) && !area(&out));
    assert!(area(&fly));
    // A walker inside is kept in: it never reaches the ring; a flier inside gets out.
    let (inn, fly_in) = (b.reachable(2), b.reachable(3));
    assert!(area(&inn) && !ring(&inn));
    assert!(ring(&fly_in));
    assert!(map.is_flying_fortress());
}

/// Every command that could be written, for the units and squares of a battle.
fn every_command(b: &Battle) -> Vec<BattleCommand> {
    let mut all = vec![BattleCommand::EndTurn, BattleCommand::Flee];
    for unit in 0..b.units.len() + 1 {
        all.push(BattleCommand::Done { unit });
        for target in 0..b.units.len() + 1 {
            all.push(BattleCommand::Attack { unit, target });
            all.push(BattleCommand::Shoot { unit, target });
        }
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                all.push(BattleCommand::Move { unit, to: (x, y) });
                all.push(BattleCommand::AttackWall { unit, cell: (x, y) });
                all.push(BattleCommand::ShootWall { unit, cell: (x, y) });
            }
        }
    }
    all
}

/// The list of legal commands is exactly what `apply` accepts.
fn check_legal_list(b: &Battle) {
    let legal = b.legal_commands();
    for c in every_command(b) {
        let accepted = b.clone().apply(c.clone()).is_ok();
        assert_eq!(
            accepted,
            legal.contains(&c),
            "{c:?}: apply says {accepted}, the list says otherwise"
        );
    }
}

#[test]
fn the_legal_commands_are_exactly_what_apply_accepts() -> TestResult {
    let archer = |side, cell| {
        let mut u = BattleUnit::simple(side, "archer", 4, 1, 1, 0).at(cell);
        u.ranged = 4;
        u.ranged_kind = RangedKind::Missile;
        u.ammo = 2;
        u
    };
    // An open field: a spearman next to an enemy, an archer, a unit with no ammunition.
    let mut spent = archer(Side::Attacker, (4, 14));
    spent.ammo = 0;
    let units = vec![
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (4, 10)),
        archer(Side::Attacker, (3, 12)),
        archer(Side::Defender, (9, 12)),
        spent,
    ];
    let mut b = Battle::new(A, D, BattleMap::open(), units, 1);
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
fn the_legal_commands_cover_walls_crushers_and_darkness() -> TestResult {
    let mut b = siege(
        vec![
            crusher(Side::Attacker, (11, 11), true),
            crusher(Side::Attacker, (3, 3), false),
            guard((14, 11)),
            guard((12, 12)),
        ],
        true,
        3,
    );
    check_legal_list(&b);
    assert!(b.legal_commands().contains(&BattleCommand::AttackWall {
        unit: 0,
        cell: (12, 11)
    }));
    // A Wall of Darkness stops the shots from outside the city at a unit inside it.
    b.wall_of_darkness = true;
    check_legal_list(&b);
    // A won battle allows nothing.
    b.apply(BattleCommand::Flee).map_err(|e| format!("{e:?}"))?;
    assert!(b.legal_commands().is_empty());
    Ok(())
}

#[test]
fn an_undead_unit_fights_as_a_fantastic_death_creature_with_its_immunities() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let a2 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    assert!(g.make_undead(a1));
    g.apply(Command::BeginBattle {
        attackers: vec![a1, a2],
        defenders: vec![d1],
    });
    let b = g.battle().ok_or("battle is missing")?;
    let raised = b
        .units
        .iter()
        .find(|u| u.source == Some(0))
        .ok_or("find is missing")?;
    let living = b
        .units
        .iter()
        .find(|u| u.source == Some(1))
        .ok_or("find is missing")?;
    assert!(raised.undead && raised.is_fantastic());
    assert_eq!(raised.realm, Some(immunity::Realm::Death));
    for a in [
        "death_immunity",
        "cold_immunity",
        "poison_immunity",
        "illusion_immunity",
    ] {
        assert!(raised.has(a), "{a}");
        assert!(!living.has(a), "{a}");
    }
    assert!(!living.undead && !living.is_fantastic());
    Ok(())
}

/// A player of battles that always flees.
struct Quitter;

impl crate::battle::game::BattleBrain for Quitter {
    fn command(&mut self, _: &Game) -> BattleCommand {
        BattleCommand::Flee
    }
}

#[test]
fn a_battle_brain_plays_for_its_wizard_and_the_plain_ai_for_the_rest() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    g.set_battle_brain(A, Some(Box::new(Quitter)));
    let events = g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers: vec![a1],
        defenders: vec![d1],
    });
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Battle(BattleEvent::Fled {
                side: Side::Attacker
            })
        )),
        "{events:?}"
    );
    assert!(!g.in_battle());
    // Taken away again, the plain AI fights it out.
    g.set_battle_brain(A, None);
    let a2 = g.spawn_unit(A, at(4, 4), walker);
    let d2 = g.spawn_unit(D, at(5, 4), walker);
    let events = g.apply(Command::AutoResolve {
        mana_budget: None,
        attackers: vec![a2],
        defenders: vec![d2],
    });
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::Fled { .. }))));
    Ok(())
}

// ---- combat ability rules, spell shots, after the battle -------------

#[test]
fn a_large_shield_turns_missiles_but_not_blows() {
    let archer = || {
        let mut u = BattleUnit::simple(Side::Attacker, "archer", 6, 1, 1, 1).at((3, 10));
        u.ranged = 5;
        u.ammo = 4;
        u.ranged_kind = RangedKind::Missile;
        with_abilities(u, &["bow"])
    };
    let swordsman = || BattleUnit::simple(Side::Attacker, "swordsman", 6, 2, 4, 2).at((3, 10));
    let target = |shield: bool| move || dummy(if shield { &["large_shield"] } else { &[] });
    let open = damage_over_seeds(archer, target(false), true);
    let shielded = damage_over_seeds(archer, target(true), true);
    assert!(shielded * 4 < open * 3, "{shielded} vs {open}");
    // Two more dice of defence do not touch a melee blow.
    let open = damage_over_seeds(swordsman, target(false), false);
    let shielded = damage_over_seeds(swordsman, target(true), false);
    assert_eq!(open, shielded);
}

fn caster(abilities: &[&str], mana: i32, ammo: i32) -> BattleUnit {
    let mut u = BattleUnit::simple(Side::Attacker, "caster", 3, 2, 3, 2).at((3, 10));
    u.ranged = 6;
    u.ranged_kind = RangedKind::Magic;
    u.mana = mana;
    u.ammo = ammo;
    with_abilities(u, abilities)
}

#[test]
fn a_spell_caster_shoots_with_its_mana_and_mana_leak_costs_a_shot() -> TestResult {
    let mut b = duel(
        caster(&["fireball_attack", "spellcaster"], 20, 6),
        dummy(&[]),
        1,
    );
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!((u.mana, u.ammo), (17, 5), "three mana a shot");
    // Mana leak takes five mana, which is another shot and a half.
    b.units[0].leak_mana(5);
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!((u.mana, u.ammo), (12, 4));
    // With no mana left it cannot shoot.
    b.units[0].leak_mana(100);
    assert_eq!(
        (
            b.unit(0).ok_or("unit is missing")?.mana,
            b.unit(0).ok_or("unit is missing")?.ammo
        ),
        (0, 0)
    );
    // A unit with spell shots but no mana of its own simply loses one.
    let mut plain = caster(&["fireball_attack"], 0, 3);
    plain.leak_mana(5);
    assert_eq!((plain.mana, plain.ammo), (0, 2));
    plain.ranged_kind = RangedKind::Missile;
    plain.leak_mana(5);
    assert_eq!(plain.ammo, 2, "arrows are not spell shots");
    Ok(())
}

#[test]
fn regenerating_units_heal_a_hit_as_their_sides_turn_begins() -> TestResult {
    let mut hurt = dummy(&["regeneration"]);
    hurt.front_damage = 5;
    let mut b = duel(spearman(Side::Attacker, (3, 10)), hurt, 1);
    let before = b.unit(1).ok_or("unit is missing")?.hit_points();
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), before + 1);
    // Not on the other side's turn.
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), before + 1);
    // The Regeneration spell grants the same ability.
    let mut under_spell = dummy(&[]);
    under_spell.front_damage = 5;
    under_spell.abilities.push("regeneration".into());
    let mut b = duel(spearman(Side::Attacker, (3, 10)), under_spell, 1);
    let before = b.unit(1).ok_or("unit is missing")?.hit_points();
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), before + 1);
    Ok(())
}

fn game_with_abilities(unit_id: &str, abilities: &[&str]) -> Game {
    let mut g = game();
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == unit_id) {
        u.kind = crate::rules::UnitKind::Normal;
        u.abilities.extend(abilities.iter().map(|a| a.to_string()));
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(units);
    g
}

#[test]
fn luck_adds_one_to_hit_to_block_and_resistance_in_battle() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &["lucky"]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    let plain = {
        let mut other = game();
        let w = other
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let u = other.spawn_unit(A, at(4, 4), w);
        let d = other.spawn_unit(D, at(5, 4), w);
        other.apply(Command::BeginBattle {
            attackers: vec![u],
            defenders: vec![d],
        });
        other
            .battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?
            .clone()
    };
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    let lucky = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    assert_eq!(lucky.to_hit, plain.to_hit + 1);
    assert_eq!(lucky.to_block, plain.to_block + 1);
    assert_eq!(lucky.resistance, plain.resistance + 1);
    Ok(())
}

/// Start a battle of `attackers` against `defenders` walkers, then mark the units in `dead`
/// (attacker indexes, then defender indexes counted from the attackers' end) as slain and end
/// the turn so that the battle finds its winner.
fn finish_with_dead(
    g: &mut Game,
    attackers: &[hecs::Entity],
    defenders: &[hecs::Entity],
    dead: &[usize],
) -> Vec<Event> {
    g.apply(Command::BeginBattle {
        attackers: attackers.to_vec(),
        defenders: defenders.to_vec(),
    });
    let b = &mut g.battle.as_mut().expect("as_mut succeeds").battle;
    for &i in dead {
        b.units[i].figures = 0;
        b.units[i].status = Status::Dead;
    }
    g.apply(Command::Battle(BattleCommand::EndTurn))
}

#[test]
fn a_regenerating_unit_is_healed_after_the_fight_and_comes_back_if_its_side_won() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let troll = g
        .content
        .rules
        .units
        .id_of("troll_war_trolls")
        .ok_or("troll_war_trolls is not in the content")?;
    let alive = g.spawn_unit(A, at(4, 4), walker);
    let slain_troll = g.spawn_unit(A, at(4, 4), troll);
    let hurt_troll = g.spawn_unit(A, at(4, 4), troll);
    let slain_walker = g.spawn_unit(A, at(4, 4), walker);
    let foe = g.spawn_unit(D, at(5, 4), walker);
    g.world.insert_one(hurt_troll, crate::unit::Hurt(4))?;
    let events = finish_with_dead(
        &mut g,
        &[alive, slain_troll, hurt_troll, slain_walker],
        &[foe],
        &[1, 3, 4],
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::BattleEnded { winner, .. } if *winner == A)));
    assert!(g.world.contains(alive));
    assert!(!g.world.contains(slain_walker), "the dead stay dead");
    assert!(g.world.contains(slain_troll), "regeneration brings it back");
    assert_eq!(g.world.get::<&crate::unit::Hurt>(slain_troll)?.0, 0);
    assert_eq!(g.world.get::<&crate::unit::Hurt>(hurt_troll)?.0, 0);
    assert_eq!(
        g.units().unit_position(slain_troll),
        Some(at(5, 4)),
        "and moves in"
    );
    Ok(())
}

#[test]
fn a_regenerating_unit_that_loses_stays_dead() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let troll = g
        .content
        .rules
        .units
        .id_of("troll_war_trolls")
        .ok_or("troll_war_trolls is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), troll);
    let d = g.spawn_unit(D, at(5, 4), walker);
    finish_with_dead(&mut g, &[a], &[d], &[0]);
    assert!(!g.world.contains(a));
    Ok(())
}

#[test]
fn the_winners_survivors_gain_experience_and_fame_for_four_kills() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let fallen = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.set_fame(A, 3);
    let events = finish_with_dead(&mut g, &[a, fallen], &foes, &[1, 2, 3, 4, 5]);
    assert!(g.world.contains(a));
    // Four dead enemies: 2 each. The fallen of its own side give nothing, nor does it gain.
    let xp = g.world.get::<&crate::unit::Experience>(a)?.0;
    assert_eq!(xp, 8);
    assert_eq!(g.wizard(A).ok_or("wizard is missing")?.fame(), 4);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::BattleSpoils { player, experience: 8, fame: 1, .. } if *player == A
    )));
    Ok(())
}

#[test]
fn fewer_than_four_kills_give_experience_but_no_fame() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..2).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    finish_with_dead(&mut g, &[a], &foes, &[1, 2]);
    assert_eq!(g.world.get::<&crate::unit::Experience>(a)?.0, 4);
    assert_eq!(g.wizard(A).ok_or("wizard is missing")?.fame(), 0);
    Ok(())
}

#[test]
fn a_very_rare_foe_is_worth_a_point_of_fame() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == "high_men_cavalry") {
        u.cost = 600;
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(units);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    g.set_fame(D, 5);
    finish_with_dead(&mut g, &[a], &[d], &[1]);
    assert_eq!(g.wizard(A).ok_or("wizard is missing")?.fame(), 1);
    assert_eq!(
        g.wizard(D).ok_or("wizard is missing")?.fame(),
        5,
        "no hero fell, so the loser keeps its fame"
    );
    Ok(())
}

#[test]
fn a_fallen_hero_costs_its_owner_fame_by_level_and_a_human_loser_gives_one_back() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == "high_men_cavalry") {
        u.kind = crate::rules::UnitKind::Hero;
    }
    g.content.rules.units = crate::rules::Registry::new_sorted(units);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    // The first foe is a Veteran (level 1): half of 1 + 1 is 1.
    g.world.insert_one(foes[0], crate::unit::Experience(20))?;
    g.set_fame(D, 25);
    g.set_human(Some(D));
    finish_with_dead(&mut g, &[a], &foes, &[1, 2, 3, 4]);
    // 25 - 1 for the hero, then one more for the four kills of a human loser above 20 fame.
    assert_eq!(g.wizard(D).ok_or("wizard is missing")?.fame(), 23);
    // A hero's level 0 costs nothing.
    let mut g2 = game_with_abilities("high_men_cavalry", &[]);
    let walker = g2
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g2.spawn_unit(A, at(4, 4), walker);
    let d = g2.spawn_unit(D, at(5, 4), walker);
    g2.set_fame(D, 5);
    finish_with_dead(&mut g2, &[a], &[d], &[1]);
    assert_eq!(g2.wizard(D).ok_or("wizard is missing")?.fame(), 5);
    Ok(())
}

#[test]
fn healing_cannot_bring_back_what_was_destroyed() {
    let mut u = BattleUnit::simple(Side::Attacker, "spearman", 4, 5, 3, 2);
    // One figure lost to destruction, one to ordinary blows.
    u.figures = 2;
    u.record(attack::Damage {
        points: 10,
        kills: 2,
        gone: 5,
        ..Default::default()
    });
    assert_eq!(u.healable(), 5);
    assert_eq!(
        u.heal_points(20, false),
        5,
        "only the ordinary damage comes back"
    );
    assert_eq!(u.figures, 3);
    assert_eq!(u.healable(), 0);
    assert_eq!(u.damage_kinds, [0, 0, 5]);
    // Life steal can overheal, but not past the figures it has: the surplus becomes extra hits.
    let gained = u.heal_points(9, true);
    // Five bring the lost figure back, the other four are one more hit point for each of the
    // four figures.
    assert_eq!((gained, u.figures, u.extra_hits), (9, 4, 1));
    assert_eq!(u.healable(), 0, "the destroyed damage is still there");
}

#[test]
fn extra_hits_do_not_count_as_the_units_real_strength() {
    let mut u = BattleUnit::simple(Side::Attacker, "spearman", 2, 5, 3, 2);
    assert_eq!(u.max_hit_points(), 10);
    u.heal_points(4, true);
    assert_eq!(u.extra_hits, 2);
    assert_eq!(u.hit_points(), 14);
    assert_eq!(u.max_hit_points(), 10, "the real maximum stays");
}

#[test]
fn a_vortex_crushes_the_unit_it_lands_on_and_is_steered_once_a_turn_by_its_owner() -> TestResult {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (11, 10)),
        5,
    );
    b.units[1].figures_start = 1;
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

#[test]
fn vortices_wander_three_squares_each_turn_and_there_are_at_most_ten() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        8,
    );
    for i in 0..10 {
        assert!(b.can_make_vortex((6, 2 + i)));
        b.make_vortex(Side::Defender, (6, 2 + i));
    }
    assert!(!b.can_make_vortex((8, 8)), "only ten at once");
    let starts: Vec<_> = b.vortices.iter().map(|v| v.cell).collect();
    b.vortex_round();
    for (v, from) in b.vortices.iter().zip(starts) {
        assert!(map::distance(v.cell, from) <= 3, "{from:?} -> {:?}", v.cell);
        assert!(b.map.is_playable(v.cell));
    }
}

#[test]
fn magic_vortex_is_cast_on_a_free_square() -> TestResult {
    let mut g = hook_game("magic_vortex");
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    assert_eq!(
        cast_unit(&mut g, "magic_vortex", SpellAim::Tile(enemy_cell), 0),
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NoVortexRoom
        )))
    );
    let events = cast_unit(&mut g, "magic_vortex", SpellAim::Tile((10, 10)), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::VortexMade { cell: (10, 10) }))));
    assert_eq!(g.battle().ok_or("battle is missing")?.vortices.len(), 1);
    Ok(())
}

#[test]
fn a_battle_fought_after_recording_began_plays_again_the_same() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let a2 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    g.apply(Command::BeginBattle {
        attackers: vec![a1, a2],
        defenders: vec![d1],
    });
    let mut ended = false;
    for _ in 0..200 {
        let (side, ids): (Side, Vec<usize>) = {
            let b = g.battle().ok_or("battle is missing")?;
            (b.active, b.active_units(b.active).map(|u| u.id).collect())
        };
        for id in ids {
            let Some((t, cell)) = ({
                let b = g.battle().ok_or("battle is missing")?;
                b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            }) else {
                break;
            };
            let step = {
                let b = g.battle().ok_or("battle is missing")?;
                b.reachable(id)
                    .into_iter()
                    .min_by_key(|(p, _)| distance(*p, cell))
                    .map(|(p, _)| p)
            };
            if let Some(to) = step {
                g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
            }
            let evs = g.apply(Command::Battle(BattleCommand::Attack {
                unit: id,
                target: t,
            }));
            if evs.iter().any(|e| matches!(e, Event::BattleEnded { .. })) {
                ended = true;
                break;
            }
        }
        if ended {
            break;
        }
        g.apply(Command::Battle(BattleCommand::EndTurn));
    }
    assert!(ended && !g.in_battle());
    let replay = g.stop_recording().ok_or("stop_recording is missing")?;
    assert!(replay.commands.len() > 3, "the battle's orders are in it");
    let (again, _) = replay
        .play(g.content.clone(), usize::MAX)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(again.snapshot().to_text()?, g.snapshot().to_text()?);
    Ok(())
}

fn ghost(side: Side, cell: Pos) -> BattleUnit {
    let mut u = spearman(side, cell);
    u.abilities.push("invisibility".into());
    u
}

#[test]
fn an_invisible_unit_is_unseen_and_cannot_be_aimed_at_until_it_gives_itself_away() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        ghost(Side::Defender, (17, 10)),
        1,
    );
    assert!(!b.seen_by(Side::Attacker, 1), "hidden from the start");
    assert!(b.seen_by(Side::Defender, 1), "its own side sees it");
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::Unseen)
    );
    assert!(!b.legal_commands().iter().any(|c| matches!(
        c,
        BattleCommand::Attack { target: 1, .. } | BattleCommand::Shoot { target: 1, .. }
    )));
    // An enemy unit next to it sees it, and any attacker may then aim at it.
    b.units[0].cell = (16, 10);
    assert!(b.seen_by(Side::Attacker, 1));
    assert!(b
        .legal_commands()
        .iter()
        .any(|c| matches!(c, BattleCommand::Attack { target: 1, .. })));
}

#[test]
fn an_invisible_unit_shows_after_it_attacks_until_its_next_turn_and_to_those_who_see_through(
) -> TestResult {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        ghost(Side::Defender, (5, 10)),
        1,
    );
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    // The ghost walks up to the spearman and strikes it.
    b.apply(BattleCommand::Move {
        unit: 1,
        to: (4, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    b.apply(BattleCommand::Attack { unit: 1, target: 0 })
        .map_err(|e| format!("{e:?}"))?;
    assert!(b.units[1].revealed);
    b.units[0].cell = (3, 3); // out of reach of the ghost's square, so only the strike shows it
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
    b.units[0].abilities.push("illusion_immunity".into());
    assert!(
        b.seen_by(Side::Attacker, 1),
        "a unit immune to illusions sees it"
    );
    Ok(())
}

#[test]
fn invisibility_that_begins_in_battle_hides_the_unit_at_once() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        1,
    );
    assert!(b.seen_by(Side::Attacker, 1));
    b.units[1].revealed = true;
    b.raise_field(Side::Defender, Field::MassInvisibility, SpellId::new(1));
    b.refresh_fields();
    assert!(!b.units[1].revealed);
    assert!(!b.seen_by(Side::Attacker, 1));
}

/// Dispel Magic with 1500 mana against the enemy's Darkness (difficulty 25) and the caster's own
/// Prayer, after `burn` rolls of the game's dice: whether Darkness came down, and whether Prayer
/// is still up.
fn dispel_the_field(burn: usize) -> (bool, bool) {
    use crate::battle::field::Field;
    let mut g = casting_game();
    for _ in 0..burn {
        g.next_u64();
    }
    g.treasury_mut(A).expect("treasury_mut succeeds").mana = 2000;
    g.init_wizard(A, &[("sorcery", 5)], &[], None)
        .expect("init_wizard succeeds");
    let dispel = g
        .content
        .rules
        .spells
        .id_of("dispel_magic")
        .expect("id_of succeeds");
    let darkness = g
        .content
        .rules
        .spells
        .id_of("darkness")
        .expect("id_of succeeds");
    let prayer = g
        .content
        .rules
        .spells
        .id_of("prayer")
        .expect("id_of succeeds");
    {
        let mut m = g.wizard_mut(A).expect("wizard_mut succeeds");
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(dispel);
        m.get_mut::<crate::wizard::sheet::Skill>().left = 500;
    }
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .expect("id_of succeeds");
    let ours = g.spawn_unit(A, at(4, 4), walker);
    let theirs = g.spawn_unit(D, at(5, 4), walker);
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders: vec![theirs],
    });
    {
        let b = &mut g.battle.as_mut().expect("as_mut succeeds").battle;
        b.set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
        assert!(b.raise_field(Side::Defender, Field::Darkness, darkness));
        assert!(b.raise_field(Side::Attacker, Field::Prayer, prayer));
    }
    let events = g
        .battle_command(BattleCommand::Cast {
            spell: dispel,
            aim: SpellAim::Unit(1),
            mana: 1500,
        })
        .expect("value succeeds");
    let b = g.battle().expect("battle succeeds");
    let down = !b.has_field(Side::Defender, Field::Darkness);
    assert_eq!(
        down,
        events.contains(&Event::Battle(BattleEvent::FieldDispelled {
            side: Side::Defender,
            spell: darkness
        })),
        "{events:?}"
    );
    (down, b.has_field(Side::Attacker, Field::Prayer))
}

#[test]
fn dispel_magic_takes_down_the_enemy_battlefield_spells_and_leaves_its_casters_own() {
    // The roll misses one time in 250 whatever the strength, so look at several throws of the dice.
    let throws: Vec<(bool, bool)> = (0..8).map(dispel_the_field).collect();
    assert!(
        throws.iter().filter(|&&(down, _)| down).count() >= 6,
        "{throws:?}"
    );
    assert!(
        throws.iter().all(|&(_, own)| own),
        "a dispel never takes down the caster's own spells"
    );
}

/// A battle of player A's wizard, who knows `spells` and has plenty to cast with; the first
/// attacker is a `unit` of the test content.
fn wizard_battle(unit: &str, spells: &[&str]) -> Game {
    let mut g = casting_game();
    g.treasury_mut(A).expect("treasury_mut succeeds").mana = 2000;
    g.init_wizard(A, &[("nature", 5), ("life", 5), ("death", 5)], &[], None)
        .expect("value succeeds");
    for name in spells {
        let id = g.content.rules.spells.id_of(name).expect("id_of succeeds");
        let mut m = g.wizard_mut(A).expect("wizard_mut succeeds");
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&id);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != id);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
    }
    let kind = g.content.rules.units.id_of(unit).expect("id_of succeeds");
    let ours = g.spawn_unit(A, at(4, 4), kind);
    begin(&mut g, ours);
    g
}

/// The wizard casts `spell` on the first attacker, with a fresh turn's casting.
fn cast_on_first(g: &mut Game, spell: &str) -> Result<Vec<Event>, BattleError> {
    let id = g.content.rules.spells.id_of(spell).expect("id_of succeeds");
    g.battle
        .as_mut()
        .expect("as_mut succeeds")
        .battle
        .set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
    g.battle_command(BattleCommand::Cast {
        spell: id,
        aim: SpellAim::Unit(0),
        mana: 0,
    })
}

#[test]
fn a_lasting_spell_is_cast_on_a_unit_once() -> TestResult {
    let mut g = wizard_battle("high_men_cavalry", &["giant_strength"]);
    cast_on_first(&mut g, "giant_strength").map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, "giant_strength"),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );
    Ok(())
}

#[test]
fn stone_skin_gives_way_to_iron_skin_in_a_battle_too() -> TestResult {
    let mut g = wizard_battle("high_men_cavalry", &["stone_skin", "iron_skin"]);
    let defense = |g: &Game| {
        g.battle()
            .expect("battle succeeds")
            .unit(0)
            .expect("unit succeeds")
            .defense
    };
    let base = defense(&g);
    cast_on_first(&mut g, "stone_skin").map_err(|e| format!("{e:?}"))?;
    assert_eq!(defense(&g), base + 1);
    // Iron Skin on top leaves its five, not six, and Stone Skin cannot follow it.
    cast_on_first(&mut g, "iron_skin").map_err(|e| format!("{e:?}"))?;
    assert_eq!(defense(&g), base + 5);
    assert_eq!(
        cast_on_first(&mut g, "stone_skin"),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );

    let mut g = wizard_battle("high_men_cavalry", &["stone_skin", "iron_skin"]);
    cast_on_first(&mut g, "iron_skin").map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, "stone_skin"),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    assert_eq!(defense(&g), base + 5);
    Ok(())
}

#[test]
fn a_dispel_does_not_bring_back_the_bonus_of_a_spell_that_gives_way() -> TestResult {
    // Whichever of the three the dice take off, the defence is that of the stronger skin left.
    for strength in [12, 25, 45, 80, 150] {
        let mut g = wizard_battle(
            "high_men_cavalry",
            &["stone_skin", "iron_skin", "giant_strength"],
        );
        let base = g
            .battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?
            .defense;
        for spell in ["stone_skin", "iron_skin", "giant_strength"] {
            cast_on_first(&mut g, spell).map_err(|e| format!("{e:?}"))?;
        }
        g.dispel_battle_unit(0, strength, D);
        let unit = g
            .battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?;
        let has = |id: &str| unit.enchantments.iter().any(|e| e == id);
        let expected = if has("iron_skin") {
            5
        } else if has("stone_skin") {
            1
        } else {
            0
        };
        assert_eq!(unit.defense, base + expected, "strength {strength}");
    }
    Ok(())
}

#[test]
fn a_dispel_in_a_battle_tells_the_world_which_spell_went_and_whose_it_was() -> TestResult {
    use crate::game::{DispelSite, Event};
    let mut g = wizard_battle("high_men_cavalry", &["stone_skin"]);
    cast_on_first(&mut g, "stone_skin").map_err(|e| format!("{e:?}"))?;
    let events = g.dispel_battle_unit(0, 10_000, D);
    let out = g.battle_event_list(events);
    let named = out
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::EnchantmentDispelled {
                    site: DispelSite::BattleUnit,
                    ..
                }
            )
        })
        .count();
    assert_eq!(named, 1);
    Ok(())
}

#[test]
fn a_wizard_cannot_give_a_unit_the_ability_it_has_by_nature() {
    let mut g = wizard_battle("magic_spirit", &["wraith_form", "flight"]);
    assert_eq!(
        cast_on_first(&mut g, "wraith_form"),
        Err(BattleError::Spell(SpellError::AlreadyAble))
    );
    assert!(cast_on_first(&mut g, "flight").is_ok());
}

#[test]
fn holy_weapon_is_refused_in_a_battle_while_the_wizard_keeps_holy_arms() -> TestResult {
    let mut g = wizard_battle("high_men_cavalry", &["holy_weapon"]);
    let arms = g
        .content
        .rules
        .spells
        .id_of("holy_arms")
        .ok_or("holy_arms is not in the content")?;
    g.add_global(A, arms);
    assert_eq!(
        cast_on_first(&mut g, "holy_weapon"),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    Ok(())
}

#[test]
fn spells_the_battle_reads_by_name_can_be_cast_and_stay_on_the_unit() -> TestResult {
    let names = ["bless", "righteousness", "resist_magic", "spell_lock"];
    let mut g = wizard_battle("high_men_cavalry", &names);
    for name in names {
        cast_on_first(&mut g, name).map_err(|e| format!("{e:?}"))?;
    }
    let unit = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    assert!(names
        .iter()
        .all(|n| unit.enchantments.iter().any(|e| e == n)));
    assert_eq!(
        cast_on_first(&mut g, "bless"),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );
    Ok(())
}

#[test]
fn the_resistance_pairs_are_refused_in_a_battle() -> TestResult {
    let mut g = wizard_battle(
        "high_men_cavalry",
        &[
            "resist_elements",
            "elemental_armor",
            "resist_magic",
            "magic_immunity",
        ],
    );
    cast_on_first(&mut g, "elemental_armor").map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, "resist_elements"),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    let mut g = wizard_battle("high_men_cavalry", &["resist_elements", "elemental_armor"]);
    cast_on_first(&mut g, "resist_elements").map_err(|e| format!("{e:?}"))?;
    // The stronger one goes on top.
    cast_on_first(&mut g, "elemental_armor").map_err(|e| format!("{e:?}"))?;
    let mut g = wizard_battle("high_men_cavalry", &["resist_magic", "magic_immunity"]);
    cast_on_first(&mut g, "magic_immunity").map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, "resist_magic"),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    Ok(())
}

#[test]
fn heroism_cast_in_a_battle_raises_the_unit_to_the_level_it_gives() -> TestResult {
    let mut g = wizard_battle("high_men_cavalry", &["heroism"]);
    let melee = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .melee;
    cast_on_first(&mut g, "heroism").map_err(|e| format!("{e:?}"))?;
    // Heroism makes the unit Elite, which in the real path adds two points of attack (Regular and Elite).
    assert_eq!(
        g.battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?
            .melee,
        melee + 2
    );
    // Dispelled, the unit goes back to what it was; a dispel of nothing leaves the bonus.
    g.dispel_battle_unit(0, 1000, D);
    let unit = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    let has = unit.enchantments.iter().any(|e| e == "heroism");
    assert_eq!(unit.melee, melee + i32::from(has));
    Ok(())
}

#[test]
fn a_battle_is_saved_and_read_back_with_its_spells_and_items() -> TestResult {
    use crate::item::{Item, ItemBonus};
    use crate::rules::ItemKind;
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = Item {
        name: "Wand".into(),
        kind: ItemKind::Wand,
        cost: 100,
        icon: 0,
        bonus: ItemBonus::default(),
        powers: Vec::new(),
        charges: Some(("fire_bolt".into(), 2)),
        prefab: None,
        made: None,
    };
    g.arm_hero(hero, [Some(wand), None, None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .ok_or("fire_bolt is not in the content")?;
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: zap,
        aim: SpellAim::Unit(1),
        mana: 0,
    })
    .map_err(|e| format!("{e:?}"))?;
    let rules = &g.content.rules;
    let battle = g.battle().ok_or("battle is missing")?;
    let text = ron::ser::to_string(battle)?;
    let mut back: Battle = ron::from_str(&text)?;
    back.check_spells(rules).map_err(|e| format!("{e:?}"))?;
    let wand_entity = g
        .items()
        .hero_item_entity(hero, 0)
        .ok_or("hero_item_entity is missing")?;
    assert_eq!(
        back.units[0].casting.item_charges[0].item,
        hecs::Entity::DANGLING,
        "the entity is not saved"
    );
    back.bind_items(|s| (s == g.serial_of(wand_entity)).then_some(wand_entity))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(back.units, battle.units);
    assert_eq!(back.units[0].casting.item_charges[0].charges, 1);
    // The random stream continues where it stopped.
    assert_eq!(back.rng, battle.rng);
    // A save that holds a spell number the rules lack is refused.
    let mut save = battle.clone();
    save.fields[0].push((Field::Darkness, SpellId::new(9999)));
    assert_eq!(
        save.check_spells(rules).err(),
        Some(save::BattleLoadError::UnknownSpell(9999))
    );
    Ok(())
}

/// A battle between a hero with a charged wand and a spear, the hero's first spell cast.
fn battle_in_the_middle() -> (Game, hecs::Entity) {
    use crate::item::{Item, ItemBonus};
    use crate::rules::ItemKind;
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = Item {
        name: "Wand".into(),
        kind: ItemKind::Wand,
        cost: 100,
        icon: 0,
        bonus: ItemBonus::default(),
        powers: Vec::new(),
        charges: Some(("fire_bolt".into(), 2)),
        prefab: None,
        made: None,
    };
    g.arm_hero(hero, [Some(wand), None, None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .expect("id_of succeeds");
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell: zap,
        aim: SpellAim::Unit(1),
        mana: 0,
    })
    .expect("value succeeds");
    (g, hero)
}

/// Play the battle to its end with the battle AI on both sides.
fn finish_with_the_ai(g: &mut Game) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..5_000 {
        let Some(command) = g.next_battle_command() else {
            break;
        };
        match g.battle_command(command) {
            Ok(e) => events.extend(e),
            Err(_) => break,
        }
    }
    events
}

#[test]
fn a_game_saved_in_a_battle_continues_exactly_like_the_unsaved_one() -> TestResult {
    let (mut g, hero) = battle_in_the_middle();
    let text = g.snapshot().to_text()?;
    let mut loaded = Snapshot::from_text(&text)?.restore(g.content.clone())?;
    assert!(loaded.in_battle(), "the battle comes back with the world");
    assert_eq!(loaded.snapshot().to_text()?, text);
    // The wand's charges are bound to the loaded world's own item entity.
    let loaded_hero = loaded
        .units()
        .entity_of_serial(g.serial_of(hero))
        .ok_or("entity_of_serial is missing")?;
    let wand = loaded
        .items()
        .hero_item_entity(loaded_hero, 0)
        .ok_or("hero_item_entity is missing")?;
    let charges = &loaded
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .casting
        .item_charges;
    assert_eq!(charges.len(), 1);
    assert_eq!((charges[0].item, charges[0].charges), (wand, 1));
    let (a, b) = (finish_with_the_ai(&mut g), finish_with_the_ai(&mut loaded));
    assert!(!g.in_battle() && !loaded.in_battle());
    assert_eq!(a, b);
    assert_eq!(g.snapshot().to_text()?, loaded.snapshot().to_text()?);
    Ok(())
}

#[test]
fn a_game_saved_outside_a_battle_holds_no_battle() {
    let (mut g, _) = battle_in_the_middle();
    finish_with_the_ai(&mut g);
    assert!(g.snapshot().battle.is_none());
}

#[test]
fn a_save_whose_battle_names_a_unit_that_is_gone_is_refused() -> TestResult {
    let (g, _) = battle_in_the_middle();
    let mut snap = g.snapshot();
    let battle = snap.battle.as_mut().ok_or("as_mut is missing")?;
    let gone = battle.sources[0];
    snap.units.retain(|u| u.serial != gone.0);
    snap.heroes.retain(|h| h.serial != gone.0);
    assert!(snap.restore(g.content.clone()).is_err());
    Ok(())
}

#[test]
fn a_replay_can_start_in_a_battle() -> TestResult {
    let (mut g, _) = battle_in_the_middle();
    g.start_recording().map_err(|e| format!("{e:?}"))?;
    while let Some(command) = g.next_battle_command() {
        let events = g.apply(Command::Battle(command));
        if events.iter().any(|e| matches!(e, Event::Rejected(_))) {
            break;
        }
    }
    assert!(!g.in_battle());
    let replay = g.stop_recording().ok_or("stop_recording is missing")?;
    let (played, _) = replay
        .play(g.content.clone(), usize::MAX)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(played.snapshot().to_text()?, g.snapshot().to_text()?);
    Ok(())
}

#[test]
fn the_defender_raises_walls_at_its_city_for_the_battle_and_nobody_else_can() -> TestResult {
    let mut g = wizard_battle("high_men_cavalry", &[]);
    g.treasury_mut(D).ok_or("treasury_mut is missing")?.mana = 2000;
    g.init_wizard(D, &[("nature", 5), ("chaos", 5), ("death", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    for name in ["wall_of_stone", "wall_of_fire", "wall_of_darkness"] {
        let id = g
            .content
            .rules
            .spells
            .id_of(name)
            .ok_or("id_of is missing")?;
        g.wizard_mut(D)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
    }
    let cast = |g: &mut Game, name: &str, side: Side| {
        let spell = g.content.rules.spells.id_of(name).expect("id_of succeeds");
        let b = &mut g.battle.as_mut().expect("as_mut succeeds").battle;
        b.active = side;
        b.set_caster(side, crate::battle::BattleCaster::new(2000, 2000, 10));
        g.battle_command(BattleCommand::Cast {
            spell,
            aim: SpellAim::None,
            mana: 0,
        })
    };
    // In open country there is no city to wall.
    assert!(cast(&mut g, "wall_of_stone", Side::Defender).is_err());
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(false);
    assert!(g
        .battle()
        .ok_or("battle is missing")?
        .map
        .standing_walls()
        .is_empty());
    // The attacker cannot raise one, the defender can, once.
    assert!(cast(&mut g, "wall_of_stone", Side::Attacker).is_err());
    let events = cast(&mut g, "wall_of_stone", Side::Defender).map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::WallRaised { .. }))));
    assert!(g.battle().ok_or("battle is missing")?.map.is_fully_walled());
    assert_eq!(
        cast(&mut g, "wall_of_stone", Side::Defender),
        Err(BattleError::Spell(SpellError::AlreadyUp))
    );
    // Fire and Darkness are flags of the battle.
    cast(&mut g, "wall_of_fire", Side::Defender).map_err(|e| format!("{e:?}"))?;
    cast(&mut g, "wall_of_darkness", Side::Defender).map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("battle is missing")?;
    assert!(b.wall_of_fire && b.wall_of_darkness);
    Ok(())
}

/// A fight of player A's wizard (who knows `spells`, with `mana` in the reserve) against a
/// stack of walkers, left to the battle AI; returns the mana spent.
fn strategic_fight(spells: &[&str], mana: i32, budget: Option<u32>) -> i32 {
    let mut g = casting_game();
    g.treasury_mut(A).expect("treasury_mut succeeds").mana = 2000;
    g.init_wizard(A, &[("nature", 5), ("life", 5), ("death", 5)], &[], None)
        .expect("value succeeds");
    for name in spells {
        let id = g.content.rules.spells.id_of(name).expect("id_of succeeds");
        let mut m = g.wizard_mut(A).expect("wizard_mut succeeds");
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&id);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != id);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
    }
    g.treasury_mut(A).expect("treasury_mut succeeds").mana = mana;
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .expect("id_of succeeds");
    let ours: Vec<_> = (0..3).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let theirs: Vec<_> = (0..6).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.apply(Command::AutoResolve {
        attackers: ours,
        defenders: theirs,
        mana_budget: budget.map(|b| (A, b)),
    });
    assert!(!g.in_battle());
    mana - g.treasury_mut(A).expect("treasury_mut succeeds").mana
}

#[test]
fn the_battle_ai_spends_mana_in_a_strategic_fight_and_a_limit_stops_it() {
    let spells = ["giant_strength", "stone_skin", "bless"];
    let free = strategic_fight(&spells, 500, None);
    assert!(free > 3, "the AI casts when it may: spent {free}");
    let limited = strategic_fight(&spells, 500, Some(3));
    assert!(limited <= 3, "limited to 3, spent {limited}");
    assert_eq!(strategic_fight(&spells, 500, Some(0)), 0);
}

#[test]
fn the_default_limit_is_the_lower_of_the_casting_skill_and_half_the_reserve() -> TestResult {
    let mut g = casting_game();
    g.init_wizard(A, &[("nature", 5), ("life", 5), ("death", 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 500;
    // No spell for a battle: nothing to ask.
    g.wizard_mut(A)
        .ok_or("wizard_mut is missing")?
        .get_mut::<crate::wizard::sheet::KnownSpells>()
        .known
        .clear();
    assert_eq!(g.strategic_mana_default(A), None);
    let id = g
        .content
        .rules
        .spells
        .id_of("giant_strength")
        .ok_or("giant_strength is not in the content")?;
    {
        let mut m = g.wizard_mut(A).ok_or("wizard_mut is missing")?;
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&id);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != id);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
    }
    let skill = g.wizard(A).ok_or("wizard is missing")?.casting_skill() as u32;
    assert_eq!(g.strategic_mana_default(A), Some(skill.clamp(1, 250)));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 10;
    assert_eq!(g.strategic_mana_default(A), Some(skill.clamp(1, 5)));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 1;
    assert_eq!(g.strategic_mana_default(A), Some(1));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana = 0;
    assert_eq!(g.strategic_mana_default(A), None);
    Ok(())
}

#[test]
fn a_hasted_unit_attacks_twice_in_a_turn() -> TestResult {
    let mut hasted = spearman(Side::Attacker, (9, 10));
    hasted.abilities.push("hasted".into());
    hasted.apply_lasting(&[], &[]);
    let mut b = duel(hasted, spearman(Side::Defender, (10, 10)), 3);
    assert!(b.unit(0).ok_or("unit is missing")?.extra_attack);
    b.apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    assert!(
        !b.unit(0).ok_or("unit is missing")?.attacked,
        "one attack is left"
    );
    b.apply(BattleCommand::Attack { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    assert!(b.unit(0).ok_or("unit is missing")?.attacked);
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::AlreadyAttacked)
    );
    Ok(())
}

#[test]
fn a_battle_starts_with_the_games_melee_against_fliers_setting() -> TestResult {
    use crate::game::setting::MeleeVsFliers;
    for rule in [
        MeleeVsFliers::Zero,
        MeleeVsFliers::Half,
        MeleeVsFliers::Full,
    ] {
        let mut g = game();
        g.settings.rules.melee_vs_fliers = rule;
        let walker = g
            .content
            .rules
            .units
            .id_of("high_men_cavalry")
            .ok_or("high_men_cavalry is not in the content")?;
        let a = g.spawn_unit(A, at(4, 4), walker);
        let d = g.spawn_unit(D, at(5, 4), walker);
        g.apply(Command::BeginBattle {
            attackers: vec![a],
            defenders: vec![d],
        });
        assert_eq!(g.battle().ok_or("battle is missing")?.melee_vs_fliers, rule);
    }
    Ok(())
}

// ---- a defended square holds more than the nine who fight -------------------------------

#[test]
fn the_nine_most_experienced_defenders_fight_from_a_crowded_square() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attacker = g.spawn_unit(A, at(4, 4), walker);
    let defenders: Vec<_> = (0..12).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    // The last three are the veterans.
    for &d in &defenders[9..] {
        let _ = g.world.insert_one(d, crate::unit::Experience(500));
    }
    g.apply(Command::BeginBattle {
        attackers: vec![attacker],
        defenders: defenders.clone(),
    });
    let battle = g.battle().expect("a battle started");
    let fighting = battle
        .units
        .iter()
        .filter(|u| u.side == Side::Defender)
        .count();
    assert_eq!(fighting, crate::unit::MAX_STACK);
    let picked = g.fighter_preset(&defenders);
    assert!(defenders[9..].iter().all(|d| picked[..3].contains(d)));
    Ok(())
}

#[test]
fn defenders_who_sat_a_lost_battle_out_are_half_lost_and_driven_off_the_square() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attackers: Vec<_> = (0..9).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let defenders: Vec<_> = (0..crate::unit::MAX_TILE_LIMIT)
        .map(|_| g.spawn_unit(D, at(5, 4), walker))
        .collect();
    let benched = g.fighter_preset(&defenders)[crate::unit::MAX_STACK..].to_vec();
    g.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    // The attacker waits and the defence runs.
    g.apply(Command::Battle(BattleCommand::EndTurn));
    g.apply(Command::Battle(BattleCommand::Flee));
    assert!(!g.in_battle());
    let left: Vec<_> = benched.iter().filter(|&&b| g.world.contains(b)).collect();
    assert!(
        !left.is_empty() && left.len() < benched.len(),
        "{}",
        left.len()
    );
    for &&b in &left {
        assert_ne!(g.units().unit_position(b), Some(at(5, 4)), "driven off");
    }
    Ok(())
}

#[test]
fn defenders_who_sat_a_won_battle_out_stay() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attacker = g.spawn_unit(A, at(4, 4), walker);
    let defenders: Vec<_> = (0..12).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.apply(Command::BeginBattle {
        attackers: vec![attacker],
        defenders: defenders.clone(),
    });
    g.apply(Command::Battle(BattleCommand::Flee));
    assert!(!g.in_battle());
    assert!(defenders
        .iter()
        .all(|&d| g.units().unit_position(d) == Some(at(5, 4))));
    Ok(())
}

#[test]
fn the_player_names_who_fights_and_the_others_stay_out_with_their_moves() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let attackers: Vec<_> = (0..12).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let defenders: Vec<_> = (0..12).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    // Nine of each are named; the last three of the defenders are not.
    g.apply(Command::BeginBattle {
        attackers: attackers[..9].to_vec(),
        defenders: defenders[..9].to_vec(),
    });
    let sources = &g.battle.as_ref().ok_or("as_ref is missing")?.sources;
    assert!(defenders[..9].iter().all(|d| sources.contains(d)));
    assert!(defenders[9..].iter().all(|d| !sources.contains(d)));
    assert_eq!(
        g.battle.as_ref().ok_or("as_ref is missing")?.benched,
        defenders[9..].to_vec()
    );
    // The three attackers who were not named never entered it and keep their moves.
    let full = g.units().unit_moves_left(attackers[0]);
    assert!(attackers[9..]
        .iter()
        .all(|&a| g.units().unit_moves_left(a) == full));
    assert!(attackers[9..].iter().all(|a| !sources.contains(a)));
    Ok(())
}

#[test]
fn monsters_that_win_get_no_experience_and_the_loser_loses_no_fame() -> TestResult {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let beast = g.spawn_unit(crate::player::MONSTERS, at(4, 4), walker);
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(A, at(5, 4), walker)).collect();
    g.set_fame(A, 25);
    g.set_human(Some(A));
    let events = finish_with_dead(&mut g, &[beast], &foes, &[1, 2, 3, 4]);
    assert!(events.iter().any(
        |e| matches!(e, Event::BattleEnded { winner, .. } if *winner == crate::player::MONSTERS)
    ));
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BattleSpoils { .. })));
    assert_eq!(g.world.get::<&crate::unit::Experience>(beast)?.0, 0);
    assert_eq!(g.wizard(A).ok_or("wizard is missing")?.fame(), 25);
    Ok(())
}

/// A battle of `ours` against `foes` cavalry in which wizard A knows `spell`, has `mana` in the
/// battle and `reserve` overland.
fn spell_duel(
    spell: &str,
    foes: usize,
    mana: i32,
    reserve: i32,
) -> Result<Game, Box<dyn std::error::Error>> {
    let mut g = game_with_abilities("high_men_cavalry", &[]);
    let id = g
        .content
        .rules
        .spells
        .id_of(spell)
        .ok_or("the spell is not in the content")?;
    {
        let mut m = g.wizard_mut(A).ok_or("wizard_mut is missing")?;
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .researchable
            .remove(&id);
        m.get_mut::<crate::wizard::sheet::Candidates>()
            .0
            .retain(|c| *c != id);
        m.get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
        m.get_mut::<crate::wizard::sheet::Skill>().left = 500;
    }
    g.treasury_mut(A).ok_or("A has no treasury")?.mana = reserve;
    let walker = g
        .content
        .rules
        .units
        .id_of("high_men_cavalry")
        .ok_or("high_men_cavalry is not in the content")?;
    let ours = g.spawn_unit(A, at(4, 4), walker);
    let theirs: Vec<_> = (0..foes)
        .map(|_| g.spawn_unit(D, at(5, 4), walker))
        .collect();
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders: theirs,
    });
    g.battle
        .as_mut()
        .ok_or("as_mut is missing")?
        .battle
        .set_caster(Side::Attacker, BattleCaster::new(mana, mana, 10));
    Ok(g)
}

#[test]
fn the_scripted_wizard_casts_a_damage_spell_at_an_enemy_when_it_can_pay() -> TestResult {
    let g = spell_duel("fire_bolt", 2, 50, 50)?;
    let bolt = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .ok_or("fire_bolt is not in the content")?;
    let Some(BattleCommand::Cast { spell, aim, .. }) = super::brain::next_command(&g) else {
        return Err("the wizard did not cast".into());
    };
    assert_eq!(spell, bolt);
    let SpellAim::Unit(target) = aim else {
        return Err(format!("a bolt is aimed at a unit, not {aim:?}").into());
    };
    let battle = g.battle().ok_or("battle is missing")?;
    assert_eq!(
        battle.unit(target).ok_or("no such unit")?.side,
        Side::Defender
    );
    Ok(())
}

#[test]
fn the_scripted_wizard_with_no_mana_to_spare_moves_its_units_instead() -> TestResult {
    // No mana in the battle.
    let g = spell_duel("fire_bolt", 2, 0, 50)?;
    assert!(!matches!(
        super::brain::next_command(&g),
        Some(BattleCommand::Cast { .. })
    ));
    // Mana in the battle, but an empty reserve overland: the upkeep comes first.
    let mut g = spell_duel("fire_bolt", 2, 50, 0)?;
    g.treasury_mut(A).ok_or("A has no treasury")?.mana = -5;
    assert!(!matches!(
        super::brain::next_command(&g),
        Some(BattleCommand::Cast { .. })
    ));
    Ok(())
}

#[test]
fn the_scripted_wizard_heals_a_hurt_unit_but_not_a_whole_one() -> TestResult {
    let mut g = spell_duel("healing", 1, 50, 50)?;
    assert!(
        !matches!(
            super::brain::next_command(&g),
            Some(BattleCommand::Cast { .. })
        ),
        "nothing is hurt"
    );
    let battle = &mut g.battle.as_mut().ok_or("no battle")?.battle;
    let hurt = battle
        .units
        .iter_mut()
        .find(|u| u.side == Side::Attacker)
        .ok_or("no attacker")?;
    hurt.front_damage = 5;
    let id = hurt.id;
    let Some(BattleCommand::Cast { aim, .. }) = super::brain::next_command(&g) else {
        return Err("the wizard did not heal".into());
    };
    assert_eq!(aim, SpellAim::Unit(id));
    Ok(())
}

#[test]
fn the_legal_unit_casts_are_the_castable_spells_with_the_units_they_can_hit() -> TestResult {
    let mut g = casting_game();
    let hero = hero_unit(&mut g, &["fire_bolt", "healing"], 2);
    begin(&mut g, hero);
    let bolt = g
        .content
        .rules
        .spells
        .id_of("fire_bolt")
        .ok_or("fire_bolt is not in the content")?;
    // 7 mana pay the Fire Bolt (5), not Healing (15).
    let legal = g.legal_unit_casts();
    assert_eq!(legal.len(), 1);
    let (unit, cast) = &legal[0];
    assert_eq!((*unit, cast.spell, cast.least_mana), (0, bolt, 5));
    assert_eq!(cast.aims, [SpellAim::Unit(1)], "only the enemy unit");
    // The scripted units choose that cast.
    assert!(matches!(
        super::brain::unit_cast_choice(&g, g.battle().ok_or("no battle")?),
        Some(BattleCommand::UnitCast { unit: 0, spell, aim: SpellAim::Unit(1), mana: 5 }) if spell == bolt
    ));
    Ok(())
}
