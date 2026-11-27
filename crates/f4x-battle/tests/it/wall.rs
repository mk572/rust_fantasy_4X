//! The walls and the enchantments of a city in a battle on real units of the shipped content: wall
//! crushers, the wall of fire and of darkness, a flying fortress, and what the walls do to damage
//! and defence.
//!
//! Tests: this is the test file; the units come from `f4x_battle::testutil`.

use f4x_battle::battle::{Battle, BattleCommand, BattleError, BattleEvent};
use f4x_battle::map;
use f4x_battle::map::{BattleMap, Pos};
use f4x_battle::testutil::*;
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::TestResult;

/// Squares round one section of a city's wall, asked from the map: a wall section that is neither a
/// tower nor the gate, the square outside it, the squares behind it (inside the city area) and a
/// square far outside, in line. Nothing here knows where the city stands or how big it is.
struct WallSpot {
    wall: Pos,
    outside: Pos,
    behind: Pos,
    deeper: Pos,
    two_out: Pos,
    distant: Pos,
}

/// A battle of the real game at a city, walled or not: `ours` attack, `theirs` defend.
fn siege(ours: &[(u16, Pos)], theirs: &[(u16, Pos)], walled: bool, seed: u64) -> Battle {
    let mut b = real_battle(ours, theirs, seed);
    b.map = open_map().with_city(walled);
    b.note_invisibility();
    b
}

fn wall_spot() -> WallSpot {
    use f4x_battle::map::{in_city_area, is_wall_corner, is_wall_site};
    let m = open_map().with_city(true);
    let wall = (0..map::GRID_H)
        .flat_map(|y| (0..map::GRID_W).map(move |x| (x, y)))
        .find(|&p| is_wall_site(p) && !is_wall_corner(p) && !m.is_gate(p))
        .expect("a wall has a plain section");
    let (outside, dir) = [(-1, 0), (1, 0), (0, -1), (0, 1)]
        .into_iter()
        .map(|d| ((wall.0 + d.0, wall.1 + d.1), d))
        .find(|&(p, _)| !in_city_area(p))
        .expect("a section has an outside");
    let step = |n: i32| (wall.0 - n * dir.0, wall.1 - n * dir.1);
    let spot = WallSpot {
        wall,
        outside,
        behind: step(1),
        deeper: step(2),
        two_out: step(-2),
        distant: step(-4),
    };
    for p in [spot.behind, spot.deeper] {
        assert!(in_city_area(p) && !m.has_wall(p), "{p:?} is inside");
    }
    assert!(BattleMap::in_bounds(spot.distant) && !in_city_area(spot.distant));
    spot
}

/// Whether `count` successes in `tries` fit a chance of `percent`, give or take five standard deviations.
fn fits_percent(count: i32, tries: i32, percent: u32) -> bool {
    let p = f64::from(percent) / 100.0;
    let n = f64::from(tries);
    (f64::from(count) - n * p).abs() <= 5.0 * (n * p * (1.0 - p)).sqrt()
}

fn city_map() -> BattleMap {
    open_map().with_city(false)
}

/// A battle of the real game at an open city, `ours` against `theirs`.
fn city_battle(ours: &[(u16, Pos)], theirs: &[(u16, Pos)]) -> Battle {
    let mut b = real_battle(ours, theirs, 5);
    b.map = city_map();
    b
}

#[test]
fn a_wall_crusher_brings_a_wall_down_as_often_as_the_rules_say_in_melee() -> TestResult {
    let (spot, chance) = (wall_spot(), rules().city_wall.crush_melee_percent);
    let mut fell = 0;
    for seed in 0..400 {
        let mut b = siege(
            &[(153 /* troll_war_mammoths */, spot.outside)],
            &[(104 /* high_men_spearmen */, spot.deeper)],
            true,
            seed,
        );
        let events = b
            .apply(BattleCommand::AttackWall {
                unit: 0,
                cell: spot.wall,
            })
            .map_err(|e| format!("{e:?}"))?;
        let BattleEvent::WallAttacked { destroyed, .. } = events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(destroyed, !b.map.has_wall(spot.wall));
        fell += i32::from(destroyed);
        // The blow costs half the allowance, as a blow on a unit does.
        let me = b.unit(0).ok_or("unit is missing")?;
        assert!(!me.attacked);
        assert_eq!(me.moves_left, me.moves - me.moves.half());
    }
    assert!(
        fits_percent(fell, 400, chance),
        "{fell} of 400 at {chance}%"
    );
    Ok(())
}

#[test]
fn shooting_a_wall_brings_it_down_as_often_as_the_rules_say_and_costs_a_shot() -> TestResult {
    let (spot, chance) = (wall_spot(), rules().city_wall.crush_ranged_percent);
    let mut fell = 0;
    for seed in 0..400 {
        let mut b = siege(
            &[(37 /* catapult */, spot.distant)],
            &[(104 /* high_men_spearmen */, spot.deeper)],
            true,
            seed,
        );
        let shots = b.units[0].ammo;
        b.apply(BattleCommand::ShootWall {
            unit: 0,
            cell: spot.wall,
        })
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(b.unit(0).ok_or("unit is missing")?.ammo, shots - 1);
        fell += i32::from(!b.map.has_wall(spot.wall));
    }
    assert!(
        fits_percent(fell, 400, chance),
        "{fell} of 400 at {chance}%"
    );
    Ok(())
}

#[test]
fn only_attacking_wall_crushers_may_strike_a_standing_wall() -> TestResult {
    let spot = wall_spot();
    let mut b = siege(
        &[
            (105 /* high_men_swordsmen */, spot.outside),
            (110 /* high_men_engineers */, spot.distant),
        ],
        &[(104 /* high_men_spearmen */, spot.deeper)],
        true,
        1,
    );
    assert_eq!(
        b.apply(BattleCommand::AttackWall {
            unit: 0,
            cell: spot.wall
        }),
        Err(BattleError::NotWallCrusher)
    );
    // Not next to the wall.
    assert_eq!(
        b.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: spot.wall
        }),
        Err(BattleError::NotAdjacent)
    );
    // No wall on the square.
    assert_eq!(
        b.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: spot.distant
        }),
        Err(BattleError::NoWall)
    );
    assert_eq!(
        b.apply(BattleCommand::ShootWall {
            unit: 1,
            cell: spot.wall
        }),
        Err(BattleError::NoRangedAttack)
    );
    // Once the defenders' turn comes, a defending crusher is no wall breaker.
    let mut b2 = siege(
        &[(104 /* high_men_spearmen */, spot.distant)],
        &[(110 /* high_men_engineers */, spot.behind)],
        true,
        1,
    );
    b2.apply(BattleCommand::EndTurn)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        b2.apply(BattleCommand::AttackWall {
            unit: 1,
            cell: spot.wall
        }),
        Err(BattleError::NotWallCrusher)
    );
    // And a city without walls has none to strike.
    let mut open = siege(
        &[(110 /* high_men_engineers */, spot.outside)],
        &[(104 /* high_men_spearmen */, spot.deeper)],
        false,
        1,
    );
    assert_eq!(
        open.apply(BattleCommand::AttackWall {
            unit: 0,
            cell: spot.wall
        }),
        Err(BattleError::NoWall)
    );
    Ok(())
}

/// 1.50 note 476: a wall crusher can strike a wall section with an enemy standing right behind it:
/// the wall has its own target path, not the one of a unit hit.
#[test]
fn a_wall_crusher_strikes_a_wall_with_a_defender_standing_behind_it() -> TestResult {
    let spot = wall_spot();
    let mut b = siege(
        &[(110 /* high_men_engineers */, spot.outside)],
        &[(104 /* high_men_spearmen */, spot.behind)],
        true,
        1,
    );
    let events = b
        .apply(BattleCommand::AttackWall {
            unit: 0,
            cell: spot.wall,
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
    let (spot, chance) = (wall_spot(), rules().city_wall.crush_melee_percent);
    let mut fell = 0;
    for seed in 0..300 {
        let mut b = siege(
            &[(153 /* troll_war_mammoths */, spot.outside)],
            &[(104 /* high_men_spearmen */, spot.wall)],
            true,
            seed,
        );
        b.apply(BattleCommand::Attack { unit: 0, target: 1 })
            .map_err(|e| format!("{e:?}"))?;
        fell += i32::from(!b.map.has_wall(spot.wall));
    }
    assert!(
        fits_percent(fell, 300, chance),
        "{fell} of 300 at {chance}%"
    );
    Ok(())
}

#[test]
fn walls_lower_the_damage_a_unit_on_them_takes_from_attackers_outside_and_nothing_else(
) -> TestResult {
    let spot = wall_spot();
    // Hit points a defender loses over many fights against swordsmen that stand at `from`.
    let lost = |target: Pos, from: Pos, walled: bool| {
        let mut total = 0;
        for seed in 0..300 {
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
    // A defender on the wall, struck from outside: the walls help it by what the rules give.
    let on_wall = lost(spot.wall, spot.outside, true);
    let on_open_ground = lost(spot.wall, spot.outside, false);
    assert_eq!(
        on_wall < on_open_ground,
        rules().wall_defence > 0,
        "{on_wall} vs {on_open_ground}"
    );
    // Struck by an attacker that is inside the city area too, the walls give nothing.
    assert_eq!(
        lost(spot.behind, spot.deeper, true),
        lost(spot.behind, spot.deeper, false)
    );
    Ok(())
}

#[test]
fn the_defence_the_walls_give_follows_the_rules_for_the_wall_and_behind_it() {
    let (spot, rules) = (wall_spot(), rules());
    let (wall, city) = (rules.wall_defence, rules.city_defence);
    // An attacker outside the city area, a target on the wall and one behind it.
    let mut b = siege(
        &[(105 /* high_men_swordsmen */, spot.outside)],
        &[
            (104 /* high_men_spearmen */, spot.wall),
            (104 /* high_men_spearmen */, spot.behind),
        ],
        true,
        1,
    );
    assert_eq!(
        (b.city_defense_bonus(1, 0), b.city_defense_bonus(2, 0)),
        (wall, city)
    );
    b.walls_cover_city = true;
    assert_eq!(
        (b.city_defense_bonus(1, 0), b.city_defense_bonus(2, 0)),
        (wall, wall)
    );
    // Still nothing against an attacker inside, or without walls.
    b.units[0].cell = spot.deeper;
    assert_eq!(b.city_defense_bonus(2, 0), 0);
    let mut open = siege(
        &[(105 /* high_men_swordsmen */, spot.outside)],
        &[(104 /* high_men_spearmen */, spot.behind)],
        false,
        1,
    );
    open.walls_cover_city = true;
    assert_eq!(open.city_defense_bonus(1, 0), 0);
}

#[test]
fn attackers_inside_the_city_hurt_it_each_turn() -> TestResult {
    let spot = wall_spot();
    let mut b = siege(
        &[
            (105 /* high_men_swordsmen */, spot.behind),
            (105, spot.outside),
        ],
        &[(104 /* high_men_spearmen */, spot.deeper)],
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
    let b = real_battle(
        &[(104 /* high_men_spearmen */, (13, 11))],
        &[(104 /* high_men_spearmen */, (15, 11))],
        1,
    );
    assert_eq!(b.city_damage, 0);
    Ok(())
}

#[test]
fn a_wall_of_fire_burns_walkers_that_step_into_the_city_but_not_fliers() -> TestResult {
    let spot = wall_spot();
    let far = (104 /* high_men_spearmen */, spot.distant);
    for (number, burned) in [
        (107 /* high_men_cavalry */, true),
        (103 /* high_elf_pegasai */, false),
    ] {
        let mut b = city_battle(&[(number, spot.two_out)], &[far]);
        let figures = b.units[0].figures;
        b.wall_of_fire = true;
        let events = b
            .apply(BattleCommand::Move {
                unit: 0,
                to: spot.wall,
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
    let mut b = city_battle(&[(107 /* high_men_cavalry */, spot.behind)], &[far]);
    b.wall_of_fire = true;
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: spot.deeper,
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::SpellDamaged { .. })));
    let mut b = city_battle(&[(107 /* high_men_cavalry */, spot.two_out)], &[far]);
    let events = b
        .apply(BattleCommand::Move {
            unit: 0,
            to: spot.wall,
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(!events
        .iter()
        .any(|e| matches!(e, BattleEvent::SpellDamaged { .. })));
    Ok(())
}

#[test]
fn striking_into_a_city_behind_a_wall_of_fire_burns_the_attacker() -> TestResult {
    let spot = wall_spot();
    let mut b = city_battle(
        &[(104 /* high_men_spearmen */, spot.outside)],
        &[(104 /* high_men_spearmen */, spot.wall)],
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
    let spot = wall_spot();
    // Longbowmen shoot into the city; shadow demons (illusion immune) see through the darkness.
    let shoot = |shooter: u16, from: Pos, dark: bool| {
        let mut b = city_battle(
            &[(shooter, from)],
            &[(104 /* high_men_spearmen */, spot.behind)],
        );
        b.wall_of_darkness = dark;
        b.apply(BattleCommand::Shoot { unit: 0, target: 1 })
    };
    let longbowmen = 101;
    assert_eq!(
        shoot(longbowmen, spot.distant, true),
        Err(BattleError::WallOfDarkness)
    );
    assert!(
        shoot(171 /* shadow_demons */, spot.distant, true).is_ok(),
        "illusion immunity sees through"
    );
    assert!(
        shoot(longbowmen, spot.distant, false).is_ok(),
        "no wall, no darkness"
    );
    assert!(
        shoot(longbowmen, spot.deeper, true).is_ok(),
        "shooters inside the city are not stopped"
    );
}

#[test]
fn a_flying_fortress_shuts_the_city_to_walkers_and_its_walkers_in() {
    let map = city_map().with_flying_fortress();
    let mut b = real_battle(
        &[
            (107 /* high_men_cavalry */, (10, 11)),
            (103 /* high_elf_pegasai */, (10, 12)),
        ],
        &[(107 /* high_men_cavalry */, (13, 11)), (103, (13, 12))],
        5,
    );
    b.map = map.clone();
    for u in &mut b.units {
        u.force_moves(HalfInt::from_int(10));
        u.refill_moves();
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

#[test]
fn the_legal_commands_cover_walls_crushers_and_darkness() -> TestResult {
    let spot = wall_spot();
    let mut b = siege(
        &[
            (37 /* catapult */, spot.outside),
            (153 /* troll_war_mammoths */, spot.distant),
        ],
        &[
            (104 /* high_men_spearmen */, spot.deeper),
            (104 /* high_men_spearmen */, spot.behind),
        ],
        true,
        3,
    );
    check_legal_list(&b);
    assert!(b.legal_commands().contains(&BattleCommand::AttackWall {
        unit: 0,
        cell: spot.wall
    }));
    // A Wall of Darkness stops the shots from outside the city at a unit inside it.
    b.wall_of_darkness = true;
    check_legal_list(&b);
    // A won battle allows nothing.
    b.apply(BattleCommand::Flee).map_err(|e| format!("{e:?}"))?;
    assert!(b.legal_commands().is_empty());
    Ok(())
}
