use super::game::BattleStartError;
use super::map::{distance, Cell, GRID_H, GRID_W};
use super::spell::SpellSource;
use super::*;
use crate::game::rng::GameRng;
use crate::game::save::Snapshot;
use crate::game::testutil::{num, shipped};
use crate::game::{Command, Event, Game, Rejection};
use crate::map::{Grid, Map, Plane};
use crate::player::PlayerId;
use crate::rules::RealmId;
use crate::spell::target_error::TargetError;
use f4x_def::testdir::TestResult;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

const A: PlayerId = PlayerId(0);
const D: PlayerId = PlayerId(1);

/// A battle of the real game between shipped units: `ours` of player A and `theirs` of player D,
/// each a unit number of the content and the square it stands on. The game sets it up
/// (`BeginBattle`), so the units have their real numbers, rules and owners; only the squares are
/// chosen, on open ground, so that the squares a test names are free. The dice start from `seed`.
fn real_battle(ours: &[(u16, Pos)], theirs: &[(u16, Pos)], seed: u64) -> Battle {
    let mut g = game();
    let mut spawn = |owner: PlayerId, x: u32, list: &[(u16, Pos)]| -> Vec<hecs::Entity> {
        list.iter()
            .map(|&(number, _)| {
                let kind = g
                    .content
                    .rules
                    .units
                    .id_numbered(number)
                    .expect("a unit of the content");
                g.spawn_unit(owner, at(x, 4), kind)
            })
            .collect()
    };
    let attackers = spawn(A, 4, ours);
    let defenders = spawn(D, 5, theirs);
    g.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    let mut b = g.battle().expect("a battle began").clone();
    b.map = BattleMap::open();
    b.rng = GameRng::new(seed);
    // Put each unit on its square, and number the units in the order of the lists (the game
    // deploys the melee units first): the next unit of its side and kind that has none yet.
    let mut placed = vec![false; b.units.len()];
    let mut order = Vec::new();
    for (side, list) in [(Side::Attacker, ours), (Side::Defender, theirs)] {
        for &(number, cell) in list {
            let kind = g.content.rules.units.id_numbered(number);
            let at = b
                .units
                .iter()
                .position(|u| u.side == side && u.unit_id == kind && !placed[u.id])
                .expect("the battle has the unit");
            placed[at] = true;
            b.units[at].cell = cell;
            order.push(at);
        }
    }
    let mut units: Vec<BattleUnit> = order.iter().map(|&i| b.units[i].clone()).collect();
    for (id, u) in units.iter_mut().enumerate() {
        u.id = id;
    }
    b.units = units;
    b
}

/// The unit `number` of the content, as a battle makes it, for a side to add.
fn unit_numbered(number: u16) -> BattleUnit {
    let b = real_battle(&[(number, (0, 0))], &[(SPEARMEN, (20, 20))], 1);
    b.units[0].clone()
}

/// The same battle with its dice started from `seed`.
fn reseeded(b: &Battle, seed: u64) -> Battle {
    let mut c = b.clone();
    c.rng = GameRng::new(seed);
    c
}

const SPEARMEN: u16 = 104; // high_men_spearmen: eight figures, one move
const SWORDSMEN: u16 = 105; // high_men_swordsmen: no realm
const SKELETONS: u16 = 165; // death creatures that see through illusions
const CAVALRY: u16 = 107; // high_men_cavalry: two moves

#[test]
fn a_new_battle_starts_with_the_attacker_and_full_moves() -> TestResult {
    let b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
    let mut b = real_battle(&[(CAVALRY, (3, 10))], &[(SPEARMEN, (5, 10))], 1);
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
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
    let mut b = real_battle(&[(SPEARMEN, (9, 10))], &[(SPEARMEN, (10, 10))], 3);
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
    let mut b = real_battle(&[(CAVALRY, (3, 10))], &[(CAVALRY, (17, 10))], 1);
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
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
fn archers_shoot_with_ammunition_and_range_costs_accuracy() -> TestResult {
    // Longbowmen against a lone troll settler, who has the hit points to take every volley.
    let battle = |range: i32| {
        real_battle(
            &[(101 /* high_elf_longbowmen */, (3, 10))],
            &[(151 /* troll_settlers */, (3 + range, 10))],
            1,
        )
    };
    let (near, far) = (battle(1), battle(9));
    // Average damage at point blank against far away over many seeds.
    let mut near_total = 0;
    let mut far_total = 0;
    for seed in 0..200 {
        let (mut near, mut far) = (reseeded(&near, seed), reseeded(&far, seed));
        let BattleEvent::Shot { dealt: n, .. } = near
            .apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?[0]
        else {
            return Err("no shot".into());
        };
        let BattleEvent::Shot { dealt: f, .. } = far
            .apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?[0]
        else {
            return Err("no shot".into());
        };
        near_total += n;
        far_total += f;
    }
    assert!(near_total > far_total, "{near_total} vs {far_total}");
    // Ammunition runs out: one volley a turn, as many as the unit has shots.
    let mut b = battle(5);
    let shots = b.units[0].ammo;
    assert!(shots > 1);
    for fired in 1..=shots {
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(b.unit(0).ok_or("unit is missing")?.ammo, shots - fired);
        assert_eq!(
            b.apply(BattleCommand::Shoot { unit: 0, target: 1 }),
            Err(BattleError::AlreadyAttacked)
        );
        for _ in 0..2 {
            b.apply(BattleCommand::EndTurn)
                .map_err(|e| format!("{e:?}"))?;
        }
    }
    assert_eq!(
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 }),
        Err(BattleError::OutOfAmmo)
    );
    Ok(())
}

#[test]
fn a_walkers_melee_against_a_flier_follows_the_setting_and_the_flier_hits_back() -> TestResult {
    use crate::game::setting::MeleeVsFliers;
    let mut b = real_battle(
        &[(105 /* high_men_swordsmen */, (9, 10))],
        &[(103 /* high_elf_pegasai, a flier */, (10, 10))],
        8,
    );
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
        return Err(format!("{e:?}").into());
    };
    assert_eq!(dealt, 0);
    Ok(())
}

#[test]
fn a_battle_replays_exactly_from_the_same_seed() {
    let start = real_battle(&[(SPEARMEN, (9, 10))], &[(SPEARMEN, (10, 10))], 0);
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
fn a_stalemate_ends_at_the_turn_limit_in_the_defenders_favour() -> TestResult {
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
    let exhausted = at(|e| matches!(e, BattleEvent::Exhausted)).ok_or("no exhausted event")?;
    assert!(exhausted < at(|e| matches!(e, BattleEvent::Ended { .. })).ok_or("at is missing")?);
    Ok(())
}

/// Both sides walk at the nearest enemy and attack: every such battle ends.
fn play_out(mut b: Battle) -> Battle {
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
    let start = real_battle(
        &[(CAVALRY, (3, 9)), (CAVALRY, (3, 11)), (CAVALRY, (3, 10))],
        &[
            (48 /* beastmen_halberdiers */, (17, 10)),
            (48 /* beastmen_halberdiers */, (17, 11)),
        ],
        0,
    );
    for seed in 0..40 {
        let b = play_out(reseeded(&start, seed));
        assert!(b.winner.is_some(), "seed {seed}");
        assert!(b.turn < TURN_LIMIT, "seed {seed}: took {} turns", b.turn);
    }
}

#[test]
fn the_stronger_side_usually_wins() {
    // Paladins against spearmen.
    let start = real_battle(
        &[(113 /* high_men_paladins */, (3, 10))],
        &[(SPEARMEN, (17, 10))],
        0,
    );
    let mut strong_wins = 0;
    for seed in 0..60 {
        if play_out(reseeded(&start, seed)).winner == Some(Side::Attacker) {
            strong_wins += 1;
        }
    }
    assert!(strong_wins > 50, "{strong_wins} of 60");
}

#[test]
fn obstacles_shape_where_units_can_go() {
    let mut b = real_battle(&[(SPEARMEN, (9, 10))], &[(SPEARMEN, (12, 10))], 1);
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
    assert_eq!(GRID_W, 21);
}

// ---- joining the game ------------------------------------------------------------------

fn game() -> Game {
    game_seeded(5)
}

fn game_seeded(seed: u64) -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(num::terrain::GRASSLAND)
        .expect("id_of succeeds");
    let mut g = Game::new(seed);
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
fn flee_with(difficulty: u8, n: usize, held: &[SpellId]) -> (Game, Vec<hecs::Entity>) {
    let mut g = game();
    g.settings =
        crate::game::setting::GameSettings::for_difficulty(&g.content.settings, difficulty);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
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
        unit.put_enchantment(*enchantment);
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
    let (g, att) = flee_with(
        0,
        6,
        &[
            SpellId::new(165), /* black_sleep */
            SpellId::new(46),  /* confusion */
            SpellId::new(5),   /* web */
        ],
    );
    let alive: Vec<bool> = att.iter().map(|&u| g.world.contains(u)).collect();
    assert_eq!(alive, [false, false, false, true, true, true]);
    // On Average every unit rolls, but all three held ones are certain to go.
    let (g, att) = flee_with(
        2,
        3,
        &[
            SpellId::new(165), /* black_sleep */
            SpellId::new(46),  /* confusion */
            SpellId::new(5),   /* web */
        ],
    );
    assert!(att.iter().all(|&u| !g.world.contains(u)));
}

#[test]
fn ranged_abilities_map_to_battle_kinds() {
    use super::game::ranged_kind_of;
    let v = crate::game::testutil::abilities;
    assert_eq!(ranged_kind_of(&v(&[3 /* bow */])), RangedKind::Missile);
    assert_eq!(
        ranged_kind_of(&v(&[57 /* rock_throwing */])),
        RangedKind::Missile
    );
    assert_eq!(ranged_kind_of(&v(&[69 /* thrown */])), RangedKind::Short);
    assert_eq!(
        ranged_kind_of(&v(&[20 /* fire_breath */, 24 /* flying */])),
        RangedKind::Short
    );
    assert_eq!(ranged_kind_of(&v(&[29 /* ice_bolt */])), RangedKind::Magic);
    assert_eq!(ranged_kind_of(&v(&[25 /* forester */])), RangedKind::None);
}

fn found(g: &mut Game, owner: PlayerId, pos: crate::map::Position) -> hecs::Entity {
    let race = g.content.rules.races.id_numbered(7);
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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

const GREAT_WYRM: u16 = 189; // has Merging
const UNICORNS: u16 = 175; // teleport, two moves

/// A battle of one Merging unit and one spearman each side, and a second spearman for each
/// side so that one unit lost does not end it.
fn merging_battle() -> Battle {
    real_battle(
        &[(GREAT_WYRM, (3, 10)), (SPEARMEN, (3, 12))],
        &[(SPEARMEN, (17, 10)), (SPEARMEN, (17, 12))],
        1,
    )
}

/// The unit of the content `number` as a battle makes it (taken from a battle that has one).
fn unit_of(b: &Battle, number: u16) -> BattleUnit {
    let kind = shipped().rules.units.id_numbered(number);
    b.units
        .iter()
        .find(|u| u.unit_id == kind)
        .expect("the battle has the unit")
        .clone()
}

#[test]
fn a_merged_unit_leaves_the_field_and_comes_up_on_the_next_turn_wherever_it_likes() -> TestResult {
    let mut b = merging_battle();
    b.apply(BattleCommand::Merge { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    let gone = b.unit(0).ok_or("no unit")?;
    assert_eq!(gone.status, Status::Merged);
    assert!(b.unit_at((3, 10)).is_none(), "its square is free");
    assert!(b.active_units(Side::Attacker).all(|u| u.id != 0));
    // It cannot come up on the turn it went down, and a unit that is not merged cannot come up.
    assert_eq!(
        b.apply(BattleCommand::Surface {
            unit: 0,
            to: (9, 9)
        }),
        Err(BattleError::NotMerged)
    );
    assert_eq!(
        b.apply(BattleCommand::Surface {
            unit: 1,
            to: (9, 9)
        }),
        Err(BattleError::NotMerged)
    );
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    // The enemy cannot touch it: nothing to attack, and it is not on the legal lists.
    assert!(b
        .legal_commands()
        .iter()
        .all(|c| !matches!(c, BattleCommand::Attack { target: 0, .. })));
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    // Its own side's next turn: it comes up anywhere free, beside an enemy if it likes.
    assert!(b.legal_commands().contains(&BattleCommand::Surface {
        unit: 0,
        to: (16, 10)
    }));
    assert_eq!(
        b.apply(BattleCommand::Surface {
            unit: 0,
            to: (17, 10)
        }),
        Err(BattleError::Unreachable),
        "not onto a unit"
    );
    b.apply(BattleCommand::Surface {
        unit: 0,
        to: (16, 10),
    })
    .map_err(|e| format!("{e:?}"))?;
    let up = b.unit(0).ok_or("no unit")?;
    assert_eq!(up.status, Status::Active);
    assert_eq!(up.cell, (16, 10));
    assert_eq!(up.moves_left, up.moves, "coming up costs no moves");
    assert!(!up.attacked, "it can strike at once");
    b.apply(BattleCommand::Attack { unit: 0, target: 2 })
        .map_err(|e| format!("{e:?}"))?;
    // It cannot go down again on the turn it came up.
    assert_eq!(
        b.apply(BattleCommand::Merge { unit: 0 }),
        Err(BattleError::CannotMerge)
    );
    assert!(!b
        .legal_commands()
        .contains(&BattleCommand::Merge { unit: 0 }));
    // Next turn it can.
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert!(b
        .legal_commands()
        .contains(&BattleCommand::Merge { unit: 0 }));
    Ok(())
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
    b.units[0].moves_left = HalfInt::from_raw(1);
    assert!(b.units[0].moves_left > 0);
    b.apply(BattleCommand::Merge { unit: 0 })
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn a_side_with_only_merged_units_loses_and_they_get_away_whole() -> TestResult {
    let mut b = real_battle(&[(GREAT_WYRM, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
            (GREAT_WYRM, (3, 10)),
            (79 /* dwarven_hammerhands */, (3, 12)),
        ],
        &[(SPEARMEN, (4, 12))],
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
    let mut b = real_battle(&[(UNICORNS, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
    let mut b = real_battle(&[(UNICORNS, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
    b.map.set((10, 10), Cell::Blocked);
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (10, 10)
        }),
        Err(BattleError::Unreachable)
    );
    let mut w = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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
    // Spearmen and bowmen, alternately.
    let all = real_battle(
        &[(SPEARMEN, (3, 10)), (106 /* high_men_bowmen */, (3, 11))],
        &[(SPEARMEN, (17, 10))],
        1,
    );
    let (melee, shooter) = (unit_of(&all, SPEARMEN), unit_of(&all, 106));
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
    let mut army: Vec<(u16, Pos)> = vec![(SPEARMEN, (3, 9))];
    army.extend(
        shooters
            .iter()
            .enumerate()
            .map(|(i, &n)| (n, (3, 10 + i as i32))),
    );
    let all = real_battle(&army, &[(SPEARMEN, (17, 10))], 1);
    let mut units = vec![unit_of(&all, SPEARMEN)];
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
    assert_eq!(b.units[0].melee, melee + NODE_AURA_BONUS);
    assert_eq!(b.units[0].ranged, ranged + NODE_AURA_BONUS);
    assert_eq!(b.units[0].defense, defense + NODE_AURA_BONUS);
    assert_eq!(b.units[0].resistance, resist + NODE_AURA_BONUS);
    assert_eq!((b.units[1].melee, b.units[1].defense), theirs);
    // Gone with the aura.
    b.node_aura = None;
    b.refresh_fields();
    assert_eq!(b.units[0].melee, melee);
}

#[test]
fn a_node_stops_spells_of_other_realms_but_not_its_own() {
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 4);
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
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 4);
    // No wizard on either side to begin with (a battle of a game may have them).
    b.casters = Default::default();
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

/// Hit points that unit 1 of the battle `start` lost over many one-round fights from different
/// seeds: unit 0 strikes (or shoots) at it.
fn damage_over_seeds(start: &Battle, shoot: bool) -> i32 {
    let mut total = 0;
    for seed in 0..300 {
        let mut b = reseeded(start, seed);
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

/// The battle with the ability `number` taken off `unit`: the one difference between the unit
/// as shipped and the same unit without it.
fn without(b: &Battle, unit: usize, number: u16) -> Battle {
    let mut c = b.clone();
    let gone = crate::game::testutil::ability(number);
    c.units[unit].abilities.retain(|a| *a != gone);
    c
}

#[test]
fn weapon_immunity_blunts_normal_blows_but_not_magical_ones() {
    // Swordsmen against werewolves, who are immune to weapons.
    let stone = real_battle(
        &[(105 /* high_men_swordsmen */, (3, 10))],
        &[(168 /* werewolves */, (4, 10))],
        1,
    );
    let open = without(&stone, 1, 72 /* weapon_immunity */);
    let mut magic = stone.clone();
    magic.units[0].magical_weapon = true;
    let (open, stone, stone_vs_magic) = (
        damage_over_seeds(&open, false),
        damage_over_seeds(&stone, false),
        damage_over_seeds(&magic, false),
    );
    assert!(stone * 3 < open, "{stone} vs {open}");
    let ratio = f64::from(stone_vs_magic) / f64::from(open);
    assert!((ratio - 1.0).abs() < 0.1, "{ratio}");
}

#[test]
fn missile_immunity_stops_arrows_completely() {
    // Longbowmen against a necromancer, who is immune to missiles.
    let proof = real_battle(
        &[(101 /* high_elf_longbowmen */, (3, 10))],
        &[(32 /* necromancer */, (5, 10))],
        1,
    );
    let open = without(&proof, 1, 44 /* missile_immunity */);
    let (open, proof) = (
        damage_over_seeds(&open, true),
        damage_over_seeds(&proof, true),
    );
    assert!(open > 20, "{open}");
    assert_eq!(proof, 0);
}

#[test]
fn magic_immunity_stops_a_bolt_that_a_fire_immunity_does_not() {
    // Magicians' fireballs against paladins (magic immunity) and a fire elemental (fire immunity).
    let warded = real_battle(
        &[(51 /* beastmen_magicians */, (3, 10))],
        &[(113 /* high_men_paladins */, (5, 10))],
        1,
    );
    let open = damage_over_seeds(&without(&warded, 1, 40 /* magic_immunity */), true);
    assert!(open > 20, "{open}");
    assert_eq!(damage_over_seeds(&warded, true), 0);
    let fireproof = real_battle(
        &[(51 /* beastmen_magicians */, (3, 10))],
        &[(158 /* fire_elemental */, (5, 10))],
        1,
    );
    // The bolt is magic, not fire (only fire breath is stopped by fire immunity).
    let open = damage_over_seeds(&without(&fireproof, 1, 21 /* fire_immunity */), true);
    let proof = damage_over_seeds(&fireproof, true);
    assert!(open > 0 && proof * 10 > open * 9, "{proof} vs {open}");
}

#[test]
fn fire_breath_is_stopped_by_fire_immunity_and_magic_immunity() {
    // Doom drakes (fire breath, then melee) against a fire elemental and against paladins.
    for (foe, ability) in [
        (158 /* fire_elemental */, 21 /* fire_immunity */),
        (
            113, /* high_men_paladins */
            40,  /* magic_immunity */
        ),
    ] {
        let shielded = real_battle(
            &[(74 /* draconian_doom_drakes */, (3, 10))],
            &[(foe, (4, 10))],
            1,
        );
        let open = damage_over_seeds(&without(&shielded, 1, ability), false);
        let shielded = damage_over_seeds(&shielded, false);
        assert!(open > 0);
        // The melee blow that follows the breath still lands: only the breath's share is gone.
        assert!(shielded < open, "{ability}: {shielded} vs {open}");
    }
}

#[test]
fn an_illusion_ignores_armor_unless_the_defender_sees_through_it() {
    // Phantom warriors against skeletons, who see through illusions.
    let seen = real_battle(
        &[(192 /* phantom_warriors */, (3, 10))],
        &[(165 /* skeletons */, (4, 10))],
        1,
    );
    let plain = damage_over_seeds(&without(&seen, 1, 31 /* illusion_immunity */), false);
    let seen = damage_over_seeds(&seen, false);
    assert!(plain > seen * 2, "{plain} vs {seen}");
}

#[test]
fn a_defender_with_a_gaze_stones_the_attackers_before_they_strike() -> TestResult {
    // Spearmen attack a basilisk.
    let start = real_battle(&[(SPEARMEN, (3, 10))], &[(182 /* basilisk */, (4, 10))], 1);
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
    let mut start = real_battle(&[(SPEARMEN, (3, 10))], &[(182 /* basilisk */, (4, 10))], 1);
    start.units[1].ranged = 0;
    let mut plain = without(&start, 1, 64 /* stoning_gaze */);
    plain.units[1].ranged = 0;
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
        &[(SPEARMEN, (5, 10))],
        1,
    );
    start.units[0].front_damage = 8;
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
fn a_power_drain_shot_takes_two_to_twenty_mana_from_a_caster_that_fails_its_check() -> TestResult {
    // The elven archer with the Master Crossbow (it has Power Drain) shoots a wind mage hero, who
    // has mana from its casting skill.
    let mut g = game();
    let hero = hero_of_kind(&mut g, A, 30, &[], 0);
    let mage = hero_of_kind(&mut g, D, 15 /* wind_mage */, &[], 3);
    let crossbow = g
        .content
        .rules
        .items
        .defs()
        .iter()
        .find(|d| d.id == "the_master_crossbow")
        .map(crate::item::Item::from_def)
        .ok_or("the Master Crossbow is not in the content")?;
    g.arm_hero(hero, [Some(crossbow), None, None]);
    let events = g.apply(Command::BeginBattle {
        attackers: vec![hero],
        defenders: vec![mage],
    });
    assert!(g.in_battle(), "{events:?}");
    let mut start = g.battle().ok_or("no battle")?.clone();
    start.map = BattleMap::open();
    start.units[0].cell = (3, 10);
    start.units[1].cell = (5, 10);
    let mana = start.units[1].mana;
    assert!(mana > 0 && start.units[0].has(Ability::new(103)));
    let (mut drained, mut tries) = (0, 0);
    for seed in 0..60 {
        let mut b = reseeded(&start, seed);
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        let lost = mana - b.unit(1).ok_or("unit is missing")?.mana;
        assert!((0..=20).contains(&lost) && lost != 1, "{lost}");
        tries += 1;
        drained += i32::from(lost > 0);
    }
    assert!(drained > 0 && drained < tries, "{drained} of {tries}");
    Ok(())
}

const MAMMOTHS: u16 = 153; // troll_war_mammoths: wall crusher in melee
const CATAPULT: u16 = 37; // wall crusher and a ranged attack
const ENGINEERS: u16 = 110; // high_men_engineers: wall crusher

/// A battle of the real game at a city, walled or not: `ours` attack, `theirs` defend.
fn siege(ours: &[(u16, Pos)], theirs: &[(u16, Pos)], walled: bool, seed: u64) -> Battle {
    let mut b = real_battle(ours, theirs, seed);
    b.map = BattleMap::open().with_city(walled);
    b.refresh_fields();
    b
}

#[test]
fn a_wall_crusher_brings_a_wall_down_half_the_time_in_melee() -> TestResult {
    let mut fell = 0;
    for seed in 0..400 {
        let mut b = siege(&[(MAMMOTHS, (11, 11))], &[(SPEARMEN, (14, 11))], true, seed);
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
        let mut b = siege(&[(CATAPULT, (8, 11))], &[(SPEARMEN, (14, 11))], true, seed);
        let shots = b.units[0].ammo;
        b.apply(BattleCommand::ShootWall {
            unit: 0,
            cell: (12, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(b.unit(0).ok_or("unit is missing")?.ammo, shots - 1);
        fell += i32::from(!b.map.has_wall((12, 11)));
    }
    assert!((60..140).contains(&fell), "{fell}");
    Ok(())
}

#[test]
fn only_attacking_wall_crushers_may_strike_a_standing_wall() -> TestResult {
    let mut b = siege(
        &[
            (105 /* high_men_swordsmen */, (11, 11)),
            (ENGINEERS, (11, 13)),
        ],
        &[(SPEARMEN, (14, 11))],
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
    let mut b2 = siege(&[(SPEARMEN, (3, 3))], &[(ENGINEERS, (13, 11))], true, 1);
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
    let mut open = siege(&[(ENGINEERS, (11, 11))], &[(SPEARMEN, (14, 11))], false, 1);
    assert_eq!(
        open.apply(BattleCommand::AttackWall {
            unit: 0,
            cell: (12, 11)
        }),
        Err(BattleError::NoWall)
    );
    Ok(())
}

/// 1.50 note 476: a wall crusher can strike a wall section with an enemy standing right behind it:
/// the wall has its own target path, not the one of a unit hit.
#[test]
fn a_wall_crusher_strikes_a_wall_with_a_defender_standing_behind_it() -> TestResult {
    let mut b = siege(&[(ENGINEERS, (11, 11))], &[(SPEARMEN, (13, 11))], true, 1);
    let events = b
        .apply(BattleCommand::AttackWall {
            unit: 0,
            cell: (12, 11),
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(
        events.first(),
        Some(BattleEvent::WallAttacked { unit: 0, .. })
    ));
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::Attacked { .. } | BattleEvent::UnitDied { .. }
        )),
        "no unit is hit: {events:?}"
    );
    Ok(())
}

#[test]
fn a_wall_crusher_hitting_a_unit_on_the_wall_may_bring_the_wall_down_too() -> TestResult {
    let mut fell = 0;
    for seed in 0..300 {
        let mut b = siege(&[(MAMMOTHS, (11, 11))], &[(SPEARMEN, (12, 11))], true, seed);
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
    // Hit points a unit of the defenders loses over many fights: swordsmen strike it from the
    // square beside it, outside the city or (`from_inside`) from behind.
    let lost = |target: Pos, walled: bool, from_inside: bool| {
        let mut total = 0;
        for seed in 0..300 {
            let from = if from_inside {
                (14, 11)
            } else {
                (11, target.1)
            };
            let mut b = siege(
                &[(105 /* high_men_swordsmen */, from)],
                &[(79 /* dwarven_hammerhands */, target)],
                walled,
                seed,
            );
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
fn with_walls_covering_the_city_a_unit_behind_the_wall_gets_three_not_one() {
    // An attacker outside the city area, a target on the wall ring (12, 11) and one behind it.
    let mut b = siege(
        &[(105 /* high_men_swordsmen */, (10, 11))],
        &[(SPEARMEN, (12, 11)), (SPEARMEN, (13, 11))],
        true,
        1,
    );
    assert_eq!(
        (b.city_defense_bonus(1, 0), b.city_defense_bonus(2, 0)),
        (3, 1)
    );
    b.walls_cover_city = true;
    assert_eq!(
        (b.city_defense_bonus(1, 0), b.city_defense_bonus(2, 0)),
        (3, 3)
    );
    // Still nothing against an attacker inside, or without walls.
    b.units[0].cell = (14, 11);
    assert_eq!(b.city_defense_bonus(2, 0), 0);
    let mut open = siege(
        &[(105 /* high_men_swordsmen */, (10, 11))],
        &[(SPEARMEN, (13, 11))],
        false,
        1,
    );
    open.walls_cover_city = true;
    assert_eq!(open.city_defense_bonus(1, 0), 0);
}

#[test]
fn attackers_inside_the_city_hurt_it_each_turn() -> TestResult {
    let mut b = siege(
        &[(105 /* high_men_swordsmen */, (13, 11)), (105, (9, 11))],
        &[(SPEARMEN, (15, 11))],
        true,
        1,
    );
    // Each new turn of the attackers counts the unit that stands inside (not the one outside).
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.city_damage, 0, "the defenders' turn adds nothing");
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.city_damage, 1);
    for _ in 0..2 {
        b.apply(BattleCommand::EndTurn)
            .map_err(|e| format!("{e:?}"))?;
    }
    assert_eq!(b.city_damage, 2);
    // A battle in the open has no city to hurt.
    let b = real_battle(&[(SPEARMEN, (13, 11))], &[(SPEARMEN, (15, 11))], 1);
    assert_eq!(b.city_damage, 0);
    Ok(())
}

// ---- spells -----------------------------------------------------------------

mod casting {
    use super::*;
    use crate::battle::attack::{spell_attack, Combatant, Mode, Volley};
    use crate::battle::casting::{range_percent, BattleCaster};
    use crate::battle::immunity::{AttackNature, Immunities, Realm};
    use crate::rules::effects::DamageTraits;
    use crate::rules::stats::Stat;
    use f4x_def::testdir::TestResult;

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
            assert_eq!(range_percent(distance, |d| d), percent, "{distance:?}");
        }
        // A channeler is two squares away wherever they are, except at the fortress.
        let rules = shipped().rules;
        let channeler = [crate::wizard::magic::Retort::new(8)].into_iter().collect();
        let counted = |d| crate::wizard::retort::stat(&rules, &channeler, Stat::CastingDistance, d);
        assert_eq!(range_percent(Some(25), counted), 10);
        assert_eq!(range_percent(None, counted), 10);
        assert_eq!(range_percent(Some(0), counted), 5);
    }

    #[test]
    fn a_mana_budget_caps_what_the_wizard_can_afford_and_is_used_up_by_casting() -> TestResult {
        let mut near = BattleCaster::new(20, 100, 10);
        assert_eq!(near.affordable(), 20);
        near.budget = Some(HalfInt::from_int(8));
        assert_eq!(near.affordable(), 8);
        // Two mana for each point of skill: a budget of 8 pays for 4.
        let mut far = BattleCaster::new(20, 100, 20);
        far.budget = Some(HalfInt::from_int(8));
        assert_eq!(far.affordable(), 4);
        // Casting spends it.
        let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
        // No wizard on either side to begin with (a battle of a game may have them).
        b.casters = Default::default();
        b.set_caster(Side::Attacker, BattleCaster::new(20, 100, 10));
        b.set_mana_budget(Side::Attacker, Some(8));
        assert_eq!(b.pay_for_spell(5), HalfInt::from_int(5));
        let c = b.caster(Side::Attacker).ok_or("caster is missing")?;
        assert_eq!(
            (c.budget, c.mana, c.skill_left),
            (Some(HalfInt::from_int(3)), HalfInt::from_int(95), 15)
        );
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
        assert_eq!(far.mana_for(10), HalfInt::from_int(30));
        assert_eq!(
            BattleCaster::new(20, 100, 5).mana_for(7),
            HalfInt::from_raw(7),
            "halved, to the half point"
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

    #[test]
    fn magic_immunity_and_righteousness_turn_spells_away() {
        // A spell of the realm hits the unit `foe` of the content, blessed or not.
        let strike = |foe: u16, blessed: bool, realm| {
            let mut b = real_battle(&[(SPEARMEN, (3, 5))], &[(foe, (8, 5))], 1);
            if blessed {
                b.units[1].put_enchantment(SpellId::new(144));
            }
            let nature = AttackNature {
                stopped_by: Immunities::default(),
                realm: Some(realm),
                armor_piercing: false,
            };
            b.spell_damage(1, 10, &DamageTraits::default(), nature)
        };
        let paladins = 113; // high_men_paladins: magic immunity
        assert!(strike(SPEARMEN, false, Realm::Chaos).is_some());
        assert!(strike(paladins, false, Realm::Chaos).is_none());
        assert!(strike(SPEARMEN, true, Realm::Death).is_none());
        assert!(
            strike(SPEARMEN, true, Realm::Nature).is_some(),
            "only chaos and death"
        );
    }

    #[test]
    fn a_lasting_spell_changes_the_numbers_and_a_rebuild_takes_it_off_again() -> TestResult {
        // Haste and Giant Strength, cast the way the game casts them.
        let (haste, strength) = (70, 4);
        let mut g = wizard_battle(107 /* high_men_cavalry */, &[haste, strength]);
        let before = g
            .battle()
            .ok_or("no battle")?
            .unit(0)
            .ok_or("no unit")?
            .clone();
        cast_on_first(&mut g, haste).map_err(|e| format!("{e:?}"))?;
        cast_on_first(&mut g, strength).map_err(|e| format!("{e:?}"))?;
        let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
        let u = &b.units[0];
        assert_eq!(u.moves, before.moves * 2);
        assert_eq!(
            u.moves_left,
            before.moves_left * 2,
            "the extra moves come at once"
        );
        assert!(u.melee > before.melee);
        let spells: Vec<_> = u.combat_spells().iter().map(|(s, _)| s.num()).collect();
        assert!(
            spells.contains(&haste) && spells.contains(&strength),
            "{spells:?}"
        );
        // A dispel: back to the base, with no spells on.
        let base = u.base.clone().ok_or("as_deref is missing")?;
        b.rebuild_unit(0, base, Vec::new());
        let u = &b.units[0];
        assert_eq!(
            (u.moves, u.melee, u.first_strike),
            (before.moves, before.melee, before.first_strike)
        );
        assert!(u.combat_spells().is_empty());
        assert_eq!(u.abilities, before.abilities);
        assert!(u.enchantments.is_empty());
        Ok(())
    }

    /// A unit that Lionheart gave hit points, hurt so that `left` hit points of the new maximum
    /// remain, and the spell dispelled. Row 138 of the 1.50 notes: the original left such a unit
    /// alive at zero hit points or less. Returns the unit, the hit points the spell gave, and the
    /// unit's normal maximum.
    fn after_lionheart_goes(
        unit: u16,
        left: impl Fn(i32, i32) -> i32,
    ) -> Result<(BattleUnit, i32, i32), Box<dyn std::error::Error>> {
        let mut g = wizard_battle(unit, &[141 /* lionheart */]);
        cast_on_first(&mut g, 141).map_err(|e| format!("{e:?}"))?;
        let (normal, max) = {
            let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
            (
                u.base.clone().ok_or("no base")?.hits * u.figures_start as i32,
                u.max_hit_points(),
            )
        };
        assert!(max > normal, "lionheart gave hit points");
        {
            let u = &mut g.battle.as_mut().ok_or("no battle")?.battle.units[0];
            let left = left(normal, max);
            u.figures = ((left + u.hits - 1) / u.hits) as u32;
            u.front_damage = u.figures as i32 * u.hits - left;
            assert_eq!(u.hit_points(), left);
        }
        g.dispel_battle_unit(0, 10_000, D);
        let unit = g
            .battle()
            .ok_or("no battle")?
            .unit(0)
            .ok_or("no unit")?
            .clone();
        Ok((unit, max - normal, normal))
    }

    #[test]
    fn a_unit_whose_borrowed_hit_points_go_with_the_damage_it_took_dies() -> TestResult {
        // One figure (a fire elemental) takes all but one of the hit points it has with the
        // spell. The spell is dispelled: its hit points go with the damage, and 1 - borrowed < 0.
        let elemental = 158;
        let (u, borrowed, _) = after_lionheart_goes(elemental, |_, max| 1.min(max))?;
        assert!(borrowed > 1);
        assert!(!u.is_active() && u.hit_points() <= 0, "{u:?}");
        // Exactly zero is dead as well.
        let (u, _, _) = after_lionheart_goes(elemental, |normal, max| max - normal)?;
        assert!(!u.is_active(), "{u:?}");
        Ok(())
    }

    #[test]
    fn the_figures_of_a_unit_are_only_its_hit_points_so_the_whole_unit_dies_at_zero() -> TestResult
    {
        // Eight spearmen, the spell gives each more hit points. They take the damage that leaves
        // exactly what the spell gave: after the dispel it is zero, and no figure is spared.
        let (u, _, _) = after_lionheart_goes(SPEARMEN, |normal, max| max - normal)?;
        assert_eq!((u.figures, u.is_active()), (0, false), "{u:?}");
        Ok(())
    }

    #[test]
    fn a_hurt_unit_that_has_hit_points_left_keeps_them_in_fewer_figures() -> TestResult {
        // Eight spearmen with a few hit points more than the spell gave: the ceiling falls, and
        // the hit points that are left stay, in as many figures as they fill.
        let (u, borrowed, _) = after_lionheart_goes(SPEARMEN, |normal, max| max - normal + 2)?;
        assert!(borrowed > 0);
        assert_eq!(u.hit_points(), 2);
        assert_eq!(u.figures as i32 * u.hits - u.front_damage, 2);
        assert!(u.is_active() && u.figures < 8);
        // A unit that took no damage keeps all its figures.
        let (u, _, normal) = after_lionheart_goes(SPEARMEN, |_, max| max)?;
        assert_eq!((u.figures as i32, u.hit_points()), (8, normal));
        Ok(())
    }

    #[test]
    fn healing_brings_figures_back_and_a_new_creature_arrives_ready_to_act() {
        let mut b = real_battle(&[(SPEARMEN, (3, 5))], &[(SPEARMEN, (8, 5))], 1);
        let hits = b.units[0].hits;
        b.units[0].figures = 1;
        b.units[0].front_damage = 0;
        assert_eq!(b.units[0].hit_points(), hits);
        // Each figure has `hits` hit points: five figures' worth heal five figures.
        assert_eq!(b.spell_heal(0, 5 * hits), 5 * hits);
        assert_eq!(b.units[0].figures, 6);
        b.units[0].moves_left = HalfInt::ZERO;
        let fresh = unit_of(&b, SPEARMEN);
        let id = b.add_summoned(Side::Attacker, fresh, (4, 5));
        assert_eq!(id, 2);
        assert_eq!(b.units[2].moves_left, b.units[2].moves);
        assert!(b.units[2].is_active());
    }

    #[test]
    fn creatures_appear_only_on_a_free_square_of_the_casters_half() {
        let b = real_battle(&[(SPEARMEN, (3, 5))], &[(SPEARMEN, (17, 5))], 1);
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
    use crate::rules::SpellId;
    use f4x_def::testdir::TestResult;

    /// The spell of the shipped content that raises `field`.
    fn spell_for(field: Field) -> SpellId {
        crate::game::testutil::shipped()
            .rules
            .spells
            .iter()
            .find(|(_, def)| Field::of(def) == Some(field))
            .map(|(id, _)| id)
            .expect("a shipped spell raises the field")
    }

    /// A battle of the real game, the sides' first units standing far apart, and the dice seeded 3.
    fn pair(ours: u16, theirs: u16) -> Battle {
        real_battle(&[(ours, (3, 5))], &[(theirs, (17, 5))], 3)
    }

    const ANGEL: u16 = 177; // life
    const SHADOW_DEMONS: u16 = 171; // death, with a ranged attack
    const HELL_HOUNDS: u16 = 155; // chaos

    #[test]
    fn light_helps_life_and_hurts_death_and_darkness_does_the_opposite() {
        let mut b = pair(ANGEL, SHADOW_DEMONS);
        let (life, death) = (b.units[0].clone(), b.units[1].clone());
        assert!(death.ranged > 0);
        b.raise_field(
            Side::Attacker,
            Field::TrueLight,
            spell_for(Field::TrueLight),
        );
        let (l, d) = (&b.units[0], &b.units[1]);
        assert_eq!(
            (l.melee, l.defense, l.resistance),
            (life.melee + 1, life.defense + 1, life.resistance + 1)
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
        b.raise_field(Side::Defender, Field::Darkness, spell_for(Field::Darkness));
        assert_eq!(b.units[0].melee, life.melee);
        assert_eq!(b.units[1].resistance, death.resistance);
        // Darkness alone is the reverse.
        let mut b = pair(ANGEL, SHADOW_DEMONS);
        b.raise_field(Side::Attacker, Field::Darkness, spell_for(Field::Darkness));
        assert_eq!(b.units[0].defense, life.defense - 1);
        assert_eq!(b.units[1].defense, death.defense + 1);
    }

    #[test]
    fn torin_is_a_life_creature_to_light_and_darkness() -> TestResult {
        let mut g = super::game();
        let torin = hero_of_kind(&mut g, super::A, 34 /* the chosen */, &[], 0);
        let foe = g
            .content
            .rules
            .units
            .id_numbered(SKELETONS)
            .ok_or("skeletons are not in the content")?;
        let foe = g.spawn_unit(super::D, super::at(5, 4), foe);
        g.apply(Command::BeginBattle {
            attackers: vec![torin],
            defenders: vec![foe],
        });
        let mut b = g.battle().ok_or("no battle")?.clone();
        let plain = b.units[0].clone();
        assert_eq!(plain.realm, Some(Realm::Life));
        b.raise_field(Side::Defender, Field::Darkness, spell_for(Field::Darkness));
        assert_eq!(b.units[0].defense, plain.defense - 1);
        assert_eq!(b.units[0].resistance, plain.resistance - 1);
        Ok(())
    }

    #[test]
    fn holy_bonus_and_resistance_to_all_help_the_whole_side_and_do_not_add_up() {
        let mut b = pair(SWORDSMEN, SPEARMEN);
        let plain = b.units[0].clone();
        let enemy = b.units[1].clone();
        // A unit without a melee attack gets defence and resistance but no attack.
        let settlers = b.add_summoned(
            Side::Attacker,
            unit_numbered(73 /* draconian_settlers */),
            (3, 7),
        );
        let settlers_before = b.units[settlers].clone();
        assert_eq!(settlers_before.melee, 0);
        let arch_angel = b.add_summoned(Side::Attacker, unit_numbered(178), (3, 6));
        let level = b.units[arch_angel].special_level;
        assert!(level >= 2);
        let (p, a) = (&b.units[0], &b.units[settlers]);
        assert_eq!(
            (p.melee, p.defense, p.resistance),
            (
                plain.melee + level,
                plain.defense + level,
                plain.resistance + level
            )
        );
        assert_eq!((a.melee, a.defense), (0, settlers_before.defense + level));
        // The bearer gets it too, as it does on its own: the same numbers as alone.
        assert_eq!(
            b.units[arch_angel].melee,
            unit_numbered(178).melee,
            "the bearer gets it too"
        );
        assert_eq!(b.units[1].defense, enemy.defense, "the enemy gets nothing");
        // A second, weaker holy unit (an angel) does not add, and Resistance to All (a guardian
        // spirit, level 1) adds to the resistance only.
        b.add_summoned(Side::Attacker, unit_numbered(ANGEL), (4, 6));
        assert_eq!(b.units[0].defense, plain.defense + level);
        b.add_summoned(
            Side::Attacker,
            unit_numbered(176 /* guardian_spirit */),
            (4, 7),
        );
        let p = &b.units[0];
        assert_eq!(p.melee, plain.melee + level);
        assert!(p.resistance >= plain.resistance + level);
        // When the bearer falls the bonus goes at the next refresh.
        b.units[arch_angel].status = Status::Dead;
        b.refresh_fields();
        assert_eq!(
            b.units[0].defense,
            plain.defense + 1,
            "the weaker one still stands"
        );
    }

    /// The battle unit of a hero of the type `kind` with Leadership and Prayermaster of the doubled
    /// kind, and some experience, as a game makes it.
    fn hero_with_leading(kind: usize, leadership: bool, prayermaster: bool) -> BattleUnit {
        use crate::hero::Hero;
        use f4x_def::hero::Tier;
        let mut g = game();
        let hero = hero_of_kind(&mut g, A, kind, &[], 0);
        {
            let mut h = g.world.get::<&mut Hero>(hero).expect("a hero");
            if leadership {
                h.abilities.leadership = Tier::Super;
            }
            if prayermaster {
                h.abilities.prayermaster = Tier::Super;
            }
        }
        g.world
            .insert_one(hero, crate::unit::Experience(5000))
            .expect("insert_one succeeds");
        g.battle_unit_from(hero, Side::Attacker, 0)
            .expect("battle_unit_from succeeds")
    }

    #[test]
    fn a_heros_leadership_and_prayermaster_help_the_side_and_only_the_best_counts() {
        let mut b = pair(SWORDSMEN, SPEARMEN);
        let plain = b.units[0].clone();
        let enemy = b.units[1].clone();
        let archer = b.add_summoned(Side::Attacker, unit_numbered(101 /* longbowmen */), (3, 7));
        let archer_before = b.units[archer].clone();
        let mage = b.add_summoned(
            Side::Attacker,
            unit_numbered(51 /* beastmen_magicians */),
            (3, 8),
        );
        let mage_before = b.units[mage].clone();
        let imp = b.add_summoned(Side::Attacker, unit_numbered(HELL_HOUNDS), (3, 9));
        let imp_before = b.units[imp].clone();
        let hero = hero_with_leading(1, true, true);
        let (lead, pray) = (hero.leadership, hero.prayermaster);
        assert!(lead > 0 && pray > 0, "{lead} {pray}");
        let hero_before = hero.melee;
        let hero = b.add_summoned(Side::Attacker, hero, (3, 6));
        let (p, a) = (&b.units[0], &b.units[archer]);
        assert_eq!(p.melee, plain.melee + lead);
        assert_eq!(p.resistance, plain.resistance + pray);
        assert_eq!(
            a.ranged,
            archer_before.ranged + lead / 2,
            "half of it, rounded down, for a shot"
        );
        assert_eq!(
            b.units[mage].ranged, mage_before.ranged,
            "magic shots get none"
        );
        assert_eq!(b.units[imp].melee, imp_before.melee, "nor does a creature");
        assert_eq!(b.units[hero].melee, hero_before + lead, "the hero too");
        assert_eq!(b.units[1].melee, enemy.melee, "the enemy gets nothing");
        // A second hero with less, and Resistance to All below the Prayermaster, add nothing.
        b.add_summoned(
            Side::Attacker,
            unit_numbered(176 /* guardian_spirit */),
            (4, 6),
        );
        assert_eq!(b.units[0].melee, plain.melee + lead);
        assert_eq!(b.units[0].resistance, plain.resistance + pray);
        // When the hero falls the bonus goes at the next refresh.
        b.units[hero].status = Status::Dead;
        b.refresh_fields();
        assert_eq!(b.units[0].melee, plain.melee);
    }

    #[test]
    fn a_charmed_heros_resistance_passes_every_check() {
        let mut b = pair(SWORDSMEN, SWORDSMEN);
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
        let mut b = pair(SWORDSMEN, SPEARMEN);
        let before = b.units[0].clone();
        b.raise_field(
            Side::Attacker,
            Field::HighPrayer,
            spell_for(Field::HighPrayer),
        );
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
                before.to_hit + 1,
                before.to_block + 1
            )
        );
        assert_eq!(b.units[1].melee, 1, "the enemy gets nothing");
        // Prayer is the lesser one and High Prayer wins when both are up.
        b.raise_field(Side::Attacker, Field::Prayer, spell_for(Field::Prayer));
        assert_eq!(b.units[0].resistance, before.resistance + 3);
        b.fields[Side::Attacker.index()].clear();
        b.refresh_fields();
        let u = &b.units[0];
        assert_eq!(
            (u.melee, u.defense, u.resistance, u.to_hit, u.to_block),
            (
                before.melee,
                before.defense,
                before.resistance,
                before.to_hit,
                before.to_block
            )
        );
    }

    #[test]
    fn black_prayer_curses_the_enemy_and_warp_reality_every_unit_but_chaos() {
        let mut b = pair(SWORDSMEN, HELL_HOUNDS);
        let (ours, enemy) = (b.units[0].clone(), b.units[1].clone());
        assert!(enemy.ranged > 0);
        b.raise_field(
            Side::Attacker,
            Field::BlackPrayer,
            spell_for(Field::BlackPrayer),
        );
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
        assert_eq!(b.units[0].melee, ours.melee, "its own side is untouched");
        b.raise_field(
            Side::Defender,
            Field::WarpReality,
            spell_for(Field::WarpReality),
        );
        assert_eq!(b.units[0].to_hit, ours.to_hit - 2);
        assert_eq!(
            b.units[1].to_hit, enemy.to_hit,
            "chaos creatures are not warped"
        );
    }

    #[test]
    fn a_loss_never_takes_a_stat_below_nothing_and_comes_back_whole() {
        // Settlers have no melee attack at all.
        let mut b = pair(SWORDSMEN, 73 /* draconian_settlers */);
        assert_eq!(b.units[1].melee, 0);
        b.raise_field(
            Side::Attacker,
            Field::BlackPrayer,
            spell_for(Field::BlackPrayer),
        );
        assert_eq!(b.units[1].melee, 0);
        b.fields[0].clear();
        b.refresh_fields();
        assert_eq!(b.units[1].melee, 0, "not -1 + 1");
    }

    /// A unit's numbers, and whether the field makes it invisible or blurs it.
    type Seen = (Numbers, bool, bool);

    /// The numbers of the first unit of each side, and what the field asks of them.
    fn both_sides(g: &Game) -> Result<Vec<Seen>, Box<dyn std::error::Error>> {
        let b = g.battle().ok_or("no battle")?;
        [0, 1]
            .into_iter()
            .map(|i| {
                let u = b.unit(i).ok_or("no unit")?;
                Ok((numbers(u), u.field.invisible, u.field.blur))
            })
            .collect()
    }

    fn cast_field(g: &mut Game, spell: SpellId) -> Result<(), Box<dyn std::error::Error>> {
        g.battle.as_mut().ok_or("no battle")?.battle.set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
        g.battle_command(BattleCommand::Cast {
            spell,
            aim: SpellAim::None,
            mana: 0,
        })
        .map_err(|e| format!("casting {}: {e:?}", spell.num()))?;
        Ok(())
    }

    /// What a battlefield spell does to a unit does not depend on what else is on it or in what
    /// order they came (task 605). Every battlefield spell of the shipped content, cast by a
    /// wizard, with a lasting spell on our first unit (Giant Strength; Flame Blade, whose magical
    /// weapons Metal Fires must not take away, where the game allows it), on four pairs of shipped units: the field and the
    /// spell in either order give the same numbers, taking the field down leaves exactly the
    /// unit with the spell, dispelling the spell leaves exactly the unit with the field, and
    /// taking both off leaves the unit as it was. The pairs: plain cavalry, a life creature against
    /// a death creature (light and darkness), a unit with no melee attack (no phantom plus when
    /// the field goes down) and one that shoots arrows (Metal Fires).
    #[test]
    fn a_field_and_a_lasting_spell_are_independent_in_any_order_for_every_field() -> TestResult {
        let rules = &crate::game::testutil::shipped().rules;
        let fields: Vec<(SpellId, Field)> = rules
            .spells
            .iter()
            .filter_map(|(id, def)| Field::of(def).map(|f| (id, f)))
            .collect();
        assert!(
            fields.len() >= 12,
            "the shipped battlefield spells: {}",
            fields.len()
        );
        // Flame Blade is for normal units and heroes only, so not for the angel.
        let pairs: [(u16, u16, &[u16]); 4] = [
            (107, 107, &[4, 94]), // high_men_cavalry against the same
            (177, 165, &[4]),     // angel (life) against skeletons (death)
            (73, 107, &[4, 94]),  // draconian_settlers, no melee attack
            (69, 107, &[4, 94]),  // draconian_bowmen, arrows
        ];
        let mut changed = 0;
        for (ours, foe, lasting_spells) in pairs {
            for &lasting in lasting_spells {
                for &(spell, field) in &fields {
                    let case =
                        format!("{field:?} with spell {lasting} on unit {ours} against unit {foe}");
                    let teach = [spell.num(), lasting];
                    let build = || -> Result<Game, Box<dyn std::error::Error>> {
                        Ok(wizard_battle_versus(ours, &teach, foe, 1))
                    };
                    let plain = both_sides(&build()?)?;
                    let mut field_only = build()?;
                    cast_field(&mut field_only, spell)?;
                    changed += usize::from(both_sides(&field_only)? != plain);
                    let mut spell_only = build()?;
                    cast_on(&mut spell_only, lasting, 0).map_err(|e| format!("{e:?}"))?;
                    let spell_only_numbers = both_sides(&spell_only)?;

                    let mut field_first = build()?;
                    cast_field(&mut field_first, spell)?;
                    cast_on(&mut field_first, lasting, 0).map_err(|e| format!("{e:?}"))?;
                    let mut spell_first = build()?;
                    cast_on(&mut spell_first, lasting, 0).map_err(|e| format!("{e:?}"))?;
                    cast_field(&mut spell_first, spell)?;
                    assert_eq!(
                        both_sides(&field_first)?,
                        both_sides(&spell_first)?,
                        "order: {case}"
                    );

                    field_first
                        .battle
                        .as_mut()
                        .ok_or("no battle")?
                        .battle
                        .take_down_field(Side::Attacker, field);
                    assert_eq!(
                        both_sides(&field_first)?,
                        spell_only_numbers,
                        "the field down: {case}"
                    );

                    spell_first.dispel_battle_unit(0, 10_000, D);
                    assert_eq!(
                        both_sides(&spell_first)?,
                        both_sides(&field_only)?,
                        "the spell dispelled: {case}"
                    );
                    spell_first
                        .battle
                        .as_mut()
                        .ok_or("no battle")?
                        .battle
                        .take_down_field(Side::Attacker, field);
                    assert_eq!(both_sides(&spell_first)?, plain, "both off: {case}");
                }
            }
        }
        assert!(
            changed >= 20,
            "the fields change the numbers: {changed} cases"
        );
        Ok(())
    }

    #[test]
    fn metal_fires_arms_ordinary_units_only() -> TestResult {
        // Bowmen (ordinary) and war bears (a nature creature) against a spearman.
        let mut b = real_battle(
            &[
                (106 /* high_men_bowmen */, (3, 5)),
                (179 /* war_bears */, (4, 5)),
            ],
            &[(SPEARMEN, (17, 5))],
            1,
        );
        let (ordinary, fantastic) = (b.units[0].clone(), b.units[1].clone());
        b.raise_field(
            Side::Attacker,
            Field::MetalFires,
            spell_for(Field::MetalFires),
        );
        assert_eq!(
            (b.units[0].melee, b.units[0].ranged),
            (ordinary.melee + 1, ordinary.ranged + 1)
        );
        assert!(b.units[0].magical_weapon);
        assert_eq!(
            b.units[1].melee, fantastic.melee,
            "fantastic creatures already burn"
        );
        // A unit with a Flame Blade has magical weapons already: no double.
        let (flame, metal) = (94, spell_for(Field::MetalFires).num());
        let mut g = wizard_battle(106 /* high_men_bowmen */, &[flame, metal]);
        cast_on_first(&mut g, flame).map_err(|e| format!("{e:?}"))?;
        let blade = g
            .battle()
            .ok_or("no battle")?
            .unit(0)
            .ok_or("no unit")?
            .melee;
        cast_field(&mut g, spell_for(Field::MetalFires))?;
        assert_eq!(
            g.battle()
                .ok_or("no battle")?
                .unit(0)
                .ok_or("no unit")?
                .melee,
            blade,
            "no double with a flame blade"
        );
        Ok(())
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
        let mut b = pair(SWORDSMEN, SPEARMEN);
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
            let mut b = pair(SWORDSMEN, free);
            b.raise_field(Side::Attacker, Field::Entangle, spell_for(Field::Entangle));
            end_turns(&mut b, 1);
            assert_eq!(b.units[1].moves_left, b.units[1].moves, "{free}");
        }
        // Terror: a unit with no resistance never saves.
        let mut soft = pair(SWORDSMEN, SPEARMEN);
        soft.units[1].resistance = 0;
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
        let mut deathless = pair(SWORDSMEN, SKELETONS);
        deathless.units[1].resistance = 0;
        deathless.raise_field(Side::Attacker, Field::Terror, spell_for(Field::Terror));
        for seed in 0..20 {
            let mut b = reseeded(&deathless, seed);
            end_turns(&mut b, 1);
            assert!(!b.units[1].attacked, "seed {seed}");
        }
    }

    /// A battle of the real game in which the wizard of A has cast the spell `spell` (a number of
    /// the content) on the first unit of the other side, a `foe`. The unit has no resistance, so
    /// the spell takes hold.
    fn cast_on_foe(foe: u16, spell: u16) -> Battle {
        let mut g = wizard_battle_versus(SWORDSMEN, &[spell], foe, 1);
        g.battle.as_mut().expect("a battle").battle.units[1].resistance = 0;
        cast_on(&mut g, spell, 1).expect("the spell is cast");
        g.battle().expect("a battle").clone()
    }

    #[test]
    fn black_sleep_and_web_hold_a_unit_and_ground_a_flyer() {
        let mut b = cast_on_foe(103 /* high_elf_pegasai, a flier */, 5 /* web */);
        let u = &b.units[1];
        assert!(!u.flies && u.moves_left == 0);
        assert!(u.attacked, "a webbed unit cannot attack");
        assert_eq!(u.web_hits, WEB_HITS);
        end_turns(&mut b, 1);
        assert_eq!(b.units[1].moves_left, 0);
        assert!(b.units[1].attacked);

        let mut b = cast_on_foe(SPEARMEN, 165 /* black_sleep */);
        assert!(b.units[1].attacked);
        end_turns(&mut b, 1);
        assert!(b.units[1].attacked && b.units[1].moves_left == 0);
        assert!(b.legal_commands().iter().all(|c| !matches!(
            c,
            BattleCommand::Move { unit: 1, .. } | BattleCommand::Attack { unit: 1, .. }
        )));
    }

    #[test]
    fn dispelling_a_web_gives_a_flyer_its_flight_back() -> TestResult {
        // Row 302 of the 1.50 notes: the original left the unit without the movement type.
        let mut g = wizard_battle_versus(SWORDSMEN, &[5 /* web */], 103 /* a flier */, 1);
        let before = {
            let u = &mut g.battle.as_mut().ok_or("no battle")?.battle.units[1];
            u.resistance = 0;
            (u.flies, u.moves)
        };
        assert!(before.0);
        cast_on(&mut g, 5 /* web */, 1).expect("the spell is cast");
        assert!(
            !g.battle()
                .ok_or("no battle")?
                .unit(1)
                .ok_or("no unit")?
                .flies
        );
        g.dispel_battle_unit(1, 10_000, D);
        let u = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
        assert!(u.flies && u.moves == before.1 && !u.held_by_web(), "{u:?}");
        Ok(())
    }

    #[test]
    fn a_held_unit_spends_each_turn_tearing_at_the_web_until_it_is_free() {
        // Swordsmen, caught in the web, tear at it with their blows.
        let mut b = cast_on_foe(SWORDSMEN, 5 /* web */);
        let strength = b.units[1].web_strength();
        assert!(strength > 0 && strength < WEB_HITS);
        let mut left = WEB_HITS;
        let mut events = end_turns(&mut b, 1);
        loop {
            let dealt = strength.min(left);
            left -= dealt;
            let broken = left == 0;
            assert!(events.contains(&BattleEvent::WebStruck {
                unit: 1,
                dealt,
                broken
            }));
            assert_eq!(b.units[1].web_hits, left);
            if broken {
                break;
            }
            assert!(b.units[1].held_by_web() && b.units[1].attacked && b.units[1].moves_left == 0);
            assert!(b.legal_commands().iter().all(|c| !matches!(
                c,
                BattleCommand::Move { unit: 1, .. } | BattleCommand::Attack { unit: 1, .. }
            )));
            events = end_turns(&mut b, 2);
        }
        // It spent that turn on the web too; it acts from the next one, and still cannot fly.
        assert!(!b.units[1].held_by_web() && b.units[1].attacked && b.units[1].moves_left == 0);
        end_turns(&mut b, 2);
        let u = &b.units[1];
        assert!(!u.attacked && u.moves_left == u.moves && u.moves > 0);
        assert!(u.has(Ability::new(73)) && !u.flies);
    }

    #[test]
    fn a_unit_that_cannot_hurt_the_web_stays_held() {
        // Settlers have no attack at all.
        let mut b = cast_on_foe(111 /* high_men_settlers */, 5 /* web */);
        assert_eq!(b.units[1].web_strength(), 0);
        end_turns(&mut b, 4);
        assert_eq!(b.units[1].web_hits, WEB_HITS);
        assert!(b.units[1].held_by_web());
    }

    #[test]
    fn a_web_cast_again_on_a_unit_that_broke_free_closes_round_it_anew() {
        // A great wyrm tears through the web in one blow.
        let mut b = cast_on_foe(GREAT_WYRM, 5 /* web */);
        end_turns(&mut b, 2);
        assert_eq!(b.units[1].web_hits, 0);
        end_turns(&mut b, 2);
        let rules = crate::game::testutil::shipped().rules;
        let effects = crate::spell::unit_effect::UnitEffects::of(rules.spells.get(SpellId::new(5)));
        b.enchant_unit(1, SpellId::new(5), A, effects, &[], &[]);
        let u = &b.units[1];
        assert_eq!(u.web_hits, WEB_HITS);
        assert!(u.attacked && u.moves_left == 0);
        assert_eq!(
            u.enchantments
                .iter()
                .filter(|&&e| e == SpellId::new(5))
                .count(),
            1
        );
        assert_eq!(u.combat_spells().len(), 1);
    }

    #[test]
    fn what_a_unit_brings_to_the_web_follows_its_attacks() {
        // The strength of unit `number` as it ships; `ammo` of 0 for a unit that has shot its last.
        let web = |number: u16, ammo: Option<i32>| {
            let mut u = unit_numbered(number);
            if let Some(ammo) = ammo {
                u.ammo = ammo;
            }
            (u.web_strength(), u.melee, u.ranged)
        };
        let (w, melee, _) = web(SWORDSMEN, None);
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
        assert_eq!(w, melee + attack::IMMOLATION_STRENGTH);
    }

    #[test]
    fn a_bound_unit_fights_for_the_caster_and_goes_home_when_the_battle_ends() {
        let mut b = real_battle(
            &[(SWORDSMEN, (3, 5))],
            &[(SPEARMEN, (17, 5)), (SPEARMEN, (17, 7))],
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
            &[(SWORDSMEN, (3, 5))],
            &[(SPEARMEN, (17, 5)), (SPEARMEN, (17, 7))],
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
    fn confusion_picks_one_of_four_acts_and_can_lend_the_unit_to_the_enemy() {
        let start = cast_on_foe(SPEARMEN, 46 /* confusion */);
        let (mut idle, mut wandered, mut seized, mut free) = (0, 0, 0, 0);
        for seed in 0..200 {
            let mut b = reseeded(&start, seed);
            let events = end_turns(&mut b, 1);
            let act = events.iter().find_map(|e| match e {
                BattleEvent::Confused { unit: 1, act } => Some(*act),
                _ => None,
            });
            match act {
                Some(Confusion::Idle) => idle += 1,
                Some(Confusion::Wandered) => {
                    wandered += 1;
                    assert_ne!(b.units[1].cell, start.units[1].cell);
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
        let mut b = pair(SWORDSMEN, SPEARMEN);
        b.units[1].resistance = 0;
        b.raise_field(Side::Attacker, Field::Wrack, spell_for(Field::Wrack));
        end_turns(&mut b, 2);
        assert_eq!(b.turn, 2);
        let hurt = b.units[1].max_hit_points() - b.units[1].hit_points();
        assert!(hurt >= 1, "a figure with no resistance feels it");
        assert_eq!(b.units[0].hit_points(), b.units[0].max_hit_points());
        // Wraith Form is no shelter from it.
        let mut b = pair(SWORDSMEN, SPEARMEN);
        b.units[1].resistance = 0;
        b.units[1].put_enchantment(SpellId::new(181 /* wraith_form */));
        b.raise_field(Side::Attacker, Field::Wrack, spell_for(Field::Wrack));
        end_turns(&mut b, 4);
        assert!(b.units[1].hit_points() < b.units[1].max_hit_points());
        // Call Lightning: bolts of strength eight on the enemy only.
        let mut b = pair(SWORDSMEN, SPEARMEN);
        b.units[1].defense = 0;
        b.units[1].hits = 100;
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
    }

    /// What Wrack did to spearmen (eight figures) with plenty of hit points over ten turns: the
    /// points of each of its strikes.
    fn wrack_strikes(resistance: i32, abilities: &[u16]) -> Vec<i32> {
        let mut b = pair(SWORDSMEN, SPEARMEN);
        b.units[1].resistance = resistance;
        b.units[1].hits = 1000;
        for &a in abilities {
            b.units[1].abilities.push(Ability::new(a));
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

    /// Wraiths are non-corporeal and immune to normal weapons, as a unit under Wraith Form is:
    /// the bolts of Call Lightning are spells, and still hurt them.
    #[test]
    fn call_lightning_strikes_wraiths_that_weapons_cannot_hurt() -> TestResult {
        let mut g =
            wizard_battle_versus(107, &[35 /* call_lightning */], 170 /* wraiths */, 1);
        let spell = g
            .content
            .rules
            .spells
            .id_numbered(35)
            .ok_or("call_lightning is not in the content")?;
        g.battle.as_mut().ok_or("no battle")?.battle.set_caster(
            Side::Attacker,
            crate::battle::BattleCaster::new(2000, 2000, 10),
        );
        g.battle_command(BattleCommand::Cast {
            spell,
            aim: SpellAim::None,
            mana: 0,
        })
        .map_err(|e| format!("{e:?}"))?;
        let mut struck = 0;
        for _ in 0..16 {
            if !g.in_battle() {
                break;
            }
            let events = g
                .battle_command(BattleCommand::EndTurn)
                .map_err(|e| format!("{e:?}"))?;
            struck += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Event::Battle(BattleEvent::SpellDamaged { unit: 1, dealt, .. }) if *dealt > 0
                    )
                })
                .count();
        }
        assert!(struck > 0, "no bolt hurt the wraiths");
        Ok(())
    }

    #[test]
    fn mana_leak_drains_the_enemy_wizard_and_its_spell_shots() -> TestResult {
        // Beastmen magicians shoot spells; their wizard has a reserve.
        let mut b = pair(SWORDSMEN, 51 /* beastmen_magicians */);
        let ammo = b.units[1].ammo;
        assert!(ammo > 0);
        b.set_caster(Side::Defender, crate::battle::BattleCaster::new(20, 40, 10));
        b.raise_field(Side::Attacker, Field::ManaLeak, spell_for(Field::ManaLeak));
        end_turns(&mut b, 2);
        assert_eq!(
            b.caster(Side::Defender).ok_or("caster is missing")?.mana,
            40 - MANA_LEAK_DRAIN
        );
        assert_eq!(b.units[1].ammo, ammo - 1);
        Ok(())
    }

    #[test]
    fn blur_takes_one_off_the_chance_to_hit_unless_the_attacker_sees_through_illusions() {
        let mut b = real_battle(&[(SWORDSMEN, (5, 5))], &[(SPEARMEN, (9, 5))], 3);
        assert_eq!(b.blur_penalty(0, 1), 0);
        b.raise_field(Side::Defender, Field::Blur, spell_for(Field::Blur));
        assert_eq!(b.blur_penalty(0, 1), 1, "whatever the distance");
        assert_eq!(
            b.blur_penalty(1, 0),
            0,
            "only the blurred side's units are harder to hit"
        );
        // Skeletons see through illusions.
        let mut b = real_battle(&[(SKELETONS, (5, 5))], &[(SPEARMEN, (9, 5))], 3);
        b.raise_field(Side::Defender, Field::Blur, spell_for(Field::Blur));
        assert_eq!(b.blur_penalty(0, 1), 0);
    }

    #[test]
    fn an_invisible_creature_is_harder_to_hit_unless_the_attacker_sees_through_illusions() {
        // A night stalker is invisible.
        let mut b = real_battle(
            &[(SWORDSMEN, (5, 5))],
            &[(167 /* night_stalker */, (6, 5))],
            3,
        );
        assert_eq!(b.invisibility_penalty(0, 1), 1, "adjacent");
        assert_eq!(b.invisibility_penalty(1, 0), 0);
        b.units[0].cell = (1, 5);
        assert_eq!(b.invisibility_penalty(0, 1), 5, "one for each square");
        let b = real_battle(
            &[(SKELETONS, (5, 5))],
            &[(167 /* night_stalker */, (6, 5))],
            3,
        );
        assert_eq!(b.invisibility_penalty(0, 1), 0, "skeletons see through it");
        // Mass Invisibility does the same for the caster's whole side.
        let mut b = real_battle(&[(SWORDSMEN, (5, 5))], &[(SPEARMEN, (6, 5))], 3);
        b.raise_field(
            Side::Defender,
            Field::MassInvisibility,
            spell_for(Field::MassInvisibility),
        );
        assert_eq!(b.invisibility_penalty(0, 1), 1);
    }

    #[test]
    fn figures_that_fail_their_resistance_are_destroyed() -> TestResult {
        let slay = |foe: u16, resistance: i32| {
            let mut b = real_battle(&[(SWORDSMEN, (5, 5))], &[(foe, (6, 5))], 3);
            b.units[1].resistance = resistance;
            b
        };
        let mut b = slay(SPEARMEN, 0);
        let figures = b.units[1].figures;
        let dmg = b
            .spell_slay(1, Realm::Death, -9, &[], None)
            .ok_or("spell_slay is missing")?;
        assert_eq!(dmg.kills, figures, "no chance against a penalty of nine");
        assert!(!b.units[1].is_active());
        // A unit with the right immunity (skeletons) takes no harm; one with enough resistance
        // resists.
        let mut b = slay(SKELETONS, 30);
        assert!(b
            .spell_slay(1, Realm::Death, -2, &[crate::rules::Ability::new(16)], None)
            .is_none());
        assert_eq!(
            b.spell_slay(1, Realm::Death, -2, &[], None)
                .ok_or("spell_slay is missing")?
                .kills,
            0
        );
        // Disintegrate: resistance under ten and the whole unit is gone.
        let mut b = slay(SPEARMEN, 9);
        b.spell_slay(1, Realm::Chaos, 0, &[], Some(10))
            .ok_or("spell_slay is missing")?;
        assert!(!b.units[1].is_active());
        let mut b = slay(SPEARMEN, 10);
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
        .id_numbered(107)
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
    use crate::item::Item;
    use crate::rules::ItemKind;
    let mut g = game();
    // Only a hero carries items: the game's first hero, the Dwarf.
    let walker = g
        .content
        .rules
        .units
        .id_numbered(0)
        .ok_or("dwarf is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let plain = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    let sword = real_item(|d| {
        d.kind == ItemKind::Sword
            && d.powers
                .contains(&crate::rules::ItemPowerId::new(33 /* flaming */))
            && Item::from_def(d).bonus.attack > 0
    });
    let shield = real_item(|d| {
        let b = Item::from_def(d).bonus;
        d.kind == ItemKind::Shield && d.powers.is_empty() && b.defense > 0 && b.resistance > 0
    });
    let (sword_bonus, shield_bonus) = (sword.bonus, shield.bonus);
    let granted = sword.granted_abilities(&shipped().rules);
    g.arm_hero(unit, [Some(sword), Some(shield), None]);
    let armed = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    assert_eq!(
        armed.melee,
        plain.melee + sword_bonus.attack + 3,
        "attack and Flaming"
    );
    assert_eq!(armed.melee_plus, sword_bonus.to_hit);
    assert_eq!(armed.ranged_plus, 0);
    assert_eq!(armed.defense, plain.defense + shield_bonus.defense);
    assert_eq!(armed.resistance, plain.resistance + shield_bonus.resistance);
    assert!(armed.has(Ability::new(34)), "the shield's own ability");
    for a in granted {
        assert!(armed.has(a), "{a:?} from the sword's powers");
    }
    // The plus to hit counts for the melee blow, not for a shot.
    assert_eq!(
        armed.melee_combatant().to_hit,
        plain.melee_combatant().to_hit + sword_bonus.to_hit
    );
    Ok(())
}

#[test]
fn a_hero_with_casting_skill_has_mana_that_pays_for_its_spell_shots() -> TestResult {
    use crate::hero::Hero;
    use crate::rules::ItemKind;
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
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
    let staff = real_item(|d| d.kind == ItemKind::Staff && d.spell_skill > 0);
    let skill = staff.bonus.spell_skill;
    g.arm_hero(unit, [Some(staff), None, None]);
    let mut armed = g
        .battle_unit_from(unit, Side::Attacker, 0)
        .ok_or("battle_unit_from is missing")?;
    let mana = 7 + skill;
    assert_eq!(armed.mana, mana);
    // A magic shot costs the hero three mana, and its shots follow its mana.
    armed.ranged_kind = RangedKind::Magic;
    armed.ammo = armed.mana / SHOT_MANA;
    assert_eq!(armed.ammo, mana / SHOT_MANA);
    armed.spend_shot();
    assert_eq!(
        (armed.mana, armed.ammo),
        (mana - SHOT_MANA, (mana - SHOT_MANA) / SHOT_MANA)
    );
    armed.leak_mana(5);
    let left = (mana - SHOT_MANA - 5).max(0);
    assert_eq!((armed.mana, armed.ammo), (left, left / SHOT_MANA));
    Ok(())
}

/// The shipped wand with two charges of Dispel Magic True.
fn charged_wand() -> crate::item::Item {
    let wand = real_item(|d| d.num == 227);
    assert_eq!(
        wand.charges.map(|(spell, n)| (spell.num(), n)),
        Some((42, 2))
    );
    wand
}

/// The first shipped item (in the original's order) that `wanted` accepts.
fn real_item(wanted: impl Fn(&f4x_def::rules::ItemDef) -> bool) -> crate::item::Item {
    let content = shipped();
    let (_, def) = content
        .rules
        .items
        .iter()
        .filter(|(_, d)| wanted(d))
        .min_by_key(|(_, d)| d.num)
        .expect("the content has such an item");
    crate::item::Item::from_def(def)
}

/// A hero of player A that serves at (4, 4), made the way the game makes one: a hero type in the
/// rules (a walker that is a hero), the hero entity, and hiring it. Skill 0 and the spells it knows
/// are set after, as the rolled ones would be.
fn hero_unit(g: &mut Game, spells: &[u16], skill: u8) -> hecs::Entity {
    hero_of_kind(g, A, 0, spells, skill)
}

/// [`hero_unit`] of the hero type `kind` (the number of its unit), serving `owner`.
fn hero_of_kind(
    g: &mut Game,
    owner: PlayerId,
    kind: usize,
    spells: &[u16],
    skill: u8,
) -> hecs::Entity {
    use crate::hero::{Hero, HeroAbilities, HeroState};
    g.make_hero(
        Hero {
            kind,
            name: "Ann".into(),
            abilities: HeroAbilities::default(),
            casting_skill: skill,
            spells: spells
                .iter()
                .map(|s| {
                    shipped()
                        .rules
                        .spells
                        .id_numbered(*s)
                        .expect("a shipped spell")
                })
                .collect(),
        },
        HeroState::Free,
        0,
    );
    g.spawn_hero(owner, at(if owner == A { 4 } else { 5 }, 4), kind)
        .expect("spawn_hero succeeds")
}

fn begin(g: &mut Game, attacker: hecs::Entity) {
    begin_versus(g, attacker, 107);
}

/// [`begin`] against one unit of the kind `foe` (a unit number) of player D.
fn begin_versus(g: &mut Game, attacker: hecs::Entity, foe: u16) {
    let walker = g
        .content
        .rules
        .units
        .id_numbered(foe)
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
    let mut g = game();
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(A, &[(RealmId::new(2 /* chaos */), 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let call = g
        .content
        .rules
        .spells
        .id_numbered(115)
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
        .id_numbered(107)
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
    let mut g = game();
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(A, &[(RealmId::new(2 /* chaos */), 5)], &[], None)
        .map_err(|e| format!("{e:?}"))?;
    let spell = g
        .content
        .rules
        .spells
        .id_numbered(93)
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
        .id_numbered(39)
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

/// 1.50 note 720: with Evil Omens in play a caster unit pays half as much again for a life or
/// nature spell, as a wizard does; a chaos spell costs the same.
#[test]
fn evil_omens_make_a_unit_pay_more_for_life_and_nature_spells() -> TestResult {
    let mut g = game();
    let hero = hero_unit(&mut g, &[83 /* fire_bolt */, 125 /* healing */], 2);
    begin(&mut g, hero);
    g.battle.as_mut().ok_or("no battle")?.battle.units[0].mana = 20;
    let costs = |g: &Game| -> Vec<(i32, bool)> {
        g.battle_unit_spells(0)
            .iter()
            .map(|s| (s.cost, s.castable))
            .collect()
    };
    let plain = costs(&g);
    let omens = SpellId::new(198 /* evil_omens */);
    g.add_global(PlayerId(1), omens);
    let dear = costs(&g);
    assert_eq!(plain, [(5, true), (15, true)]);
    assert_eq!(
        dear,
        [(5, true), (22, false)],
        "healing costs 15 and a half"
    );
    Ok(())
}

#[test]
fn a_hero_casts_an_innate_spell_with_its_own_mana_and_ends_its_turn() -> TestResult {
    let mut g = game();
    let hero = hero_unit(&mut g, &[83 /* fire_bolt */, 125 /* healing */], 2);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_numbered(83)
        .ok_or("dispel_magic_true is not in the content")?;
    let mend = g
        .content
        .rules
        .spells
        .id_numbered(125)
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
    let mut g = game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = charged_wand();
    let save = wand.bonus.spell_save;
    // The spell is in the second slot: any slot works, not only the first.
    g.arm_hero(hero, [None, Some(wand), None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_numbered(42)
        .ok_or("dispel_magic_true is not in the content")?;
    let b = g.battle().ok_or("battle is missing")?;
    assert_eq!(
        b.unit(0).ok_or("unit is missing")?.mana,
        0,
        "no casting skill, no mana"
    );
    assert_eq!(b.unit(0).ok_or("unit is missing")?.casting.spell_save, save);
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
        Some((SpellId::new(42) /* dispel_magic_true */, 1))
    );
    Ok(())
}

#[test]
fn an_ability_spell_is_cast_once_a_battle_and_a_ward_or_a_miss_of_aim_stops_it() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(108)
        .ok_or("high_men_priests is not in the content")?;
    let unit = g.spawn_unit(A, at(4, 4), walker);
    begin(&mut g, unit);
    let healing = g
        .content
        .rules
        .spells
        .id_numbered(125)
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
    let casters: Vec<(u16, u16)> = [
        (5 /* casts_fireball */, 96 /* fireball */),
        (6 /* casts_healing */, 125 /* healing */),
        (4 /* casts_doom_bolt */, 104 /* doom_bolt */),
        (7 /* casts_web */, 5 /* web */),
    ]
    .iter()
    .flat_map(|&(ability, spell)| {
        shipped
            .rules
            .units
            .defs()
            .iter()
            .filter(move |d| {
                d.abilities
                    .iter()
                    .any(|a| *a == crate::game::testutil::ability(ability))
            })
            .map(move |d| (d.num, spell))
    })
    .collect();
    assert_eq!(casters.len(), 12);
    for (unit_id, spell_id) in casters {
        let mut g = game();
        let id = g
            .content
            .rules
            .units
            .id_numbered(unit_id)
            .ok_or("the unit is not in the content")?;
        let caster = g.spawn_unit(A, at(4, 4), id);
        begin(&mut g, caster);
        let spell = g
            .content
            .rules
            .spells
            .id_numbered(spell_id)
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
            (125 /* healing */, None) => continue,
            (_, Some(cast)) => assert!(!cast.aims.is_empty(), "{unit_id}"),
            (_, None) => return Err(format!("{unit_id} cannot cast {spell_id}").into()),
        }
        let aim = SpellAim::Unit(if spell_id == 125 /* healing */ { 0 } else { 1 });
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
    // War bears have a resistance of 6.
    let mut b = real_battle(&[(SPEARMEN, (1, 1))], &[(179 /* war_bears */, (5, 5))], 3);
    let resisted = |b: &mut Battle| (0..200).filter(|_| !b.resist_fails(1, None, 0)).count();
    assert!(resisted(&mut b) > 50);
    b.save_penalty = 10;
    assert_eq!(resisted(&mut b), 0);
}

/// A game whose hero (casting skill 4, so mana for several casts) has the real spell `id`. The
/// battle is on; the enemy is battle unit 1.
fn hook_game(id: u16) -> Game {
    hook_game_against(id, 107)
}

/// [`hook_game`] against one unit of the kind `foe`.
fn hook_game_against(id: u16, foe: u16) -> Game {
    let mut g = game();
    let hero = hero_unit(&mut g, &[id], 4);
    begin_versus(&mut g, hero, foe);
    g.battle.as_mut().expect("as_mut succeeds").battle.units[0].mana = 1000;
    g
}

fn cast_unit(
    g: &mut Game,
    spell: u16,
    aim: SpellAim,
    mana: u32,
) -> Result<Vec<Event>, BattleError> {
    let spell = g
        .content
        .rules
        .spells
        .id_numbered(spell)
        .expect("id_of succeeds");
    // The unit may cast again in the next test step.
    g.battle.as_mut().expect("as_mut succeeds").battle.units[0].moves_left = HalfInt::from_int(2);
    g.battle_command(BattleCommand::UnitCast {
        unit: 0,
        spell,
        aim,
        mana,
    })
}

#[test]
fn earth_to_mud_makes_open_ground_in_five_by_five_cost_a_walker_all_its_moves() -> TestResult {
    let mut g = hook_game(1 /* earth_to_mud */);
    // Any playable square may be aimed at.
    let centre = (10, 11);
    let events = cast_unit(&mut g, 1 /* earth_to_mud */, SpellAim::Tile(centre), 0)
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
    let walk = map.reachable_as((6, 11), HalfInt::from_int(4), Mover::walker(), &none);
    assert_eq!(
        walk[&(7, 11)].0,
        OPEN_COST,
        "the square before the mud costs the usual"
    );
    let into = map.reachable_as((7, 11), HalfInt::from_int(4), Mover::walker(), &none);
    assert!(
        into[&(8, 11)].0 >= HalfInt::from_int(4),
        "{:?}",
        into[&(8, 11)]
    );
    let ghost = Mover {
        ignores_mud: true,
        ..Mover::walker()
    };
    assert_eq!(
        map.reachable_as((7, 11), HalfInt::from_int(4), ghost, &none)[&(8, 11)].0,
        OPEN_COST
    );
    let flier = Mover {
        flying: true,
        through_walls: true,
        ..Mover::default()
    };
    assert_eq!(
        map.reachable_as((7, 11), HalfInt::from_int(4), flier, &none)[&(8, 11)].0,
        OPEN_COST
    );
    Ok(())
}

#[test]
fn disrupt_destroys_one_wall_section_and_only_where_a_wall_stands() -> TestResult {
    let mut g = hook_game(82 /* disrupt */);
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(true);
    let walls = g.battle().ok_or("battle is missing")?.map.standing_walls();
    assert!(walls.len() > 2);
    let bad = cast_unit(&mut g, 82 /* disrupt */, SpellAim::Tile((3, 3)), 0);
    assert_eq!(
        bad,
        Err(BattleError::Spell(SpellError::Target(TargetError::NoWall)))
    );
    let target = walls[1];
    let events = cast_unit(&mut g, 82 /* disrupt */, SpellAim::Tile(target), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(events.iter().any(
        |e| matches!(e, Event::Battle(BattleEvent::WallDestroyed { cell }) if *cell == target)
    ));
    let map = &g.battle().ok_or("battle is missing")?.map;
    assert!(!map.has_wall(target));
    assert_eq!(map.standing_walls().len(), walls.len() - 1);
    // The broken section cannot be struck again.
    let again = cast_unit(&mut g, 82 /* disrupt */, SpellAim::Tile(target), 0);
    assert_eq!(
        again,
        Err(BattleError::Spell(SpellError::Target(TargetError::NoWall)))
    );
    Ok(())
}

#[test]
fn cracks_call_never_swallows_a_merging_unit() -> TestResult {
    // The great wyrm merges.
    let mut g = hook_game_against(11 /* cracks_call */, 189 /* great_wyrm */);
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    assert!(g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .has(Ability::new(43) /* merging */));
    for _ in 0..30 {
        cast_unit(
            &mut g,
            11, /* cracks_call */
            SpellAim::Tile(enemy_cell),
            0,
        )
        .map_err(|e| format!("{e:?}"))?;
        assert!(g
            .battle()
            .ok_or("battle is missing")?
            .unit(1)
            .ok_or("unit is missing")?
            .is_active());
    }
    Ok(())
}

#[test]
fn cracks_call_swallows_a_walker_one_time_in_four_but_not_a_flier_and_breaks_a_wall() -> TestResult
{
    // A flier (pegasai) is never touched.
    let mut g = hook_game_against(11 /* cracks_call */, 103 /* high_elf_pegasai */);
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    for _ in 0..40 {
        cast_unit(
            &mut g,
            11, /* cracks_call */
            SpellAim::Tile(enemy_cell),
            0,
        )
        .map_err(|e| format!("{e:?}"))?;
        assert!(g
            .battle()
            .ok_or("battle is missing")?
            .unit(1)
            .ok_or("unit is missing")?
            .is_active());
    }
    // A walker (cavalry) falls in sooner or later.
    let mut g = hook_game(11 /* cracks_call */);
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    let mut died = false;
    for _ in 0..60 {
        let events = cast_unit(
            &mut g,
            11, /* cracks_call */
            SpellAim::Tile(enemy_cell),
            0,
        )
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
    let mut g = hook_game(11 /* cracks_call */);
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(true);
    let wall = g.battle().ok_or("battle is missing")?.map.standing_walls()[0];
    let events = cast_unit(&mut g, 11 /* cracks_call */, SpellAim::Tile(wall), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::WallDestroyed { .. }))));
    assert!(!g.battle().ok_or("battle is missing")?.map.has_wall(wall));
    Ok(())
}

#[test]
fn life_drain_takes_hit_points_that_a_hero_keeps_and_death_creatures_are_immune() -> TestResult {
    let mut g = hook_game(167 /* life_drain */);
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
    let events = cast_unit(&mut g, 167 /* life_drain */, SpellAim::Unit(1), 10)
        .map_err(|e| format!("{e:?}"))?;
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
    let refused = cast_unit(&mut g, 167 /* life_drain */, SpellAim::Unit(1), 10);
    assert_eq!(
        refused,
        Err(BattleError::Spell(SpellError::Target(TargetError::Deathly)))
    );
    Ok(())
}

#[test]
fn word_of_recall_takes_any_unit_to_the_circle_where_recall_hero_wants_a_hero() -> TestResult {
    let mut g = hook_game(47 /* word_of_recall */);
    let no_circle = cast_unit(&mut g, 47 /* word_of_recall */, SpellAim::Unit(0), 0);
    assert_eq!(
        no_circle,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NoCircle
        )))
    );
    g.wiz_mut(0)
        .put(Some(crate::wizard::fortress::SummonAt(at(1, 1))));
    let enemy = cast_unit(&mut g, 47 /* word_of_recall */, SpellAim::Unit(1), 0);
    assert_eq!(
        enemy,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NotYourUnit
        )))
    );
    // A fight at the circle itself has nowhere to recall to (1.50 note 704).
    let site = g
        .battle()
        .and_then(|b| b.site)
        .ok_or("the battle has no site")?;
    g.wiz_mut(0)
        .put(Some(crate::wizard::fortress::SummonAt(site)));
    assert_eq!(
        cast_unit(&mut g, 47 /* word_of_recall */, SpellAim::Unit(0), 0),
        Err(BattleError::Spell(SpellError::Target(
            TargetError::CircleHere
        )))
    );
    g.wiz_mut(0)
        .put(Some(crate::wizard::fortress::SummonAt(at(1, 1))));
    let events = cast_unit(&mut g, 47 /* word_of_recall */, SpellAim::Unit(0), 0)
        .map_err(|e| format!("{e:?}"))?;
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
    let walker = g
        .content
        .rules
        .units
        .id_numbered(173)
        .ok_or("demon_lord is not in the content")?;
    let lord = g.spawn_unit(A, at(4, 4), walker);
    begin(&mut g, lord);
    assert!(g.can_summon_demon(0));
    // Not the other side's unit, not a unit without the ability.
    assert_eq!(
        g.battle_command(BattleCommand::SummonDemon { unit: 1 }),
        Err(BattleError::NotYours)
    );
    for turn in 0..3 {
        g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[0].moves_left =
            HalfInt::from_int(1);
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
            (demon.side, demon.unit_id),
            (Side::Attacker, Some(UnitId::new(169))) // demon
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
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[0].moves_left = HalfInt::from_int(1);
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

/// A battle of the real game at an open city, `ours` against `theirs`.
fn city_battle(ours: &[(u16, Pos)], theirs: &[(u16, Pos)]) -> Battle {
    let mut b = real_battle(ours, theirs, 5);
    b.map = city_map();
    b
}

#[test]
fn a_wall_of_fire_burns_walkers_that_step_into_the_city_but_not_fliers() -> TestResult {
    let far = (SPEARMEN, (19, 2));
    for (number, burned) in [(CAVALRY, true), (103 /* high_elf_pegasai */, false)] {
        let mut b = city_battle(&[(number, (10, 11))], &[far]);
        let figures = b.units[0].figures;
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
        assert_eq!(hurt, burned, "unit {number}");
        assert_eq!(
            b.unit(0).ok_or("unit is missing")?.figures < figures,
            burned
        );
    }
    // Walking about outside, or inside once there, costs nothing; so does a city without it.
    let mut b = city_battle(&[(CAVALRY, (13, 11))], &[far]);
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
    let mut b = city_battle(&[(CAVALRY, (10, 11))], &[far]);
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
    let mut b = city_battle(&[(SPEARMEN, (11, 11))], &[(SPEARMEN, (12, 11))]);
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
    // Longbowmen shoot into the city; shadow demons (illusion immune) see through the darkness.
    let shoot = |shooter: u16, from: Pos, dark: bool| {
        let mut b = city_battle(&[(shooter, from)], &[(SPEARMEN, (13, 11))]);
        b.wall_of_darkness = dark;
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
    };
    let longbowmen = 101;
    assert_eq!(
        shoot(longbowmen, (8, 11), true),
        Err(BattleError::WallOfDarkness)
    );
    assert!(
        shoot(171 /* shadow_demons */, (8, 11), true).is_ok(),
        "illusion immunity sees through"
    );
    assert!(
        shoot(longbowmen, (8, 11), false).is_ok(),
        "no wall, no darkness"
    );
    assert!(
        shoot(longbowmen, (12, 10), true).is_ok(),
        "shooters inside the city are not stopped"
    );
}

#[test]
fn a_flying_fortress_shuts_the_city_to_walkers_and_its_walkers_in() {
    let map = city_map().with_flying_fortress();
    let mut b = real_battle(
        &[(CAVALRY, (10, 11)), (103 /* high_elf_pegasai */, (10, 12))],
        &[(CAVALRY, (13, 11)), (103, (13, 12))],
        5,
    );
    b.map = map.clone();
    for u in &mut b.units {
        u.moves = HalfInt::from_int(10);
        u.moves_left = u.moves;
    }
    let area = |reach: &std::collections::HashMap<Pos, HalfInt>| {
        reach.keys().any(|&p| map::in_city_area(p))
    };
    let ring = |reach: &std::collections::HashMap<Pos, HalfInt>| {
        reach.keys().any(|&p| map::in_city_ring(p))
    };
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
    // An open field: a swordsman next to an enemy, two archers, a unit with no ammunition.
    let longbowmen = 101;
    let mut b = real_battle(
        &[
            (SWORDSMEN, (3, 10)),
            (longbowmen, (3, 12)),
            (longbowmen, (4, 14)),
        ],
        &[(SPEARMEN, (4, 10)), (longbowmen, (9, 12))],
        1,
    );
    b.units[2].ammo = 0;
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
        &[(CATAPULT, (11, 11)), (MAMMOTHS, (3, 3))],
        &[(SPEARMEN, (14, 11)), (SPEARMEN, (12, 12))],
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
        .id_numbered(107)
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
        16, /* death_immunity */
        10, /* cold_immunity */
        52, /* poison_immunity */
        31, /* illusion_immunity */
    ] {
        assert!(raised.has(crate::game::testutil::ability(a)), "{a}");
        assert!(!living.has(crate::game::testutil::ability(a)), "{a}");
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
        .id_numbered(107)
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
    // Swordsmen carry large shields. Longbowmen shoot at them, and cavalry strikes them.
    let shot = real_battle(
        &[(101 /* high_elf_longbowmen */, (3, 10))],
        &[(105 /* high_men_swordsmen */, (5, 10))],
        1,
    );
    let open = damage_over_seeds(&without(&shot, 1, 34 /* large_shield */), true);
    let shielded = damage_over_seeds(&shot, true);
    assert!(shielded * 4 < open * 3, "{shielded} vs {open}");
    // Two more dice of defence do not touch a melee blow.
    let blow = real_battle(
        &[(CAVALRY, (3, 10))],
        &[(105 /* high_men_swordsmen */, (4, 10))],
        1,
    );
    let open = damage_over_seeds(&without(&blow, 1, 34 /* large_shield */), false);
    let shielded = damage_over_seeds(&blow, false);
    assert_eq!(open, shielded);
}

#[test]
fn a_spell_caster_shoots_with_its_mana_and_mana_leak_costs_a_shot() -> TestResult {
    // An efreet casts fireballs: twenty mana, three a shot.
    let mut b = real_battle(&[(162 /* efreet */, (3, 10))], &[(SPEARMEN, (4, 10))], 1);
    let (mana, ammo) = (b.units[0].mana, b.units[0].ammo);
    assert!(mana >= 20 && ammo > 2);
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .map_err(|e| format!("{e:?}"))?;
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!((u.mana, u.ammo), (mana - 3, ammo - 1), "three mana a shot");
    // Mana leak takes five mana, which is another shot and a half.
    b.units[0].leak_mana(5);
    let u = b.unit(0).ok_or("unit is missing")?;
    assert_eq!((u.mana, u.ammo), (mana - 8, ammo - 2));
    // With no mana left it cannot shoot.
    b.units[0].leak_mana(1000);
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
    plain.leak_mana(5);
    assert_eq!((plain.mana, plain.ammo), (0, shots - 1));
    plain.ranged_kind = RangedKind::Missile;
    plain.leak_mana(5);
    assert_eq!(plain.ammo, shots - 1, "arrows are not spell shots");
    Ok(())
}

#[test]
fn regenerating_units_heal_a_hit_as_their_sides_turn_begins() -> TestResult {
    // A troll regenerates.
    let mut b = real_battle(
        &[(SPEARMEN, (3, 10))],
        &[(147 /* troll_spearmen */, (4, 10))],
        1,
    );
    b.units[1].front_damage = 2;
    let before = b.unit(1).ok_or("unit is missing")?.hit_points();
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), before + 1);
    // Not on the other side's turn.
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(1).ok_or("unit is missing")?.hit_points(), before + 1);
    // The Regeneration spell grants the same ability.
    let regeneration = 31;
    let mut g = wizard_battle(SPEARMEN, &[regeneration]);
    cast_on_first(&mut g, regeneration).map_err(|e| format!("{e:?}"))?;
    let mut b = g.battle().ok_or("no battle")?.clone();
    assert!(b.units[0].has(Ability::new(55)));
    b.units[0].front_damage = 1;
    b.units[0].moves_left = HalfInt::from_int(1);
    let before = b.unit(0).ok_or("unit is missing")?.hit_points();
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    b.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.unit(0).ok_or("unit is missing")?.hit_points(), before + 1);
    Ok(())
}

#[test]
fn luck_adds_one_to_hit_to_block_and_resistance_in_battle() -> TestResult {
    use crate::rules::Stat;
    let mut g = game();
    // The halfling swordsmen are Lucky.
    let lucky_kind = g
        .content
        .rules
        .units
        .id_numbered(90)
        .ok_or("halfling_swordsmen is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), lucky_kind);
    let d = g.spawn_unit(D, at(5, 4), lucky_kind);
    let (to_hit, resistance) = (
        g.units().unit_stat(a, Stat::ToHit),
        g.units().unit_stat(a, Stat::Resistance),
    );
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    let lucky = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    assert_eq!(lucky.to_hit, to_hit + 1);
    assert_eq!(lucky.to_block, 1);
    assert_eq!(lucky.resistance, resistance + 1);
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
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let troll = g
        .content
        .rules
        .units
        .id_numbered(152)
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
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let troll = g
        .content
        .rules
        .units
        .id_numbered(152)
        .ok_or("troll_war_trolls is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), troll);
    let d = g.spawn_unit(D, at(5, 4), walker);
    finish_with_dead(&mut g, &[a], &[d], &[0]);
    assert!(!g.world.contains(a));
    Ok(())
}

#[test]
fn the_winners_survivors_gain_experience_and_fame_for_four_kills() -> TestResult {
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
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
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..2).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    finish_with_dead(&mut g, &[a], &foes, &[1, 2]);
    assert_eq!(g.world.get::<&crate::unit::Experience>(a)?.0, 4);
    assert_eq!(g.wizard(A).ok_or("wizard is missing")?.fame(), 0);
    Ok(())
}

/// A battle of one cavalry of player A against `foes` of player D (unit numbers), all the foes
/// dead, with D's fame at 5. Returns the fame of A and D afterwards.
fn fame_after_killing(foes: &[u16]) -> Result<(i32, i32), String> {
    let mut g = game();
    let walker = g.content.rules.units.id_numbered(107).ok_or("no cavalry")?;
    let a = g.spawn_unit(A, at(4, 4), walker);
    let mut ids = Vec::new();
    for &num in foes {
        let kind = g.content.rules.units.id_numbered(num).ok_or("no foe")?;
        ids.push(g.spawn_unit(D, at(5, 4), kind));
    }
    g.set_fame(D, 5);
    let dead: Vec<usize> = (1..=foes.len()).collect();
    finish_with_dead(&mut g, &[a], &ids, &dead);
    let fame = |p| {
        g.wizard(p)
            .map(|w| w.fame())
            .ok_or("wizard is missing".to_string())
    };
    Ok((fame(A)?, fame(D)?))
}

#[test]
fn a_very_rare_fantastic_foe_is_worth_a_point_of_fame_and_costs_its_owner_one() -> TestResult {
    // The Great Drake is summoned for 900 mana.
    assert_eq!(fame_after_killing(&[164])?, (1, 4));
    // The Hydra, at 650, is rare too; the Sky Drake, the Demon Lord as well.
    assert_eq!(fame_after_killing(&[163])?, (1, 4));
    Ok(())
}

#[test]
fn a_fantastic_foe_below_the_rare_cost_and_a_normal_foe_give_no_fame() -> TestResult {
    // A catapult (fantastic, 100) and a cavalry (normal, 200).
    assert_eq!(fame_after_killing(&[37])?, (0, 5));
    assert_eq!(fame_after_killing(&[107])?, (0, 5));
    Ok(())
}

#[test]
fn the_loser_loses_the_point_the_winner_gets_for_four_dead_units() -> TestResult {
    assert_eq!(fame_after_killing(&[107, 107, 107])?, (0, 5));
    assert_eq!(fame_after_killing(&[107, 107, 107, 107])?, (1, 4));
    // A rare foe among the four earns and costs both points.
    assert_eq!(fame_after_killing(&[164, 107, 107, 107])?, (2, 3));
    Ok(())
}

#[test]
fn a_fallen_hero_costs_its_owner_half_its_level_rounded_down_counting_from_one() -> TestResult {
    // (xp, fame lost): Hero 0, Myrmidon 20 and Captain 60 lose 0, 1 and 1 (by the wiki's level
    // count from 1, halved and rounded down), Commander 120 and Champion 200 lose 2.
    for (xp, lost) in [(0, 0), (20, 1), (60, 1), (120, 2), (200, 2)] {
        let mut g = game();
        let walker = g.content.rules.units.id_numbered(107).ok_or("no cavalry")?;
        let hero = g.content.rules.units.id_numbered(0).ok_or("no dwarf")?;
        let a = g.spawn_unit(A, at(4, 4), walker);
        let d = g.spawn_unit(D, at(5, 4), hero);
        g.world.insert_one(d, crate::unit::Experience(xp))?;
        g.set_fame(D, 5);
        finish_with_dead(&mut g, &[a], &[d], &[1]);
        assert_eq!(
            g.wizard(D).ok_or("wizard is missing")?.fame(),
            5 - lost,
            "xp {xp}"
        );
    }
    // A human loser with a lot of fame loses no more than the rules say.
    let mut g = game();
    let walker = g.content.rules.units.id_numbered(107).ok_or("no cavalry")?;
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    let a = g.spawn_unit(A, at(4, 4), walker);
    g.set_fame(D, 25);
    g.set_human(Some(D));
    finish_with_dead(&mut g, &[a], &foes, &[1, 2, 3, 4]);
    assert_eq!(g.wizard(D).ok_or("wizard is missing")?.fame(), 24);
    Ok(())
}

#[test]
fn healing_cannot_bring_back_what_was_destroyed() {
    // Four troll figures.
    let mut u = unit_numbered(147 /* troll_spearmen */);
    let (figures, h) = (u.figures_start, u.hits);
    assert_eq!(u.figures, figures);
    // One figure lost to destruction, one to ordinary blows.
    u.figures = figures - 2;
    u.record(attack::Damage {
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
        (gained, u.figures, u.extra_hits),
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
    assert_eq!(u.extra_hits, 2);
    assert_eq!(u.hit_points(), max + 4);
    assert_eq!(u.max_hit_points(), max, "the real maximum stays");
}

#[test]
fn a_vortex_crushes_the_unit_it_lands_on_and_is_steered_once_a_turn_by_its_owner() -> TestResult {
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (11, 10))], 5);
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
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 8);
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
    let mut g = hook_game(105 /* magic_vortex */);
    let enemy_cell = g
        .battle()
        .ok_or("battle is missing")?
        .unit(1)
        .ok_or("unit is missing")?
        .cell;
    assert_eq!(
        cast_unit(
            &mut g,
            105, /* magic_vortex */
            SpellAim::Tile(enemy_cell),
            0
        ),
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NoVortexRoom
        )))
    );
    let events = cast_unit(
        &mut g,
        105, /* magic_vortex */
        SpellAim::Tile((10, 10)),
        0,
    )
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
        .id_numbered(107)
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

const NIGHT_STALKER: u16 = 167; // invisible

#[test]
fn an_invisible_unit_is_unseen_and_cannot_be_aimed_at_until_it_gives_itself_away() {
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(NIGHT_STALKER, (17, 10))], 1);
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
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(NIGHT_STALKER, (5, 10))], 1);
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
    let b = real_battle(&[(SKELETONS, (3, 10))], &[(NIGHT_STALKER, (17, 10))], 1);
    assert!(
        b.seen_by(Side::Attacker, 1),
        "a unit immune to illusions sees it"
    );
    Ok(())
}

#[test]
fn invisibility_that_begins_in_battle_hides_the_unit_at_once() {
    let mut b = real_battle(&[(SPEARMEN, (3, 10))], &[(SPEARMEN, (17, 10))], 1);
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

/// Dispel Magic with 1500 mana against the enemy's Darkness (difficulty 25) and the caster's own
/// Prayer, after `burn` rolls of the game's dice: whether Darkness came down, and whether Prayer
/// is still up.
fn dispel_the_field(burn: usize) -> (bool, bool) {
    use crate::battle::field::Field;
    let mut g = game();
    for _ in 0..burn {
        g.next_u64();
    }
    g.treasury_mut(A).expect("treasury_mut succeeds").mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(A, &[(RealmId::new(1 /* sorcery */), 5)], &[], None)
        .expect("init_wizard succeeds");
    let dispel = g
        .content
        .rules
        .spells
        .id_numbered(202)
        .expect("id_of succeeds");
    let darkness = g
        .content
        .rules
        .spells
        .id_numbered(169)
        .expect("id_of succeeds");
    let prayer = g
        .content
        .rules
        .spells
        .id_numbered(140)
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
        .id_numbered(107)
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

/// 1.50 note 742: the defender's Wall of Fire and Wall of Darkness are battlefield spells that a
/// strong dispel takes down.
#[test]
fn dispel_magic_takes_down_the_defenders_walls_of_fire_and_darkness() -> TestResult {
    let mut downs = 0;
    for burn in 0..8 {
        let mut g = wizard_battle(107, &[202 /* dispel_magic */]);
        for _ in 0..burn {
            g.next_u64();
        }
        {
            let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
            b.set_caster(
                Side::Attacker,
                crate::battle::BattleCaster::new(2000, 2000, 10),
            );
            b.wall_of_fire = true;
            b.wall_of_darkness = true;
        }
        let spell = g.content.rules.spells.id_numbered(202).ok_or("dispel")?;
        g.battle_command(BattleCommand::Cast {
            spell,
            aim: SpellAim::Unit(1),
            mana: 1500,
        })
        .map_err(|e| format!("{e:?}"))?;
        let b = g.battle().ok_or("no battle")?;
        downs += usize::from(!b.wall_of_fire && !b.wall_of_darkness);
    }
    assert!(downs >= 6, "{downs} of 8 took both walls down");
    Ok(())
}

/// A battle of player A's wizard, who knows `spells` and has plenty to cast with; the first
/// attacker is a `unit` of the test content.
fn wizard_battle(unit: u16, spells: &[u16]) -> Game {
    wizard_battle_against(unit, spells, 1)
}

/// [`wizard_battle`] against `foes` cavalry of player D.
fn wizard_battle_against(unit: u16, spells: &[u16], foes: usize) -> Game {
    wizard_battle_versus(unit, spells, 107, foes)
}

/// [`wizard_battle`] against `foes` units of the kind `foe` (a unit number) of player D.
fn wizard_battle_versus(unit: u16, spells: &[u16], foe: u16, foes: usize) -> Game {
    let mut g = game();
    g.treasury_mut(A).expect("treasury_mut succeeds").mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(
        A,
        &[
            (RealmId::new(0 /* nature */), 5),
            (RealmId::new(1 /* sorcery */), 5),
            (RealmId::new(2 /* chaos */), 5),
            (RealmId::new(3 /* life */), 5),
            (RealmId::new(4 /* death */), 5),
        ],
        &[],
        None,
    )
    .expect("value succeeds");
    for name in spells {
        let id = g
            .content
            .rules
            .spells
            .id_numbered(*name)
            .expect("id_of succeeds");
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
    let kind = g
        .content
        .rules
        .units
        .id_numbered(unit)
        .expect("id_of succeeds");
    let ours = g.spawn_unit(A, at(4, 4), kind);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(foe)
        .expect("id_of succeeds");
    let defenders: Vec<_> = (0..foes)
        .map(|_| g.spawn_unit(D, at(5, 4), walker))
        .collect();
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders,
    });
    assert!(g.in_battle());
    g
}

/// The wizard casts `spell` on the first attacker, with a fresh turn's casting.
fn cast_on_first(g: &mut Game, spell: u16) -> Result<Vec<Event>, BattleError> {
    cast_on(g, spell, 0)
}

/// The wizard casts `spell` on unit `target`, with a fresh turn's casting.
fn cast_on(g: &mut Game, spell: u16, target: usize) -> Result<Vec<Event>, BattleError> {
    let id = g
        .content
        .rules
        .spells
        .id_numbered(spell)
        .expect("id_of succeeds");
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
        aim: SpellAim::Unit(target),
        mana: 0,
    })
}

/// 1.50 notes 447, 451, 495: a battle turn changes hands only by `EndTurn`, whether or not a
/// unit can still move. A side whose units are all spent (or held still) keeps its turn, and its
/// wizard still casts; ending the turn once passes it once, with no turn lost on either side.
#[test]
fn a_side_whose_units_cannot_move_keeps_its_turn_and_its_casting() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[4 /* giant_strength */]);
    {
        let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
        for u in b.units.iter_mut().filter(|u| u.side == Side::Attacker) {
            u.moves_left = HalfInt::ZERO;
            u.attacked = true;
        }
        assert_eq!((b.active, b.turn), (Side::Attacker, 1));
    }
    // Nothing is left for the units to do, and the turn is still the attacker's first.
    cast_on_first(&mut g, 4 /* giant_strength */).map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("no battle")?;
    assert_eq!((b.active, b.turn), (Side::Attacker, 1));
    // One EndTurn: the defender's turn of the same round, with its units' full moves.
    g.battle_command(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("no battle")?;
    assert_eq!((b.active, b.turn), (Side::Defender, 1));
    assert!(b
        .active_units(Side::Defender)
        .all(|u| u.moves_left == u.moves));
    // And one more brings the attacker's second round.
    g.battle_command(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("no battle")?;
    assert_eq!((b.active, b.turn), (Side::Attacker, 2));
    Ok(())
}

/// 1.50 note 458: a ship on land (in a city) is no fighter unless it flies or is non-corporeal
/// (Flight, Wraith Form); a plain ship cannot move there and stays out of the battle.
#[test]
fn a_ship_in_a_city_fights_only_when_flight_or_wraith_form_lets_it_move_on_land() -> TestResult {
    let fighters = |power: Option<u16>| -> Result<usize, String> {
        let mut g = game();
        g.spawn_city(D, at(5, 4), "Port".into(), None, 3);
        let trireme = g
            .content
            .rules
            .units
            .id_numbered(35)
            .ok_or("trireme is not in the content")?;
        let ship = g.spawn_unit(D, at(5, 4), trireme);
        let guard = g.spawn_unit(
            D,
            at(5, 4),
            g.content
                .rules
                .units
                .id_numbered(107)
                .ok_or("cavalry is not in the content")?,
        );
        if let Some(a) = power {
            g.grant_temp_ability(
                crate::rules::exec::Target::Unit(ship),
                crate::rules::Ability::new(a),
            );
        }
        let attacker = g.spawn_unit(
            A,
            at(4, 4),
            g.content
                .rules
                .units
                .id_numbered(107)
                .ok_or("cavalry is not in the content")?,
        );
        g.apply(Command::BeginBattle {
            attackers: vec![attacker],
            defenders: vec![guard, ship],
        });
        let b = g.battle().ok_or("no battle")?;
        Ok(b.active_units(Side::Defender).count())
    };
    assert_eq!(fighters(None)?, 1, "a plain ship sits out");
    // A city held by nothing but a plain ship has no one to fight: the battle still starts and
    // the ship is lost with the city.
    let mut g = game();
    g.spawn_city(D, at(5, 4), "Port".into(), None, 3);
    let ship = g.spawn_unit(
        D,
        at(5, 4),
        g.content
            .rules
            .units
            .id_numbered(35)
            .ok_or("trireme is not in the content")?,
    );
    let attacker = g.spawn_unit(
        A,
        at(4, 4),
        g.content
            .rules
            .units
            .id_numbered(107)
            .ok_or("cavalry is not in the content")?,
    );
    let events = g.auto_resolve(&[attacker], &[ship]);
    assert!(!g.in_battle(), "{events:?}");
    assert_eq!(
        g.economy()
            .city_at(at(5, 4))
            .and_then(|c| g.economy().city_owner(c)),
        Some(A),
        "the attacker takes the port"
    );
    Ok(())
}

/// 1.50 notes 456, 457: a fight on the sea lays each side's ship as a deck and the land units
/// aboard start on it (a Floating Island is a ship that carries its stack the same way), where the
/// battle starts; the water between the decks is open to ships, never to walkers.
#[test]
fn the_land_units_of_a_ship_start_a_sea_fight_on_its_deck() -> TestResult {
    use crate::unit::transport::Aboard;
    let mut g = game();
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .ok_or("ocean is not in the content")?;
    for x in 4..=6 {
        g.map
            .as_mut()
            .ok_or("no map")?
            .plane_mut(Plane::Arcanus)
            .set(x, 4, ocean);
    }
    let kind = |g: &Game, n: u16| {
        g.content
            .rules
            .units
            .id_numbered(n)
            .ok_or_else(|| format!("unit {n} is not in the content"))
    };
    let side = |g: &mut Game, owner, x| -> Result<(hecs::Entity, hecs::Entity), String> {
        let ship = g.spawn_unit(owner, at(x, 4), kind(g, 35 /* trireme */)?);
        let walker = g.spawn_unit(owner, at(x, 4), kind(g, 107 /* high_men_cavalry */)?);
        let serial = g.units().serial_of(ship);
        crate::game::put_in(&mut g.world, walker, Aboard(serial));
        Ok((ship, walker))
    };
    let (ship_a, walker_a) = side(&mut g, A, 4)?;
    let (ship_d, walker_d) = side(&mut g, D, 5)?;
    g.apply(Command::BeginBattle {
        attackers: vec![ship_a, walker_a],
        defenders: vec![ship_d, walker_d],
    });
    let b = g.battle().ok_or("no battle")?;
    assert_eq!(b.units.len(), 4, "both ships and both walkers fight");
    for u in b.units.iter().filter(|u| !u.sails) {
        assert!(
            b.map.is_ship_deck(u.cell),
            "{:?} stands on its deck",
            u.cell
        );
        assert!(
            b.map.enter_cost_as(u.cell, u.mover()).is_some(),
            "and can walk there"
        );
    }
    Ok(())
}

/// 1.50 notes 655, 656: a dispel cannot be aimed at a dead unit, and Earth to Mud cannot be aimed at
/// a square outside the field (a corner square or past the edge), so it makes no mud where nobody
/// may walk.
#[test]
fn spells_are_not_aimed_at_the_dead_or_at_squares_outside_the_field() -> TestResult {
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[202 /* dispel_magic */, 1 /* earth_to_mud */],
    );
    let dead = {
        let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
        let id = b
            .units
            .iter()
            .position(|u| u.side == Side::Defender)
            .ok_or("no defender")?;
        b.units[id].status = Status::Dead;
        b.units[id].figures = 0;
        id
    };
    assert!(
        cast_on(&mut g, 202, dead).is_err(),
        "no dispel on a dead unit"
    );
    let mud = g
        .content
        .rules
        .spells
        .id_numbered(1)
        .ok_or("earth_to_mud is not in the content")?;
    assert!(
        g.battle_command(BattleCommand::Cast {
            spell: mud,
            aim: SpellAim::Tile((10, 10)),
            mana: 0,
        })
        .is_ok(),
        "a square of the field is a fine target"
    );
    for cell in [(0, 0), (20, 0), (21, 5), (5, 22), (-1, 5), (100, 100)] {
        let r = g.battle_command(BattleCommand::Cast {
            spell: mud,
            aim: SpellAim::Tile(cell),
            mana: 0,
        });
        assert!(r.is_err(), "{cell:?} is outside the field");
    }
    Ok(())
}

/// 1.50 notes 541, 687: the walls of a city without citizens (an outpost) stand in its battles.
#[test]
fn an_outpost_with_city_walls_is_fought_round_its_walls() -> TestResult {
    let mut g = game();
    let city = g.spawn_city(D, at(5, 4), "Outpost".into(), None, 0);
    g.world
        .get::<&mut crate::city::economy::Buildings>(city)
        .map_err(|e| format!("{e:?}"))?
        .0
        .push(crate::rules::BuildingId::new(35 /* city_walls */));
    let kind = |g: &Game, n: u16| {
        g.content
            .rules
            .units
            .id_numbered(n)
            .ok_or_else(|| format!("unit {n} is not in the content"))
    };
    let guard = g.spawn_unit(D, at(5, 4), kind(&g, 104)?);
    let attacker = g.spawn_unit(A, at(4, 4), kind(&g, 107)?);
    g.apply(Command::BeginBattle {
        attackers: vec![attacker],
        defenders: vec![guard],
    });
    let b = g.battle().ok_or("no battle")?;
    assert!(b.map.is_walled(), "the logic has its walls");
    assert!(
        b.map.has_wall((12, 11)) || (0..21).any(|x| (0..22).any(|y| b.map.has_wall((x, y)))),
        "and a wall stands somewhere"
    );
    Ok(())
}

/// 1.50 note 638: Weakness takes two from a thrown attack as it does from blows.
#[test]
fn weakness_weakens_a_thrown_attack_too() -> TestResult {
    let mut g = wizard_battle_versus(
        107, /* high_men_cavalry */
        &[162 /* weakness */],
        45, /* barbarian_berserkers, thrown 3 */
        1,
    );
    let foe = |g: &Game| -> Result<(i32, i32), String> {
        let b = g.battle().ok_or("no battle")?;
        let u = b
            .units
            .iter()
            .find(|u| u.side == Side::Defender)
            .ok_or("no defender")?;
        Ok((u.melee, u.ranged))
    };
    let (melee, ranged) = foe(&g)?;
    assert!(ranged >= 3, "the berserkers throw: {ranged}");
    // The save is a die roll: cast until it takes.
    for _ in 0..40 {
        let id = g
            .battle()
            .and_then(|b| {
                b.units
                    .iter()
                    .find(|u| u.side == Side::Defender)
                    .map(|u| u.id)
            })
            .ok_or("no defender")?;
        let _ = cast_on(&mut g, 162, id);
        if foe(&g)?.0 < melee {
            break;
        }
        g.battle.as_mut().ok_or("no battle")?.battle.casters[0] =
            Some(crate::battle::BattleCaster::new(2000, 2000, 10));
    }
    let (m2, r2) = foe(&g)?;
    assert_eq!(
        (melee - m2, ranged - r2),
        (2, 2),
        "melee and thrown both fall by two"
    );
    Ok(())
}

#[test]
fn a_lasting_spell_is_cast_on_a_unit_once() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[4 /* giant_strength */]);
    cast_on_first(&mut g, 4 /* giant_strength */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, 4 /* giant_strength */),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );
    Ok(())
}

#[test]
fn stone_skin_gives_way_to_iron_skin_in_a_battle_too() -> TestResult {
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[7 /* stone_skin */, 24 /* iron_skin */],
    );
    let defense = |g: &Game| {
        g.battle()
            .expect("battle succeeds")
            .unit(0)
            .expect("unit succeeds")
            .defense
    };
    let base = defense(&g);
    cast_on_first(&mut g, 7 /* stone_skin */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(defense(&g), base + 1);
    // Iron Skin on top leaves its five, not six, and Stone Skin cannot follow it.
    cast_on_first(&mut g, 24 /* iron_skin */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(defense(&g), base + 5);
    assert_eq!(
        cast_on_first(&mut g, 7 /* stone_skin */),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );

    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[7 /* stone_skin */, 24 /* iron_skin */],
    );
    cast_on_first(&mut g, 24 /* iron_skin */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, 7 /* stone_skin */),
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
            107, /* high_men_cavalry */
            &[
                7,  /* stone_skin */
                24, /* iron_skin */
                4,  /* giant_strength */
            ],
        );
        let base = g
            .battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?
            .defense;
        for spell in [
            7,  /* stone_skin */
            24, /* iron_skin */
            4,  /* giant_strength */
        ] {
            cast_on_first(&mut g, spell).map_err(|e| format!("{e:?}"))?;
        }
        g.dispel_battle_unit(0, strength, D);
        let unit = g
            .battle()
            .ok_or("battle is missing")?
            .unit(0)
            .ok_or("unit is missing")?;
        let has = |id: SpellId| unit.enchantments.contains(&id);
        let expected = if has(SpellId::new(24) /* iron_skin */) {
            5
        } else if has(SpellId::new(7) /* stone_skin */) {
            1
        } else {
            0
        };
        assert_eq!(unit.defense, base + expected, "strength {strength}");
    }
    Ok(())
}

/// A unit's numbers as the battle reads them: what a spell can change.
#[derive(Debug, PartialEq)]
struct Numbers {
    melee: i32,
    ranged: i32,
    defense: i32,
    resistance: i32,
    to_hit: i32,
    to_block: i32,
    hits: i32,
    moves: HalfInt,
    flags: [bool; 6],
    special_level: i32,
    abilities: Vec<String>,
    granted: Vec<String>,
    effects: crate::spell::unit_effect::UnitEffects,
    enchantments: Vec<String>,
    web_hits: i32,
    warp: Option<String>,
}

fn numbers(u: &BattleUnit) -> Numbers {
    let sorted = |list: &[crate::rules::Ability]| {
        let mut v: Vec<String> = list.iter().map(|a| format!("{a:?}")).collect();
        v.sort();
        v
    };
    let mut enchantments: Vec<String> = u.enchantments.iter().map(|s| format!("{s:?}")).collect();
    enchantments.sort();
    Numbers {
        melee: u.melee,
        ranged: u.ranged,
        defense: u.defense,
        resistance: u.resistance,
        to_hit: u.to_hit,
        to_block: u.to_block,
        hits: u.hits,
        moves: u.moves,
        flags: [
            u.flies,
            u.first_strike,
            u.negate_first_strike,
            u.long_range,
            u.magical_weapon,
            u.extra_attack,
        ],
        special_level: u.special_level,
        abilities: sorted(&u.abilities),
        granted: sorted(&u.granted),
        effects: u.effects,
        enchantments,
        web_hits: u.web_hits,
        warp: u.warp.map(|w| format!("{w:?}")),
    }
}

/// The lasting spells that can be cast on a unit of one's own and change its numbers, flags or
/// abilities: the matrix of the model tests of task 604.
const OWN_UNIT_SPELLS: [u16; 15] = [
    4,   // giant_strength
    7,   // stone_skin
    24,  // iron_skin
    31,  // regeneration
    56,  // flight
    62,  // invisibility
    70,  // haste
    94,  // flame_blade
    121, // bless
    124, // holy_weapon
    126, // holy_armor
    130, // heroism
    141, // lionheart
    143, // invulnerability
    179, // berserk
];

/// A battle in which a `unit` has had `casts` cast on it in turn; the casts that were refused
/// (a spell it has by nature, one that gives way) are left out.
fn unit_with_spells(unit: u16, casts: &[u16]) -> Result<Game, Box<dyn std::error::Error>> {
    let mut g = wizard_battle(unit, &OWN_UNIT_SPELLS);
    for &spell in casts {
        let _ = cast_on_first(&mut g, spell);
    }
    Ok(g)
}

/// Task 604, the model: whatever a dispel takes off a unit, what is left is the unit with only
/// the spells that remain cast on it, in the same order. (The numbers are a function of the unit
/// and its spells, not of the history.)
#[test]
fn a_dispel_leaves_exactly_the_unit_with_the_spells_that_remain_cast_on_it() -> TestResult {
    let mut checked = 0;
    let mut partial = 0;
    let mut failures = Vec::new();
    for unit in [
        107, /* high_men_cavalry */
        104, /* high_men_spearmen */
    ] {
        for (i, &a) in OWN_UNIT_SPELLS.iter().enumerate() {
            for (j, &b) in OWN_UNIT_SPELLS.iter().enumerate().skip(i + 1) {
                for &c in OWN_UNIT_SPELLS.iter().skip(j + 1).step_by(3) {
                    for strength in [25, 60] {
                        let casts = [a, b, c];
                        let mut g = unit_with_spells(unit, &casts)?;
                        let all = g
                            .battle()
                            .ok_or("no battle")?
                            .unit(0)
                            .ok_or("no unit")?
                            .combat_spells()
                            .len();
                        g.dispel_battle_unit(0, strength, D);
                        let after = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
                        let kept: Vec<u16> =
                            after.combat_spells().iter().map(|(s, _)| s.num()).collect();
                        if kept.len() < all && !kept.is_empty() {
                            partial += 1;
                        }
                        let fresh = unit_with_spells(unit, &kept)?;
                        let want = numbers(
                            fresh
                                .battle()
                                .ok_or("no battle")?
                                .unit(0)
                                .ok_or("no unit")?,
                        );
                        checked += 1;
                        let got = numbers(after);
                        if got != want {
                            failures.push(format!(
                                "unit {unit}, cast {casts:?}, strength {strength}, kept {kept:?}:\n   got  {got:?}\n   want {want:?}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        partial > 20,
        "too few partial dispels to mean anything: {partial} of {checked}"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} differ, the first:\n{}",
        failures.len(),
        failures
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

/// The curses of the matrix: each is cast on the enemy unit until it takes.
const CURSES: [u16; 6] = [
    162, // weakness
    53,  // vertigo
    67,  // mind_storm
    88,  // shatter
    165, // black_sleep
    5,   // web
];

/// A battle in which the enemy unit has had the `casts` cast on it in turn by the first wizard.
/// A curse the unit resists is cast again until it takes.
fn enemy_with_curses(casts: &[u16]) -> Result<Game, Box<dyn std::error::Error>> {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &CURSES);
    for &spell in casts {
        let landed = |g: &Game| -> Result<bool, Box<dyn std::error::Error>> {
            let u = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
            Ok(u.combat_spells().iter().any(|(s, _)| s.num() == spell))
        };
        let mut tries = 0;
        while !landed(&g)? {
            tries += 1;
            if tries > 200 {
                return Err(format!("curse {spell} never took in 200 casts").into());
            }
            let _ = cast_on(&mut g, spell, 1);
        }
    }
    Ok(g)
}

/// The same invariant for what the enemy does to a unit: a dispel leaves the unit with only the
/// curses that remain on it, whichever they are (task 604).
#[test]
fn a_dispel_leaves_exactly_the_enemy_unit_with_the_curses_that_remain_on_it() -> TestResult {
    let (mut checked, mut partial) = (0, 0);
    let mut failures = Vec::new();
    for (i, &a) in CURSES.iter().enumerate() {
        for (j, &b) in CURSES.iter().enumerate().skip(i + 1) {
            for &c in CURSES.iter().skip(j + 1) {
                for strength in [25, 60, 10_000] {
                    let casts = [a, b, c];
                    let mut g = enemy_with_curses(&casts)?;
                    let all = g
                        .battle()
                        .ok_or("no battle")?
                        .unit(1)
                        .ok_or("no unit")?
                        .combat_spells()
                        .len();
                    g.dispel_battle_unit(1, strength, D);
                    let after = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
                    let kept: Vec<u16> =
                        after.combat_spells().iter().map(|(s, _)| s.num()).collect();
                    partial += usize::from(kept.len() < all && !kept.is_empty());
                    let fresh = enemy_with_curses(&kept)?;
                    let want = numbers(
                        fresh
                            .battle()
                            .ok_or("no battle")?
                            .unit(1)
                            .ok_or("no unit")?,
                    );
                    checked += 1;
                    let got = numbers(after);
                    if got != want {
                        failures.push(format!("cast {casts:?}, strength {strength}, kept {kept:?}:\n   got  {got:?}\n   want {want:?}"));
                    }
                }
            }
        }
    }
    assert!(
        partial >= 5,
        "too few partial dispels: {partial} of {checked}"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} differ, the first:\n{}",
        failures.len(),
        failures
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

#[test]
fn a_creature_binding_that_is_dispelled_returns_the_unit_and_takes_only_its_own_effect(
) -> TestResult {
    let mut g = wizard_battle_against(
        107, /* high_men_cavalry */
        &[73 /* creature_binding */, 162 /* weakness */],
        2,
    );
    g.battle.as_mut().ok_or("no battle")?.battle.units[1].resistance = 0;
    g.battle.as_mut().ok_or("no battle")?.battle.units[1].realm = Some(Realm::Chaos);
    let pristine = numbers(g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?);
    cast_on(&mut g, 162 /* weakness */, 1).map_err(|e| format!("{e:?}"))?;
    cast_on(&mut g, 73 /* creature_binding */, 1).map_err(|e| format!("{e:?}"))?;
    let bound = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
    assert_eq!(
        (bound.side, bound.bound_from),
        (Side::Attacker, Some(Side::Defender))
    );
    let weakened = numbers(bound);
    assert!(weakened.melee < pristine.melee);
    // Dice: look at every throw until the binding alone is gone and weakness stays, and at the
    // throw that takes both.
    let mut one_of_them = false;
    for strength in 1..400 {
        let mut h = wizard_battle_against(107, &[73, 162], 2);
        h.battle.as_mut().ok_or("no battle")?.battle.units[1].resistance = 0;
        h.battle.as_mut().ok_or("no battle")?.battle.units[1].realm = Some(Realm::Chaos);
        cast_on(&mut h, 162, 1).map_err(|e| format!("{e:?}"))?;
        cast_on(&mut h, 73, 1).map_err(|e| format!("{e:?}"))?;
        h.dispel_battle_unit(1, strength, D);
        let u = h.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
        let has = |n: u16| u.combat_spells().iter().any(|(s, _)| s.num() == n);
        // The side follows the binding, the numbers follow weakness.
        assert_eq!(u.side == Side::Attacker, has(73), "strength {strength}");
        assert_eq!(u.bound_from.is_some(), has(73));
        assert_eq!(
            u.effects
                .contains(crate::spell::unit_effect::UnitEffect::CreatureBinding),
            has(73)
        );
        assert_eq!(u.melee < pristine.melee, has(162), "strength {strength}");
        one_of_them |= has(73) != has(162);
    }
    assert!(one_of_them, "the dice never took just one of the two");
    Ok(())
}

#[test]
fn a_warp_creature_that_is_dispelled_takes_its_warp_and_its_numbers_with_it() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[89 /* warp_creature */]);
    let pristine = numbers(g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?);
    // The unit may resist: cast again until the warp takes.
    for _ in 0..200 {
        if g.battle()
            .ok_or("no battle")?
            .unit(1)
            .ok_or("no unit")?
            .warp
            .is_some()
        {
            break;
        }
        let _ = cast_on(&mut g, 89 /* warp_creature */, 1);
    }
    let warped = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
    assert!(warped.warp.is_some());
    assert_ne!(numbers(warped), pristine);
    g.dispel_battle_unit(1, 10_000, D);
    let after = g.battle().ok_or("no battle")?.unit(1).ok_or("no unit")?;
    assert_eq!(numbers(after), pristine);
    assert!(after.warp.is_none() && after.combat_spells().is_empty());
    Ok(())
}

/// A spell the unit had before the battle (on the overland unit) goes through the same dispel:
/// the unit is then as one that never had it.
#[test]
fn a_dispel_of_a_spell_from_before_the_battle_leaves_the_unit_as_if_it_never_had_it() -> TestResult
{
    let build = |lionheart: bool| -> Result<Game, Box<dyn std::error::Error>> {
        let mut g = game();
        g.treasury_mut(A).ok_or("no treasury")?.mana = f4x_def::half_int::HalfInt::from_int(2000);
        g.init_wizard(A, &[(RealmId::new(3 /* life */), 5)], &[], None)
            .map_err(|e| format!("{e:?}"))?;
        let walker = g.content.rules.units.id_numbered(107).ok_or("no cavalry")?;
        let ours = g.spawn_unit(A, at(4, 4), walker);
        if lionheart {
            let spell = g
                .content
                .rules
                .spells
                .id_numbered(141)
                .ok_or("no lionheart")?;
            g.attach_enchantment(crate::rules::exec::Target::Unit(ours), spell, A);
        }
        begin(&mut g, ours);
        Ok(g)
    };
    let mut g = build(true)?;
    let plain = numbers(
        build(false)?
            .battle()
            .ok_or("no battle")?
            .unit(0)
            .ok_or("no unit")?,
    );
    let with = numbers(g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?);
    assert!(
        with.hits > plain.hits && with.melee > plain.melee,
        "{with:?} against {plain:?}"
    );
    g.dispel_battle_unit(0, 10_000, D);
    let after = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert_eq!(numbers(after), plain);
    Ok(())
}

/// Hit points a battle-cast Lionheart gave go with it at the end of the battle, and the damage
/// the unit took stays (`divergences.md` 45): a unit that stands on them is dead, one that does
/// not is hurt by what it took.
#[test]
fn a_lionheart_cast_in_the_battle_ends_with_it_and_the_damage_stays() -> TestResult {
    for (damage_on_top, lives) in [(0, true), (1, false)] {
        let mut g = wizard_battle(107 /* high_men_cavalry */, &[141 /* lionheart */]);
        cast_on_first(&mut g, 141).map_err(|e| format!("{e:?}"))?;
        let (base, max, ours) = {
            let a = g.battle.as_ref().ok_or("no battle")?;
            let u = a.battle.unit(0).ok_or("no unit")?;
            let base = u.base.clone().ok_or("no base")?.hits * u.figures_start as i32;
            (base, u.max_hit_points(), a.sources[0])
        };
        assert!(max > base, "lionheart gave hit points: {max} over {base}");
        // The damage that leaves the unit on one hit point of its normal maximum, or none.
        let taken = base - 1 + damage_on_top;
        {
            let u = &mut g.battle.as_mut().ok_or("no battle")?.battle.units[0];
            let left = max - taken;
            u.figures = ((left + u.hits - 1) / u.hits) as u32;
            u.front_damage = u.figures as i32 * u.hits - left;
            assert_eq!(u.hit_points(), left);
        }
        // The other side flees, so ours wins and nothing is lost on the way.
        g.battle_command(BattleCommand::EndTurn)
            .map_err(|e| format!("{e:?}"))?;
        g.battle_command(BattleCommand::Flee)
            .map_err(|e| format!("{e:?}"))?;
        assert!(!g.in_battle());
        if lives {
            assert_eq!(g.units().unit_hits(ours).0, 1);
        } else {
            assert!(
                !g.world.contains(ours),
                "no hit points are left, so it is dead"
            );
        }
    }
    Ok(())
}

/// Life steal, then a spell that gave hit points is dispelled: the extras stay, the spell's hit
/// points go, and the damage taken is kept.
#[test]
fn life_steal_extras_stay_and_lionheart_goes_when_it_is_dispelled() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[141 /* lionheart */]);
    cast_on_first(&mut g, 141).map_err(|e| format!("{e:?}"))?;
    let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    let base_max = u.base.clone().ok_or("no base")?.hits * u.figures_start as i32;
    let with_spell = u.max_hit_points();
    // Hurt by three, then healed past the maximum by life steal.
    {
        let u = &mut g.battle.as_mut().ok_or("no battle")?.battle.units[0];
        u.front_damage = 3;
        u.heal_points(100, true);
        assert!(u.extra_hits > 0 && u.hit_points() > with_spell);
    }
    let (extra, figures) = {
        let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
        (u.extra_hits, u.figures_start as i32)
    };
    g.dispel_battle_unit(0, 10_000, D);
    let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert_eq!(u.extra_hits, extra, "the extras are no spell's");
    assert_eq!(
        u.max_hit_points(),
        base_max,
        "the spell's hit points are gone"
    );
    assert_eq!(u.hits, base_max / figures + extra);
    assert!(u.is_active());
    assert_eq!(
        u.hit_points(),
        base_max + figures * extra,
        "it was whole with the extras, so it is whole with them still"
    );
    Ok(())
}

/// Haste lets a unit attack twice in a turn; a dispel that takes the Haste off takes the second
/// attack with it (the original, and this code before task 604, left it).
#[test]
fn a_dispelled_haste_takes_the_second_attack_with_it() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[70 /* haste */]);
    cast_on_first(&mut g, 70).map_err(|e| format!("{e:?}"))?;
    assert!(
        g.battle()
            .ok_or("no battle")?
            .unit(0)
            .ok_or("no unit")?
            .extra_attack
    );
    g.dispel_battle_unit(0, 10_000, D);
    let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert!(!u.has(Ability::new(26)));
    assert!(
        !u.extra_attack,
        "the haste is gone, the second attack is not"
    );
    Ok(())
}

/// A game in a battle, the first unit's melee and maximum hit points before its spell.
type Overhealed = (Game, i32, i32);

/// A battle of death knights (who have life steal) with Giant Strength on the first, against
/// `foes` cavalry, fought turn after turn until a blow has healed that knight past its maximum.
/// `None` if the fight ended or the knight fell before that: the dice decide, so the caller
/// fights again.
fn knights_overhealed_by_life_steal(
    foes: usize,
) -> Result<Option<Overhealed>, Box<dyn std::error::Error>> {
    let mut g = wizard_battle_versus(
        172, /* death_knights */
        &[4 /* giant_strength */],
        107, /* high_men_cavalry */
        foes,
    );
    let (plain_melee, plain_max) = {
        let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
        (u.melee, u.max_hit_points())
    };
    cast_on_first(&mut g, 4 /* giant_strength */).map_err(|e| format!("{e:?}"))?;
    for _ in 0..40 {
        let Some(b) = g.battle() else {
            return Ok(None);
        };
        match b.unit(0) {
            Some(u) if u.is_active() && u.extra_hits > 0 => {
                return Ok(Some((g, plain_melee, plain_max)))
            }
            Some(u) if !u.is_active() => return Ok(None),
            _ => {}
        }
        let side = b.active;
        let ids: Vec<usize> = b.active_units(side).map(|u| u.id).collect();
        for id in ids {
            let Some(b) = g.battle() else {
                return Ok(None);
            };
            let Some((target, cell)) = b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            else {
                break;
            };
            let step = b
                .reachable(id)
                .into_iter()
                .min_by_key(|(p, _)| crate::battle::map::distance(*p, cell))
                .map(|(p, _)| p);
            if let Some(to) = step {
                g.apply(Command::Battle(BattleCommand::Move { unit: id, to }));
            }
            g.apply(Command::Battle(BattleCommand::Attack { unit: id, target }));
        }
        g.apply(Command::Battle(BattleCommand::EndTurn));
    }
    Ok(None)
}

/// The extra hit points life steal gave are no spell's: the dispel of an unrelated spell leaves
/// them, and the unit's maximum stays what it was (before task 604 the extras were lost and the
/// maximum went negative, which healed the unit fully when the battle was written back).
#[test]
fn life_steal_hit_points_survive_the_dispel_of_an_unrelated_spell() -> TestResult {
    let mut found = None;
    for _ in 0..40 {
        found = knights_overhealed_by_life_steal(6)?;
        if found.is_some() {
            break;
        }
    }
    let (mut g, plain_melee, plain_max) =
        found.ok_or("life steal never overhealed in 40 fights")?;
    let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert!(u.melee > plain_melee, "giant strength is on");
    assert_eq!(
        u.max_hit_points(),
        plain_max,
        "this spell gives no hit points"
    );
    let (extra, hp) = (u.extra_hits, u.hit_points());
    assert!(hp > plain_max, "{hp} of {plain_max}");
    g.dispel_battle_unit(0, 10_000, D);
    let u = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert_eq!(u.melee, plain_melee, "the dispel took Giant Strength off");
    assert_eq!(u.extra_hits, extra, "the extras are no spell's");
    assert_eq!(u.max_hit_points(), plain_max);
    assert_eq!(u.hit_points(), hp);
    Ok(())
}

#[test]
fn a_dispel_in_a_battle_tells_the_world_which_spell_went_and_whose_it_was() -> TestResult {
    use crate::game::{DispelSite, Event};
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[7 /* stone_skin */]);
    cast_on_first(&mut g, 7 /* stone_skin */).map_err(|e| format!("{e:?}"))?;
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
    let mut g = wizard_battle(
        154, /* magic_spirit */
        &[181 /* wraith_form */, 56 /* flight */],
    );
    assert_eq!(
        cast_on_first(&mut g, 181 /* wraith_form */),
        Err(BattleError::Spell(SpellError::AlreadyAble))
    );
    assert!(cast_on_first(&mut g, 56 /* flight */).is_ok());
}

#[test]
fn holy_weapon_is_refused_in_a_battle_while_the_wizard_keeps_holy_arms() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[124 /* holy_weapon */]);
    let arms = g
        .content
        .rules
        .spells
        .id_numbered(154)
        .ok_or("holy_arms is not in the content")?;
    g.add_global(A, arms);
    assert_eq!(
        cast_on_first(&mut g, 124 /* holy_weapon */),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    Ok(())
}

#[test]
fn spells_the_battle_reads_by_name_can_be_cast_and_stay_on_the_unit() -> TestResult {
    let names = [
        121, /* bless */
        144, /* righteousness */
        41,  /* resist_magic */
        54,  /* spell_lock */
    ];
    let mut g = wizard_battle(107 /* high_men_cavalry */, &names);
    for name in names {
        cast_on_first(&mut g, name).map_err(|e| format!("{e:?}"))?;
    }
    let unit = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?;
    let spells = &g.content.rules.spells;
    assert!(names.iter().all(|n| spells
        .id_numbered(*n)
        .is_some_and(|id| unit.enchantments.contains(&id))));
    assert_eq!(
        cast_on_first(&mut g, 121 /* bless */),
        Err(BattleError::Spell(SpellError::AlreadyEnchanted))
    );
    Ok(())
}

#[test]
fn the_resistance_pairs_are_refused_in_a_battle() -> TestResult {
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[
            2,  /* resist_elements */
            21, /* elemental_armor */
            41, /* resist_magic */
            69, /* magic_immunity */
        ],
    );
    cast_on_first(&mut g, 21 /* elemental_armor */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, 2 /* resist_elements */),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[2 /* resist_elements */, 21 /* elemental_armor */],
    );
    cast_on_first(&mut g, 2 /* resist_elements */).map_err(|e| format!("{e:?}"))?;
    // The stronger one goes on top.
    cast_on_first(&mut g, 21 /* elemental_armor */).map_err(|e| format!("{e:?}"))?;
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[41 /* resist_magic */, 69 /* magic_immunity */],
    );
    cast_on_first(&mut g, 69 /* magic_immunity */).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        cast_on_first(&mut g, 41 /* resist_magic */),
        Err(BattleError::Spell(SpellError::NotCumulative))
    );
    Ok(())
}

#[test]
fn heroism_cast_in_a_battle_raises_the_unit_to_the_level_it_gives() -> TestResult {
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[130 /* heroism */]);
    let melee = g
        .battle()
        .ok_or("battle is missing")?
        .unit(0)
        .ok_or("unit is missing")?
        .melee;
    cast_on_first(&mut g, 130 /* heroism */).map_err(|e| format!("{e:?}"))?;
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
    let has = unit.enchantments.contains(&SpellId::new(130) /* heroism */);
    assert_eq!(unit.melee, melee + i32::from(has));
    Ok(())
}

#[test]
fn a_battle_is_saved_and_read_back_with_its_spells_and_items() -> TestResult {
    let mut g = game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = charged_wand();
    g.arm_hero(hero, [Some(wand), None, None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_numbered(42)
        .ok_or("dispel_magic_true is not in the content")?;
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
    let mut g = game();
    let hero = hero_unit(&mut g, &[], 0);
    let wand = charged_wand();
    g.arm_hero(hero, [Some(wand), None, None]);
    begin(&mut g, hero);
    let zap = g
        .content
        .rules
        .spells
        .id_numbered(42)
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
    let mut g = wizard_battle(107 /* high_men_cavalry */, &[]);
    g.treasury_mut(D).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(
        D,
        &[
            (RealmId::new(0 /* nature */), 5),
            (RealmId::new(2 /* chaos */), 5),
            (RealmId::new(4 /* death */), 5),
        ],
        &[],
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    for name in [
        3,   /* wall_of_stone */
        87,  /* wall_of_fire */
        178, /* wall_of_darkness */
    ] {
        let id = g
            .content
            .rules
            .spells
            .id_numbered(name)
            .ok_or("id_of is missing")?;
        g.wizard_mut(D)
            .ok_or("wizard_mut is missing")?
            .get_mut::<crate::wizard::sheet::KnownSpells>()
            .known
            .insert(id);
    }
    let cast = |g: &mut Game, name: u16, side: Side| {
        let spell = g
            .content
            .rules
            .spells
            .id_numbered(name)
            .expect("id_of succeeds");
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
    assert!(cast(&mut g, 3 /* wall_of_stone */, Side::Defender).is_err());
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.map = BattleMap::open().with_city(false);
    assert!(g
        .battle()
        .ok_or("battle is missing")?
        .map
        .standing_walls()
        .is_empty());
    // The attacker cannot raise one, the defender can, once.
    assert!(cast(&mut g, 3 /* wall_of_stone */, Side::Attacker).is_err());
    let events =
        cast(&mut g, 3 /* wall_of_stone */, Side::Defender).map_err(|e| format!("{e:?}"))?;
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::WallRaised { .. }))));
    assert!(g.battle().ok_or("battle is missing")?.map.is_fully_walled());
    assert_eq!(
        cast(&mut g, 3 /* wall_of_stone */, Side::Defender),
        Err(BattleError::Spell(SpellError::AlreadyUp))
    );
    // Fire and Darkness are flags of the battle.
    cast(&mut g, 87 /* wall_of_fire */, Side::Defender).map_err(|e| format!("{e:?}"))?;
    cast(&mut g, 178 /* wall_of_darkness */, Side::Defender).map_err(|e| format!("{e:?}"))?;
    let b = g.battle().ok_or("battle is missing")?;
    assert!(b.wall_of_fire && b.wall_of_darkness);
    Ok(())
}

#[test]
fn the_default_limit_is_the_lower_of_the_casting_skill_and_half_the_reserve() -> TestResult {
    let mut g = game();
    g.init_wizard(
        A,
        &[
            (RealmId::new(0 /* nature */), 5),
            (RealmId::new(3 /* life */), 5),
            (RealmId::new(4 /* death */), 5),
        ],
        &[],
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(500);
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
        .id_numbered(4)
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
    let skill = g
        .wizard(A)
        .ok_or("wizard is missing")?
        .casting_skill(&g.content.rules) as u32;
    assert_eq!(g.strategic_mana_default(A), Some(skill.clamp(1, 250)));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(10);
    assert_eq!(g.strategic_mana_default(A), Some(skill.clamp(1, 5)));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(1);
    assert_eq!(g.strategic_mana_default(A), Some(1));
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(0);
    assert_eq!(g.strategic_mana_default(A), None);
    Ok(())
}

#[test]
fn a_hasted_unit_attacks_twice_in_a_turn() -> TestResult {
    let haste = 70;
    let mut g = wizard_battle(SPEARMEN, &[haste]);
    cast_on_first(&mut g, haste).map_err(|e| format!("{e:?}"))?;
    let mut b = g.battle().ok_or("no battle")?.clone();
    b.units[0].cell = (9, 10);
    b.units[1].cell = (10, 10);
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
            .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
        .id_numbered(107)
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
    let mut g = game();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
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

#[test]
fn the_legal_unit_casts_are_the_castable_spells_with_the_units_they_can_hit() -> TestResult {
    let mut g = game();
    let hero = hero_unit(&mut g, &[83 /* fire_bolt */, 125 /* healing */], 2);
    begin(&mut g, hero);
    let bolt = g
        .content
        .rules
        .spells
        .id_numbered(83)
        .ok_or("dispel_magic_true is not in the content")?;
    // 7 mana pay the Fire Bolt (5), not Healing (15).
    let legal = g.legal_unit_casts();
    assert_eq!(legal.len(), 1);
    let (unit, cast) = &legal[0];
    assert_eq!((*unit, cast.spell, cast.least_mana), (0, bolt, 5));
    assert_eq!(cast.aims, [SpellAim::Unit(1)], "only the enemy unit");
    Ok(())
}

#[test]
fn a_curse_the_target_is_immune_to_is_refused_before_it_costs_mana() -> TestResult {
    let mut g = hook_game(46 /* confusion */);
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1]
        .abilities
        .push(crate::game::testutil::ability(
            31, /* illusion_immunity */
        ));
    let refused = cast_unit(&mut g, 46 /* confusion */, SpellAim::Unit(1), 10);
    assert_eq!(
        refused,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NotAffected
        )))
    );
    Ok(())
}

#[test]
fn warp_wood_asks_for_a_missile_attack_and_not_for_a_kind_of_unit() -> TestResult {
    let mut g = hook_game(81 /* warp_wood */);
    {
        let u = &mut g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1];
        u.realm = Some(Realm::Nature);
        u.ranged_kind = crate::battle::RangedKind::Missile;
        u.ammo = 4;
    }
    let fantastic = cast_unit(&mut g, 81 /* warp_wood */, SpellAim::Unit(1), 10);
    assert!(
        fantastic.is_ok(),
        "a fantastic archer can be warped: {fantastic:?}"
    );
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1].ammo = 0;
    let empty = cast_unit(&mut g, 81 /* warp_wood */, SpellAim::Unit(1), 10);
    assert_eq!(
        empty,
        Err(BattleError::Spell(SpellError::Target(
            TargetError::NeedsMissile
        )))
    );
    Ok(())
}

#[test]
fn warp_creature_takes_any_unit_that_is_not_warped_already() -> TestResult {
    let mut g = hook_game(89 /* warp_creature */);
    g.battle.as_mut().ok_or("as_mut is missing")?.battle.units[1].realm = Some(Realm::Nature);
    let fantastic = cast_unit(&mut g, 89 /* warp_creature */, SpellAim::Unit(1), 10);
    assert!(
        !matches!(
            fantastic,
            Err(BattleError::Spell(SpellError::Target(
                TargetError::NotNormalUnit
            )))
        ),
        "{fantastic:?}"
    );
    Ok(())
}

#[test]
fn the_three_combat_bolts_follow_the_wiki() {
    use f4x_def::rules::effects::Effect;
    let c = crate::game::testutil::shipped();
    let spell = |num: u16| {
        c.rules
            .spells
            .get(c.rules.spells.id_numbered(num).expect("shipped"))
    };
    let damage = |num: u16| {
        spell(num)
            .effects
            .iter()
            .find_map(|r| match &r.effect {
                Effect::Damage {
                    amount,
                    resistible,
                    traits,
                    ..
                } => Some((*amount, *resistible, *traits)),
                _ => None,
            })
            .expect("a damage effect")
    };
    // (number, cost, research cost, strength): the wiki's figures.
    for (num, cost, research, strength) in [
        (104 /* doom_bolt */, 40, 1_120, 10),
        (122 /* star_fires */, 5, 40, 15),
        (101 /* warp_lightning */, 35, 880, 10),
    ] {
        let def = spell(num);
        assert_eq!((def.cost, def.research_cost), (cost, research), "{num}");
        assert_eq!(
            def.contexts,
            vec![f4x_def::rules::CastContext::Battle],
            "{num}: combat only"
        );
        let (amount, resistible, _) = damage(num);
        assert_eq!(amount, strength, "{num}");
        assert!(!resistible, "{num}: defence rolls, no resistance roll");
    }
    assert!(damage(104).2.ignores_defense, "doom bolt: no defence");
    let (_, _, lightning) = damage(101);
    assert!(
        lightning.armor_piercing && lightning.decaying,
        "ten weaker bolts, half defence"
    );
}

/// Insecticide IW107/IW108: Raise Dead and Animate Dead. A hero knowing both spells and two
/// cavalry of its army fight one defender; the second cavalry (battle unit 2) is the one that
/// falls. Returns the game and the cavalry's overland unit.
fn necromancy_battle() -> Result<(Game, hecs::Entity), String> {
    let mut g = game();
    let hero = hero_unit(&mut g, &[137 /* raise_dead */, 195 /* animate_dead */], 4);
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let cavalry = g.spawn_unit(A, at(4, 4), walker);
    let foe = g.spawn_unit(D, at(5, 4), walker);
    g.apply(Command::BeginBattle {
        attackers: vec![hero, cavalry],
        defenders: vec![foe],
    });
    assert!(g.in_battle());
    let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
    b.units[0].mana = 1000;
    Ok((g, cavalry))
}

/// Kill the second attacker (the cavalry) in the way `kind` says and return its battle id.
fn fallen_cavalry(g: &mut Game, kind: Fate) -> Result<usize, String> {
    let b = &mut g.battle.as_mut().ok_or("no battle")?.battle;
    let id = b
        .units
        .iter()
        .position(|u| u.side == Side::Attacker && u.id != 0)
        .ok_or("no second attacker")?;
    let u = &mut b.units[id];
    u.status = Status::Dead;
    u.figures = 0;
    u.damage_kinds = match kind {
        Fate::Slain => [5, 0, 0],
        Fate::Drained => [0, 5, 0],
        Fate::Gone => [0, 0, 5],
    };
    Ok(id)
}

fn necromancy_events(events: &[Event]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Battle(b) => Some(b),
            _ => None,
        })
        .collect()
}

#[test]
fn raise_dead_brings_a_fallen_soldier_back_with_half_its_figures_on_its_own_side() -> TestResult {
    let (mut g, cavalry) = necromancy_battle()?;
    let dead = fallen_cavalry(&mut g, Fate::Slain)?;
    // A lasting spell it carried is gone with its death.
    g.battle.as_mut().ok_or("no battle")?.battle.units[dead]
        .enchantments
        .push(SpellId::new(0));
    let start = g.battle().ok_or("no battle")?.units[dead].figures_start;
    let events = cast_unit(&mut g, 137 /* raise_dead */, SpellAim::Unit(dead), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(necromancy_events(&events)
        .iter()
        .any(|e| matches!(e, BattleEvent::Raised { unit } if *unit == dead)));
    let u = g.battle().ok_or("no battle")?.unit(dead).ok_or("no unit")?;
    assert!(u.is_active());
    assert_eq!(
        u.side,
        Side::Attacker,
        "it fights for the wizard that raised it"
    );
    assert_eq!(u.figures, start.div_ceil(2).max(1));
    assert!(!u.undead);
    assert!(u.enchantments.is_empty());
    assert_eq!(
        g.units().unit_owner(cavalry),
        Some(A),
        "the owner is the same"
    );
    Ok(())
}

#[test]
fn animate_dead_raises_a_fallen_soldier_as_undead_at_full_strength() -> TestResult {
    let (mut g, cavalry) = necromancy_battle()?;
    let dead = fallen_cavalry(&mut g, Fate::Slain)?;
    let start = g.battle().ok_or("no battle")?.units[dead].figures_start;
    let events = cast_unit(&mut g, 195 /* animate_dead */, SpellAim::Unit(dead), 0)
        .map_err(|e| format!("{e:?}"))?;
    assert!(necromancy_events(&events)
        .iter()
        .any(|e| matches!(e, BattleEvent::Raised { unit } if *unit == dead)));
    let u = g.battle().ok_or("no battle")?.unit(dead).ok_or("no unit")?;
    assert!(u.is_active() && u.undead);
    assert_eq!(u.side, Side::Attacker);
    assert_eq!(u.figures, start, "an animated unit has all its figures");
    assert_eq!(
        g.units().unit_owner(cavalry),
        Some(A),
        "the caster owns the undead"
    );
    assert!(
        g.units().unit_is_undead(cavalry),
        "overland it is undead for good"
    );
    Ok(())
}

/// 1.50 notes 496, 498: Death Immunity keeps a fallen unit from rising as undead, by Animate Dead,
/// Raise Dead or the winner's Zombie Mastery.
#[test]
fn a_fallen_unit_with_death_immunity_is_not_raised_as_undead() -> TestResult {
    let (mut g, _) = necromancy_battle()?;
    let dead = fallen_cavalry(&mut g, Fate::Slain)?;
    g.battle.as_mut().ok_or("no battle")?.battle.units[dead]
        .abilities
        .push(Ability::new(16 /* death_immunity */));
    for spell in [137 /* raise_dead */, 195 /* animate_dead */] {
        let refused = cast_unit(&mut g, spell, SpellAim::Unit(dead), 0);
        assert!(refused.is_err(), "spell {spell}: death immunity");
    }
    let b = g.battle().ok_or("no battle")?;
    assert!(!b.unit(dead).ok_or("no unit")?.is_active(), "still dead");
    Ok(())
}

#[test]
fn the_dead_that_cannot_be_raised_are_refused_without_spending_anything() -> TestResult {
    // A living unit, a unit destroyed outright, and an enemy's dead are no targets of either.
    let (mut g, _) = necromancy_battle()?;
    let alive = g
        .battle()
        .ok_or("no battle")?
        .units
        .iter()
        .position(|u| u.side == Side::Attacker && u.id != 0)
        .ok_or("no second attacker")?;
    let gone = fallen_cavalry(&mut g, Fate::Gone)?;
    assert_eq!(gone, alive);
    for spell in [137 /* raise_dead */, 195 /* animate_dead */] {
        let refused = cast_unit(&mut g, spell, SpellAim::Unit(gone), 0);
        assert!(refused.is_err(), "spell {spell}: destroyed outright");
    }
    // The enemy's fallen unit is not ours to raise.
    let foe = g
        .battle()
        .ok_or("no battle")?
        .units
        .iter()
        .position(|u| u.side == Side::Defender)
        .ok_or("no defender")?;
    {
        let u = &mut g.battle.as_mut().ok_or("no battle")?.battle.units[foe];
        u.status = Status::Dead;
        u.figures = 0;
    }
    for spell in [137 /* raise_dead */, 195 /* animate_dead */] {
        let refused = cast_unit(&mut g, spell, SpellAim::Unit(foe), 0);
        assert!(refused.is_err(), "spell {spell}: the enemy's dead");
    }
    // Nothing came back to life.
    let b = g.battle().ok_or("no battle")?;
    assert!(!b.unit(gone).ok_or("no unit")?.is_active());
    assert!(!b.unit(foe).ok_or("no unit")?.is_active());
    Ok(())
}

#[test]
fn neither_spell_takes_a_unit_that_is_still_alive() -> TestResult {
    let (mut g, _) = necromancy_battle()?;
    let living = g
        .battle()
        .ok_or("no battle")?
        .units
        .iter()
        .position(|u| u.side == Side::Attacker && u.id != 0)
        .ok_or("no second attacker")?;
    for spell in [137 /* raise_dead */, 195 /* animate_dead */] {
        assert!(
            cast_unit(&mut g, spell, SpellAim::Unit(living), 0).is_err(),
            "spell {spell}"
        );
    }
    assert!(g
        .battle()
        .ok_or("no battle")?
        .unit(living)
        .ok_or("no unit")?
        .is_active());
    Ok(())
}

/// Wraith Form's two abilities (non-corporeal, weapon immunity) on half of the enemy units, for
/// `spell` cast over the battlefield: the units that have them and the others, with what the
/// spell did to each (a unit hurt or slain, or one reported immune).
fn cast_over_wraiths(
    spell: u16,
    victim: u16,
    seed: u64,
) -> Result<Vec<(bool, bool, bool)>, String> {
    let mut g = game_seeded(seed);
    g.treasury_mut(A).ok_or("treasury_mut is missing")?.mana =
        f4x_def::half_int::HalfInt::from_int(2000);
    g.init_wizard(
        A,
        &[
            (RealmId::new(2 /* chaos */), 5),
            (RealmId::new(3 /* life */), 5),
            (RealmId::new(4 /* death */), 5),
        ],
        &[],
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    let id = g
        .content
        .rules
        .spells
        .id_numbered(spell)
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
    }
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let victim = g
        .content
        .rules
        .units
        .id_numbered(victim)
        .ok_or("the victim is not in the content")?;
    let ours = g.spawn_unit(A, at(4, 4), walker);
    let theirs: Vec<_> = (0..9)
        .map(|i| {
            let unit = g.spawn_unit(D, at(5, 4), victim);
            if i % 2 == 0 {
                for ability in [49, 72] {
                    g.grant_temp_ability(
                        crate::rules::exec::Target::Unit(unit),
                        crate::rules::Ability::new(ability),
                    );
                }
            }
            unit
        })
        .collect();
    g.apply(Command::BeginBattle {
        attackers: vec![ours],
        defenders: theirs,
    });
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
            spell: id,
            aim: SpellAim::None,
            mana: 0,
        })
        .map_err(|e| format!("{e:?}"))?;
    let events: Vec<BattleEvent> = events
        .into_iter()
        .filter_map(|e| match e {
            Event::Battle(b) => Some(b),
            _ => None,
        })
        .collect();
    let battle = g.battle().ok_or("battle is missing")?;
    let units = battle
        .units
        .iter()
        .filter(|u| u.side == Side::Defender)
        .map(|u| {
            // A unit an immunity stops still gets a report, of damage that is nothing.
            let hit = |e: &BattleEvent| match e {
                BattleEvent::SpellDamaged {
                    unit, dealt, kills, ..
                } => *unit == u.id && (*dealt > 0 || *kills > 0),
                BattleEvent::SpellSlew { unit, kills } => *unit == u.id && *kills > 0,
                _ => false,
            };
            let immune =
                |e: &BattleEvent| matches!(e, BattleEvent::SpellImmune { unit } if *unit == u.id);
            (
                u.has(crate::rules::Ability::new(49)) && u.has(crate::rules::Ability::new(72)),
                events.iter().any(hit),
                events.iter().any(immune),
            )
        })
        .collect();
    Ok(units)
}

#[test]
fn wraith_form_is_no_shelter_from_flame_strike_death_spell_and_holy_word() -> TestResult {
    // The wiki lists these as bugs of the original (fixed in its patches): a unit under Wraith
    // Form was immune to them. Holy Word needs a creature of death or chaos.
    let content = shipped();
    let death = RealmId::new(4 /* death */);
    let undead = content
        .rules
        .units
        .defs()
        .iter()
        .find(|d| {
            d.kind == crate::rules::UnitKind::Summoned
                && d.realm == Some(death)
                && !d.abilities.contains(&crate::rules::Ability::new(49))
        })
        .ok_or("the shipped rules have no summoned creature of death that is not non-corporeal")?
        .num;
    for (spell, victim) in [
        (109 /* flame_strike */, 107 /* high_men_cavalry */),
        (194 /* death_spell */, 107),
        (150 /* holy_word */, undead),
    ] {
        // A battle holds nine units to a side: look at several, for the dice.
        let mut units = Vec::new();
        for seed in 0..8 {
            units.extend(cast_over_wraiths(spell, victim, seed)?);
        }
        let (wraiths, others): (Vec<_>, Vec<_>) = units.into_iter().partition(|u| u.0);
        assert_eq!(
            (wraiths.len(), others.len()),
            (40, 32),
            "spell {spell}: units with and without Wraith Form"
        );
        assert!(
            wraiths.iter().all(|&(_, _, immune)| !immune),
            "spell {spell}: a unit with Wraith Form was reported immune"
        );
        let touched = |units: &[(bool, bool, bool)]| units.iter().filter(|u| u.1).count();
        assert!(
            touched(&wraiths) > 0,
            "spell {spell}: no unit with Wraith Form was touched"
        );
        assert!(
            touched(&others) > 0,
            "spell {spell}: no other unit was touched either"
        );
    }
    Ok(())
}

/// Patch 1.50, note 159: every unit of the losing wizard that fell counts for his dislike of the
/// attacker, not one of them (a Great Drake kills eight spearmen; each is worth 1 to 20).
#[test]
fn the_dislike_a_battle_breeds_counts_every_unit_that_fell() -> TestResult {
    let mut g = game();
    let units = &g.content.rules.units;
    let drake = units
        .id_numbered(164)
        .ok_or("great_drake is not in the content")?;
    let spearman = units
        .id_numbered(104)
        .ok_or("high_men_spearmen is not in the content")?;
    let attacker = g.spawn_unit(A, at(4, 4), drake);
    let defenders: Vec<hecs::Entity> = (0..8)
        .map(|_| g.spawn_unit(D, at(5, 4), spearman))
        .collect();
    let before = g.relation(D, A);
    g.auto_resolve(&[attacker], &defenders);
    let fallen = defenders
        .iter()
        .filter(|&&d| g.world.get::<&crate::unit::Unit>(d).is_err())
        .count();
    assert!(fallen >= 6, "the drake wins ({fallen} fell)");
    // One unit is worth at most 20; the fallen of the whole stack are worth more than that.
    assert!(
        before - g.relation(D, A) > 20,
        "{before} to {}",
        g.relation(D, A)
    );
    Ok(())
}

/// Patch 1.50, note 177: a unit that resists Call Chaos's Disintegrate is not destroyed. The spell
/// is Disintegrate's own effect (the whole unit goes when its resistance is under 10), and a Great
/// Drake (resistance 12) shrugs it off while spearmen do not.
#[test]
fn disintegrate_as_call_chaos_throws_it_spares_the_unit_that_resists() -> TestResult {
    let mut b = real_battle(&[(164, (0, 0))], &[(SPEARMEN, (20, 20))], 1);
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

/// Patch 1.50, note 261: Invulnerability can be dispelled in a battle, and the unit goes back to
/// its own numbers.
#[test]
fn invulnerability_can_be_dispelled_in_a_battle() -> TestResult {
    let mut g = wizard_battle(
        107, /* high_men_cavalry */
        &[143 /* invulnerability */],
    );
    let pristine = numbers(g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?);
    cast_on(&mut g, 143 /* invulnerability */, 0).map_err(|e| format!("{e:?}"))?;
    let shielded = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert!(shielded.has(Ability::new(72)), "the spell took");
    g.dispel_battle_unit(0, 10_000, D);
    let after = g.battle().ok_or("no battle")?.unit(0).ok_or("no unit")?;
    assert!(!after.has(Ability::new(72)), "the shield is gone");
    assert_eq!(numbers(after), pristine);
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
