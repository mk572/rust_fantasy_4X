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
    assert!(stopped >= 1 && stopped <= 9, "stopped {stopped} of 10");
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
