use super::game::BattleStartError;
use super::map::{distance, Cell, GRID_H, GRID_W};
use super::*;
use crate::city::PlayerId;
use crate::map::{Grid, Map, Plane};
use crate::testutil::content_with_units;
use crate::{Command, Event, Game, Rejection};

const A: PlayerId = PlayerId(0);
const D: PlayerId = PlayerId(1);

fn spearman(side: Side, cell: Pos) -> BattleUnit {
    BattleUnit::simple(side, "spearman", 3, 2, 3, 2).at(cell)
}

fn duel(att: BattleUnit, def: BattleUnit, seed: u64) -> Battle {
    Battle::new(A, D, BattleMap::open(), vec![att, def], seed)
}

#[test]
fn a_new_battle_starts_with_the_attacker_and_full_moves() {
    let b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        1,
    );
    assert_eq!(b.active, Side::Attacker);
    assert_eq!(b.turn, 1);
    assert_eq!(b.winner, None);
    assert_eq!(b.unit(0).unwrap().id, 0);
    assert_eq!(b.unit(1).unwrap().id, 1);
    assert_eq!(b.unit(0).unwrap().moves_left, b.unit(0).unwrap().moves);
    assert_eq!(b.player_of(Side::Attacker), A);
    assert_eq!(b.player_of(Side::Defender), D);
    assert_eq!(b.active_units(Side::Attacker).count(), 1);
    assert_eq!(b.unit_at((17, 10)).unwrap().id, 1);
    assert!(b.unit_at((9, 9)).is_none());
}

#[test]
fn units_walk_within_their_moves_and_not_through_others() {
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
        .unwrap();
    assert!(matches!(
        &events[0],
        BattleEvent::Moved { unit: 0, path } if path.last() == Some(&(5, 11)) && path.len() == 2
    ));
    assert_eq!(b.unit(0).unwrap().cell, (5, 11));
    assert_eq!(b.unit(0).unwrap().moves_left, 0);
    assert_eq!(
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (6, 11)
        }),
        Err(BattleError::NoMovesLeft)
    );
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
fn a_melee_attack_hurts_both_sides_and_uses_up_the_turn() {
    let mut b = duel(
        spearman(Side::Attacker, (9, 10)),
        spearman(Side::Defender, (10, 10)),
        3,
    );
    let events = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .unwrap();
    let BattleEvent::Attacked { dealt, taken, .. } = events[0] else {
        panic!("{events:?}");
    };
    assert_eq!(b.unit(1).unwrap().hit_points(), 6 - dealt);
    assert_eq!(b.unit(0).unwrap().hit_points(), 6 - taken);
    assert!(b.unit(0).unwrap().attacked);
    assert_eq!(b.unit(0).unwrap().moves_left, 0);
    assert_eq!(
        b.apply(BattleCommand::Attack { unit: 0, target: 1 }),
        Err(BattleError::AlreadyAttacked)
    );
}

#[test]
fn killing_the_last_enemy_wins_and_ends_the_battle() {
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
                b.apply(BattleCommand::EndTurn).unwrap();
                b.apply(BattleCommand::EndTurn).unwrap();
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!(ended, Some(Side::Attacker));
    assert_eq!(b.winner, Some(Side::Attacker));
    assert!(!b.unit(1).unwrap().is_active());
    assert_eq!(b.apply(BattleCommand::EndTurn), Err(BattleError::Over));
}

#[test]
fn turns_alternate_and_moves_come_back() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        1,
    );
    b.apply(BattleCommand::Move {
        unit: 0,
        to: (5, 10),
    })
    .unwrap();
    assert_eq!(b.unit(0).unwrap().moves_left, 0);
    let e = b.apply(BattleCommand::EndTurn).unwrap();
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
    .unwrap();
    let e = b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(
        e,
        vec![BattleEvent::TurnStarted {
            side: Side::Attacker,
            turn: 2
        }]
    );
    assert_eq!(b.turn, 2);
    assert_eq!(
        b.unit(0).unwrap().moves_left,
        b.unit(0).unwrap().moves,
        "refreshed"
    );
}

#[test]
fn fleeing_hands_the_battle_to_the_other_side() {
    let mut b = duel(
        spearman(Side::Attacker, (3, 10)),
        spearman(Side::Defender, (17, 10)),
        1,
    );
    let e = b.apply(BattleCommand::Flee).unwrap();
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
    assert_eq!(b.unit(0).unwrap().status, Status::Fled);
}

#[test]
fn archers_shoot_with_ammunition_and_range_costs_accuracy() {
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
            .unwrap()[0]
        else {
            panic!()
        };
        let BattleEvent::Shot { dealt: f, .. } = far
            .apply(BattleCommand::Shoot { unit: 0, target: 1 })
            .unwrap()[0]
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
        .unwrap();
    assert_eq!(b.unit(0).unwrap().ammo, 1);
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
}

#[test]
fn a_walker_cannot_reach_a_flier_but_the_flier_hits_back() {
    let walker = BattleUnit::simple(Side::Attacker, "walker", 4, 2, 6, 1).at((9, 10));
    let mut flier = BattleUnit::simple(Side::Defender, "bat", 4, 2, 6, 1).at((10, 10));
    flier.flies = true;
    let mut b = duel(walker, flier, 8);
    let e = b
        .apply(BattleCommand::Attack { unit: 0, target: 1 })
        .unwrap();
    let BattleEvent::Attacked { dealt, taken, .. } = e[0] else {
        panic!()
    };
    assert_eq!(dealt, 0, "melee cannot reach a flier");
    assert!(taken > 0 || b.unit(0).unwrap().hit_points() == 8);
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
fn a_stalemate_ends_at_the_turn_limit_in_the_defenders_favour() {
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
            let me = b.unit(id).unwrap().clone();
            let Some(enemy) = b
                .active_units(side.other())
                .min_by_key(|e| distance(me.cell, e.cell))
                .map(|e| e.id)
            else {
                break;
            };
            if distance(me.cell, b.unit(enemy).unwrap().cell) > 1 {
                // Step to the reachable square nearest the enemy.
                let target = b.unit(enemy).unwrap().cell;
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
                if b.unit(enemy).unwrap().is_active()
                    && distance(b.unit(id).unwrap().cell, cell) == 1
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
            b.apply(BattleCommand::EndTurn).unwrap();
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
    let c = content_with_units();
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(5);
    g.content = c;
    g.map = Some(Map::new(Grid::new(20, 10, grass), Grid::new(20, 10, grass)));
    g
}

fn at(x: u32, y: u32) -> crate::city::Position {
    crate::city::Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

#[test]
fn a_game_battle_copies_the_units_and_writes_the_results_back() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
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
    let b = g.battle().unwrap();
    assert_eq!(b.units.len(), 3);
    // Statistics come from the overland unit: melee 3, defence 2, 2 hit points a figure, 3 figures.
    let u = b.unit(0).unwrap();
    assert_eq!((u.melee, u.defense, u.hits, u.figures), (3, 2, 2, 3));
    assert_eq!(u.moves, g.unit_max_moves(a1));
    assert_eq!(u.cell, (3, 11));
    assert_eq!(b.unit(2).unwrap().cell, (GRID_W - 4, 11));
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
            let b = g.battle().unwrap();
            (b.active, b.active_units(b.active).map(|u| u.id).collect())
        };
        for id in ids {
            let target = {
                let b = g.battle().unwrap();
                b.active_units(side.other()).next().map(|t| (t.id, t.cell))
            };
            let Some((t, cell)) = target else { break };
            // Walk towards the target, then attack.
            let step = {
                let b = g.battle().unwrap();
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
    let alive = |u| g.world.get::<&crate::units::Unit>(u).is_ok();
    assert!(alive(d1) != (alive(a1) || alive(a2)) || (alive(d1) && (alive(a1) || alive(a2))));
    for u in [a1, a2, d1] {
        if alive(u) {
            let (hp, max) = g.unit_hits(u);
            assert!(hp >= 1 && hp <= max, "hit points {hp}/{max}");
        }
    }
}

#[test]
fn battles_need_two_sides_of_opposing_players_standing_together() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let a_far = g.spawn_unit(A, at(8, 8), walker);
    let mine = g.spawn_unit(A, at(5, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    let reject = |g: &mut Game, att: Vec<hecs::Entity>, def: Vec<hecs::Entity>| {
        g.apply(Command::BeginBattle {
            attackers: att,
            defenders: def,
        })
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
}

#[test]
fn wounded_units_enter_a_battle_wounded() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    // 3 figures x 2 hit points: five damage leaves one figure with one hit point.
    g.world.insert_one(a, crate::units::Hurt(5)).unwrap();
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    let u = g.battle().unwrap().unit(0).unwrap();
    assert_eq!((u.figures, u.hit_points()), (1, 1));
    // Fleeing ends it at once, and the wound is still there afterwards (unless the unit was
    // one of the half that is lost on the way).
    let ev = g.apply(Command::Battle(BattleCommand::Flee));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::BattleEnded { winner, .. } if *winner == D)));
    assert!(!g.world.contains(a) || g.unit_hits(a).0 == 1);
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

fn found(g: &mut Game, owner: PlayerId, pos: crate::city::Position) -> hecs::Entity {
    let race = g.content.rules.races.id_of("elf");
    g.apply(Command::FoundCity {
        owner,
        pos,
        name: "Town".into(),
        race,
        population: 3,
    });
    g.city_at(pos).unwrap()
}

#[test]
fn auto_resolve_fights_to_the_end_and_the_winner_takes_the_square() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let attackers: Vec<_> = (0..4).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let defender = g.spawn_unit(D, at(5, 4), walker);
    let events = g.apply(Command::AutoResolve {
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
        assert_eq!(g.unit_position(u), Some(at(5, 4)), "moved onto the square");
    }
    // Another battle can start afterwards.
    assert!(g.battle().is_none());
}

#[test]
fn a_battle_won_at_a_city_captures_it() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let city = found(&mut g, D, at(5, 4));
    let defender = g.spawn_unit(D, at(5, 4), walker);
    let attackers: Vec<_> = (0..5).map(|_| g.spawn_unit(A, at(4, 4), walker)).collect();
    let events = g.apply(Command::AutoResolve {
        attackers,
        defenders: vec![defender],
    });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CityCaptured { to, .. } if *to == A)),
        "{events:?}"
    );
    assert_eq!(g.city_owner(city), Some(A));
}

#[test]
fn an_empty_enemy_city_is_taken_by_walking_in() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
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
    assert_eq!(g.city_owner(city), Some(A));
    assert_eq!(g.unit_position(unit), Some(at(5, 4)));
}

#[test]
fn a_losing_attacker_stays_where_it_was() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let attacker = g.spawn_unit(A, at(4, 4), walker);
    let defenders: Vec<_> = (0..6).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.apply(Command::AutoResolve {
        attackers: vec![attacker],
        defenders: defenders.clone(),
    });
    assert!(!g.world.contains(attacker));
    assert!(defenders.iter().any(|&d| g.world.contains(d)));
    for &d in defenders.iter().filter(|&&d| g.world.contains(d)) {
        assert_eq!(g.unit_position(d), Some(at(5, 4)));
    }
}

#[test]
fn auto_resolve_needs_an_adjacent_target() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(2, 4), walker);
    let d = g.spawn_unit(D, at(6, 4), walker);
    let events = g.apply(Command::AutoResolve {
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
}

fn instant_unit(side: Side, cell: Pos, ability: &str) -> BattleUnit {
    let mut u = spearman(side, cell);
    u.abilities = vec![ability.into()];
    u
}

#[test]
fn teleporting_and_merging_units_go_anywhere_free_for_one_move() {
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
            .unwrap();
        assert!(matches!(&events[0], BattleEvent::Moved { path, .. } if path == &vec![(16, 10)]));
        assert_eq!(b.unit(0).unwrap().cell, (16, 10));
        assert_eq!(b.unit(0).unwrap().moves_left, 2, "one of two moves used");
        // It may teleport again with what is left, and then has nothing left.
        b.apply(BattleCommand::Move {
            unit: 0,
            to: (16, 5),
        })
        .unwrap();
        assert_eq!(b.unit(0).unwrap().moves_left, 0);
        assert_eq!(
            b.apply(BattleCommand::Move {
                unit: 0,
                to: (3, 3)
            }),
            Err(BattleError::NoMovesLeft)
        );
    }
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
        b.apply(command).unwrap();
        total += b.unit(1).unwrap().max_hit_points() - b.unit(1).unwrap().hit_points();
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
        let mut u = BattleUnit::simple(Side::Attacker, "mage", 3, 1, 1, 1).at((3, 10));
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
fn a_defender_with_a_gaze_stones_the_attackers_before_they_strike() {
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
            .unwrap();
        lost += 10 - b.unit(0).unwrap().figures;
        let mut plain = basilisk();
        plain.abilities.clear();
        let mut b = duel(squad(), plain, seed);
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .unwrap();
        gazeless_lost += 10 - b.unit(0).unwrap().figures;
    }
    // Resistance 3 at -3 fails nine times in ten: about nine of the ten figures turn to stone.
    assert!(lost > 700, "{lost}");
    assert!(gazeless_lost < 100, "{gazeless_lost}");
}

#[test]
fn a_life_stealing_shot_heals_the_shooter() {
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
            .unwrap();
        healed += 8 - b.unit(0).unwrap().front_damage;
    }
    assert!(healed > 0);
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
fn a_wall_crusher_brings_a_wall_down_half_the_time_in_melee() {
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
            .unwrap();
        let BattleEvent::WallAttacked { destroyed, .. } = events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(destroyed, !b.map.has_wall((12, 11)));
        fell += i32::from(destroyed);
        // The blow takes the unit's turn.
        assert!(b.unit(0).unwrap().attacked);
        assert_eq!(b.unit(0).unwrap().moves_left, 0);
    }
    assert!((150..250).contains(&fell), "{fell}");
}

#[test]
fn shooting_a_wall_works_a_quarter_of_the_time_and_costs_a_shot() {
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
        .unwrap();
        assert_eq!(b.unit(0).unwrap().ammo, 3);
        fell += i32::from(!b.map.has_wall((12, 11)));
    }
    assert!((60..140).contains(&fell), "{fell}");
}

#[test]
fn only_attacking_wall_crushers_may_strike_a_standing_wall() {
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
    b.apply(BattleCommand::EndTurn).unwrap();
    let mut b2 = siege(
        vec![
            BattleUnit::simple(Side::Attacker, "x", 1, 1, 1, 1).at((3, 3)),
            crusher(Side::Defender, (13, 11), false),
        ],
        true,
        1,
    );
    b2.apply(BattleCommand::EndTurn).unwrap();
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
}

#[test]
fn a_wall_crusher_hitting_a_unit_on_the_wall_may_bring_the_wall_down_too() {
    let mut fell = 0;
    for seed in 0..300 {
        let mut b = siege(
            vec![crusher(Side::Attacker, (11, 11), false), guard((12, 11))],
            true,
            seed,
        );
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .unwrap();
        fell += i32::from(!b.map.has_wall((12, 11)));
    }
    assert!((110..190).contains(&fell), "{fell}");
}

#[test]
fn walls_add_three_to_the_defense_on_the_wall_and_one_behind_it_against_attackers_outside() {
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
            total += b.unit(1).unwrap().max_hit_points() - b.unit(1).unwrap().hit_points();
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
}

#[test]
fn attackers_inside_the_city_hurt_it_each_turn() {
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
    b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(b.city_damage, 1, "the defenders' turn adds nothing");
    b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(b.city_damage, 2);
    // A battle in the open has no city to hurt.
    let b = duel(
        spearman(Side::Attacker, (13, 11)),
        spearman(Side::Defender, (15, 11)),
        1,
    );
    assert_eq!(b.city_damage, 0);
}

#[test]
fn the_battle_ai_takes_a_walled_city() {
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
            b.apply(c).unwrap();
        }
        assert!(b.winner.is_some(), "seed {seed} never ended");
        match b.winner {
            Some(Side::Attacker) => results.0 += 1,
            _ => results.1 += 1,
        }
    }
    assert!(results.0 > 0, "the attackers never get in: {results:?}");
}

const GATE_SPOT: Pos = (12, 12);

// ---- spells (task 43) -----------------------------------------------------------------

mod casting {
    use super::*;
    use crate::battle::attack::{spell_attack, Combatant, Mode, Volley};
    use crate::battle::casting::{range_percent, BattleCaster};
    use crate::battle::immunity::{AttackNature, Immunities, Realm};
    use crate::rules::effects::DamageTraits;
    use crate::rules::stats::{ModOp, Modifier, Source, Stat};

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
    fn a_mana_budget_caps_what_the_wizard_can_afford_and_is_used_up_by_casting() {
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
        let c = b.caster(Side::Attacker).unwrap();
        assert_eq!((c.budget, c.mana, c.skill_left), (Some(3), 95, 15));
        assert_eq!(c.affordable(), 3);
        // A side without a wizard has nothing to limit; no budget means no limit.
        b.set_mana_budget(Side::Defender, Some(1));
        assert!(b.caster(Side::Defender).is_none());
        b.set_mana_budget(Side::Attacker, None);
        assert_eq!(b.caster(Side::Attacker).unwrap().affordable(), 15);
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
    fn a_lasting_spell_changes_the_numbers_and_a_rebuild_takes_it_off_again() {
        let mut b = duel(
            spearman(Side::Attacker, (3, 5)),
            spearman(Side::Defender, (8, 5)),
            1,
        );
        let m = |stat, op, value| Modifier::new(stat, op, value, Source::Other);
        let spell = crate::rules::SpellId::new(0);
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
        let base = u.base.as_deref().unwrap().clone();
        b.rebuild_unit(0, base, Vec::new(), &[]);
        let u = &b.units[0];
        assert_eq!(
            (u.moves, u.melee, u.first_strike),
            (before.moves, before.melee, false)
        );
        assert!(u.combat_spells.is_empty() && u.base.is_none());
        assert!(u.abilities.is_empty() && u.enchantments.is_empty());
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

// ---- battlefield enchantments (task 44) -----------------------------------------------

mod fields {
    use super::*;
    use crate::battle::field::{Field, MANA_LEAK_DRAIN};
    use crate::battle::immunity::Realm;
    use crate::rules::SpellId;

    fn spell() -> SpellId {
        SpellId::new(0)
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
            all.extend(b.apply(BattleCommand::EndTurn).unwrap());
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
    fn mana_leak_drains_the_enemy_wizard_and_its_spell_shots() {
        let mut shooter = of(Side::Defender, None, (17, 5));
        shooter.ranged_kind = RangedKind::Magic;
        shooter.ammo = 3;
        let mut b = pair(of(Side::Attacker, None, (3, 5)), shooter);
        b.set_caster(Side::Defender, crate::battle::BattleCaster::new(20, 40, 10));
        b.raise_field(Side::Attacker, Field::ManaLeak, spell());
        end_turns(&mut b, 2);
        assert_eq!(b.caster(Side::Defender).unwrap().mana, 40 - MANA_LEAK_DRAIN);
        assert_eq!(b.units[1].ammo, 2);
    }

    #[test]
    fn blur_turns_some_hits_away_and_illusion_immune_attackers_see_through_it() {
        let hurt = |blur: bool, sees: bool| -> i32 {
            let mut total = 0;
            for seed in 0..300 {
                let mut a = of(Side::Attacker, None, (5, 5));
                a.melee = 10;
                a.figures_start = 1;
                a.figures = 1;
                if sees {
                    a.abilities.push("illusion_immunity".into());
                }
                let mut d = of(Side::Defender, None, (6, 5));
                d.defense = 0;
                d.hits = 1000;
                d.melee = 0;
                let mut b = duel(a, d, seed);
                if blur {
                    b.raise_field(Side::Defender, Field::Blur, spell());
                }
                b.apply(BattleCommand::Attack { unit: 0, target: 1 })
                    .unwrap();
                total += b.units[1].max_hit_points() - b.units[1].hit_points();
            }
            total
        };
        let (plain, blurred, seen) = (hurt(false, false), hurt(true, false), hurt(true, true));
        // About a tenth of the hits miss.
        assert!(
            blurred < plain && blurred as f64 > plain as f64 * 0.8,
            "{blurred} of {plain}"
        );
        assert_eq!(seen, plain);
    }

    #[test]
    fn an_invisible_creature_is_harder_to_hit_unless_the_attacker_sees_through_illusions() {
        let mut ghost = of(Side::Defender, None, (6, 5));
        ghost.abilities.push("invisibility".into());
        let b = pair(of(Side::Attacker, None, (5, 5)), ghost);
        assert_eq!(b.invisibility_penalty(0, 1), 1);
        assert_eq!(b.invisibility_penalty(1, 0), 0);
        let mut b = b;
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
    fn figures_that_fail_their_resistance_are_destroyed() {
        let mut soft = of(Side::Defender, None, (6, 5));
        soft.resistance = 0;
        soft.figures_start = 6;
        soft.figures = 6;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), soft);
        let dmg = b.spell_slay(1, Realm::Death, -9, &[], None).unwrap();
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
            b.spell_slay(1, Realm::Death, -2, &[], None).unwrap().kills,
            0
        );
        // Disintegrate: resistance under ten and the whole unit is gone.
        let mut frail = of(Side::Defender, None, (6, 5));
        frail.resistance = 9;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), frail);
        b.spell_slay(1, Realm::Chaos, 0, &[], Some(10)).unwrap();
        assert!(!b.units[1].is_active());
        let mut hardy = of(Side::Defender, None, (6, 5));
        hardy.resistance = 10;
        let mut b = pair(of(Side::Attacker, None, (5, 5)), hardy);
        b.spell_slay(1, Realm::Chaos, 0, &[], Some(10)).unwrap();
        assert!(b.units[1].is_active());
    }
}

#[test]
fn mithril_and_adamantium_weapons_add_to_the_attack_defence_and_to_hit() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let unit = g.spawn_unit(A, at(4, 4), walker);
    let plain = g.battle_unit_from(unit, Side::Attacker, 0).unwrap();
    for (quality, extra, plus) in [(1, 0, 1), (2, 1, 1), (3, 2, 1)] {
        let _ = g
            .world
            .insert_one(unit, crate::units::WeaponQuality(quality));
        let bu = g.battle_unit_from(unit, Side::Attacker, 0).unwrap();
        assert_eq!(bu.melee, plain.melee + extra, "melee, quality {quality}");
        assert_eq!(
            bu.defense,
            plain.defense + extra,
            "defence, quality {quality}"
        );
        assert_eq!(bu.to_hit + bu.weapon_plus(), plain.to_hit + plus);
    }
}

// ---- city enchantments (task 25) ---------------------------------------------------------

fn city_map() -> BattleMap {
    BattleMap::open().with_city(false)
}

/// Ten figures of one hit point and no defence: a Wall of Fire's attack leaves a mark on them.
fn crowd(side: Side, cell: Pos) -> BattleUnit {
    BattleUnit::simple(side, "crowd", 10, 1, 1, 0).at(cell)
}

#[test]
fn a_wall_of_fire_burns_walkers_that_step_into_the_city_but_not_fliers() {
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
            .unwrap();
        let hurt = events
            .iter()
            .any(|e| matches!(e, BattleEvent::SpellDamaged { unit: 0, .. }));
        assert_eq!(hurt, burned, "flies: {flies}");
        assert_eq!(b.unit(0).unwrap().figures < 10, burned);
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
        .unwrap();
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
        .unwrap();
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::SpellDamaged { .. })));
}

#[test]
fn striking_into_a_city_behind_a_wall_of_fire_burns_the_attacker() {
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
        .unwrap();
    assert!(matches!(
        events.first(),
        Some(BattleEvent::SpellDamaged { unit: 0, .. })
    ));
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
fn the_legal_commands_are_exactly_what_apply_accepts() {
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
    b.apply(BattleCommand::Done { unit: 0 }).unwrap();
    b.apply(BattleCommand::EndTurn).unwrap();
    check_legal_list(&b);
}

#[test]
fn the_legal_commands_cover_walls_crushers_and_darkness() {
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
    b.apply(BattleCommand::Flee).unwrap();
    assert!(b.legal_commands().is_empty());
}

#[test]
fn an_undead_unit_fights_as_a_fantastic_death_creature_with_its_immunities() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let a2 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    assert!(g.make_undead(a1));
    g.apply(Command::BeginBattle {
        attackers: vec![a1, a2],
        defenders: vec![d1],
    });
    let b = g.battle().unwrap();
    let raised = b.units.iter().find(|u| u.source == Some(0)).unwrap();
    let living = b.units.iter().find(|u| u.source == Some(1)).unwrap();
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
}

/// A player of battles that always flees.
struct Quitter;

impl crate::battle::game::BattleBrain for Quitter {
    fn command(&mut self, _: &Game) -> BattleCommand {
        BattleCommand::Flee
    }
}

#[test]
fn a_battle_brain_plays_for_its_wizard_and_the_plain_ai_for_the_rest() {
    let mut g = game();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a1 = g.spawn_unit(A, at(4, 4), walker);
    let d1 = g.spawn_unit(D, at(5, 4), walker);
    g.set_battle_brain(A, Some(Box::new(Quitter)));
    let events = g.apply(Command::AutoResolve {
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
        attackers: vec![a2],
        defenders: vec![d2],
    });
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::Battle(BattleEvent::Fled { .. }))));
}

// ---- combat ability rules (task 37), spell shots (38), after the battle (42) -------------

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
fn a_spell_caster_shoots_with_its_mana_and_mana_leak_costs_a_shot() {
    let mut b = duel(
        caster(&["fireball_attack", "spellcaster"], 20, 6),
        dummy(&[]),
        1,
    );
    b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
        .unwrap();
    let u = b.unit(0).unwrap();
    assert_eq!((u.mana, u.ammo), (17, 5), "three mana a shot");
    // Mana leak takes five mana, which is another shot and a half.
    b.units[0].leak_mana(5);
    let u = b.unit(0).unwrap();
    assert_eq!((u.mana, u.ammo), (12, 4));
    // With no mana left it cannot shoot.
    b.units[0].leak_mana(100);
    assert_eq!((b.unit(0).unwrap().mana, b.unit(0).unwrap().ammo), (0, 0));
    // A unit with spell shots but no mana of its own simply loses one.
    let mut plain = caster(&["fireball_attack"], 0, 3);
    plain.leak_mana(5);
    assert_eq!((plain.mana, plain.ammo), (0, 2));
    plain.ranged_kind = RangedKind::Missile;
    plain.leak_mana(5);
    assert_eq!(plain.ammo, 2, "arrows are not spell shots");
}

#[test]
fn regenerating_units_heal_a_hit_as_their_sides_turn_begins() {
    let mut hurt = dummy(&["regeneration"]);
    hurt.front_damage = 5;
    let mut b = duel(spearman(Side::Attacker, (3, 10)), hurt, 1);
    let before = b.unit(1).unwrap().hit_points();
    b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(b.unit(1).unwrap().hit_points(), before + 1);
    // Not on the other side's turn.
    b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(b.unit(1).unwrap().hit_points(), before + 1);
    // The Regeneration spell grants the same ability.
    let mut under_spell = dummy(&[]);
    under_spell.front_damage = 5;
    under_spell.abilities.push("regeneration".into());
    let mut b = duel(spearman(Side::Attacker, (3, 10)), under_spell, 1);
    let before = b.unit(1).unwrap().hit_points();
    b.apply(BattleCommand::EndTurn).unwrap();
    assert_eq!(b.unit(1).unwrap().hit_points(), before + 1);
}

fn game_with_abilities(unit_id: &str, abilities: &[&str]) -> Game {
    let mut g = game();
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == unit_id) {
        u.kind = crate::rules::UnitKind::Normal;
        u.abilities.extend(abilities.iter().map(|a| a.to_string()));
    }
    g.content.rules.units = crate::rules::Registry::new(units);
    for (id, name) in [(A, "A"), (D, "D")] {
        g.players.push(crate::player::Player::new(id, name));
    }
    g
}

#[test]
fn luck_adds_one_to_hit_to_block_and_resistance_in_battle() {
    let mut g = game_with_abilities("walker", &["lucky"]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    let plain = {
        let mut other = game();
        let w = other.content.rules.units.id_of("walker").unwrap();
        let u = other.spawn_unit(A, at(4, 4), w);
        let d = other.spawn_unit(D, at(5, 4), w);
        other.apply(Command::BeginBattle {
            attackers: vec![u],
            defenders: vec![d],
        });
        other.battle().unwrap().unit(0).unwrap().clone()
    };
    g.apply(Command::BeginBattle {
        attackers: vec![a],
        defenders: vec![d],
    });
    let lucky = g.battle().unwrap().unit(0).unwrap();
    assert_eq!(lucky.to_hit, plain.to_hit + 1);
    assert_eq!(lucky.to_block, plain.to_block + 1);
    assert_eq!(lucky.resistance, plain.resistance + 1);
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
    let b = &mut g.battle.as_mut().unwrap().battle;
    for &i in dead {
        b.units[i].figures = 0;
        b.units[i].status = Status::Dead;
    }
    g.apply(Command::Battle(BattleCommand::EndTurn))
}

#[test]
fn a_regenerating_unit_is_healed_after_the_fight_and_comes_back_if_its_side_won() {
    let mut g = game_with_abilities("walker", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    let mut troll = units.iter().find(|u| u.id == "walker").unwrap().clone();
    troll.id = "troll".into();
    troll.abilities.push("regeneration".into());
    units.push(troll);
    g.content.rules.units = crate::rules::Registry::new(units);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let troll = g.content.rules.units.id_of("troll").unwrap();
    let alive = g.spawn_unit(A, at(4, 4), walker);
    let slain_troll = g.spawn_unit(A, at(4, 4), troll);
    let hurt_troll = g.spawn_unit(A, at(4, 4), troll);
    let slain_walker = g.spawn_unit(A, at(4, 4), walker);
    let foe = g.spawn_unit(D, at(5, 4), walker);
    g.world
        .insert_one(hurt_troll, crate::units::Hurt(4))
        .unwrap();
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
    assert_eq!(
        g.world.get::<&crate::units::Hurt>(slain_troll).unwrap().0,
        0
    );
    assert_eq!(g.world.get::<&crate::units::Hurt>(hurt_troll).unwrap().0, 0);
    assert_eq!(g.unit_position(slain_troll), Some(at(5, 4)), "and moves in");
}

#[test]
fn a_regenerating_unit_that_loses_stays_dead() {
    let mut g = game_with_abilities("walker", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    let mut troll = units.iter().find(|u| u.id == "walker").unwrap().clone();
    troll.id = "troll".into();
    troll.abilities.push("regeneration".into());
    units.push(troll);
    g.content.rules.units = crate::rules::Registry::new(units);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let troll = g.content.rules.units.id_of("troll").unwrap();
    let a = g.spawn_unit(A, at(4, 4), troll);
    let d = g.spawn_unit(D, at(5, 4), walker);
    finish_with_dead(&mut g, &[a], &[d], &[0]);
    assert!(!g.world.contains(a));
}

#[test]
fn the_winners_survivors_gain_experience_and_fame_for_four_kills() {
    let mut g = game_with_abilities("walker", &[]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let fallen = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    g.player_mut(A).unwrap().fame = 3;
    let events = finish_with_dead(&mut g, &[a, fallen], &foes, &[1, 2, 3, 4, 5]);
    assert!(g.world.contains(a));
    // Four dead enemies: 2 each. The fallen of its own side give nothing, nor does it gain.
    let xp = g.world.get::<&crate::units::Experience>(a).unwrap().0;
    assert_eq!(xp, 8);
    assert_eq!(g.player(A).unwrap().fame, 4);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::BattleSpoils { player, experience: 8, fame: 1 } if *player == A
    )));
}

#[test]
fn fewer_than_four_kills_give_experience_but_no_fame() {
    let mut g = game_with_abilities("walker", &[]);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..2).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    finish_with_dead(&mut g, &[a], &foes, &[1, 2]);
    assert_eq!(g.world.get::<&crate::units::Experience>(a).unwrap().0, 4);
    assert_eq!(g.player(A).unwrap().fame, 0);
}

#[test]
fn a_very_rare_foe_is_worth_a_point_of_fame() {
    let mut g = game_with_abilities("walker", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == "walker") {
        u.cost = 600;
    }
    g.content.rules.units = crate::rules::Registry::new(units);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let d = g.spawn_unit(D, at(5, 4), walker);
    g.player_mut(D).unwrap().fame = 5;
    finish_with_dead(&mut g, &[a], &[d], &[1]);
    assert_eq!(g.player(A).unwrap().fame, 1);
    assert_eq!(
        g.player(D).unwrap().fame,
        5,
        "no hero fell, so the loser keeps its fame"
    );
}

#[test]
fn a_fallen_hero_costs_its_owner_fame_by_level_and_a_human_loser_gives_one_back() {
    let mut g = game_with_abilities("walker", &[]);
    let mut units = g.content.rules.units.defs().to_vec();
    for u in units.iter_mut().filter(|u| u.id == "walker") {
        u.kind = crate::rules::UnitKind::Hero;
    }
    g.content.rules.units = crate::rules::Registry::new(units);
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let a = g.spawn_unit(A, at(4, 4), walker);
    let foes: Vec<_> = (0..4).map(|_| g.spawn_unit(D, at(5, 4), walker)).collect();
    // The first foe is a Veteran (level 1): half of 1 + 1 is 1.
    g.world
        .insert_one(foes[0], crate::units::Experience(20))
        .unwrap();
    g.player_mut(D).unwrap().fame = 25;
    g.human = Some(D);
    finish_with_dead(&mut g, &[a], &foes, &[1, 2, 3, 4]);
    // 25 - 1 for the hero, then one more for the four kills of a human loser above 20 fame.
    assert_eq!(g.player(D).unwrap().fame, 23);
    // A hero's level 0 costs nothing.
    let mut g2 = game_with_abilities("walker", &[]);
    let walker = g2.content.rules.units.id_of("walker").unwrap();
    let a = g2.spawn_unit(A, at(4, 4), walker);
    let d = g2.spawn_unit(D, at(5, 4), walker);
    g2.player_mut(D).unwrap().fame = 5;
    finish_with_dead(&mut g2, &[a], &[d], &[1]);
    assert_eq!(g2.player(D).unwrap().fame, 5);
}
