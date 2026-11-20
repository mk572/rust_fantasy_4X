use super::*;
use f4x_def::half_int::HalfInt;
use f4x_def::testdir::TestResult;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn none(_: Pos) -> bool {
    false
}

#[test]
fn the_field_holds_the_originals_squares_where_they_were_and_squares_beyond_them() {
    let m = BattleMap::open();
    const { assert!(GRID_W >= layout::W && GRID_H >= layout::H) };
    assert_eq!(m.get((0, 0)), Some(Cell::Open));
    assert_eq!(m.get((GRID_W - 1, GRID_H - 1)), Some(Cell::Open));
    assert_eq!(m.get((GRID_W, 0)), None);
    assert_eq!(m.get((0, -1)), None);
    // The original's squares keep their places in the core's frame (`x = 20 - cgx`), so every
    // position the rules name stays where it was; the new ones lie beyond its last column and row.
    for cg in [(0, 0), (layout::W - 1, layout::H - 1), layout::STRUCTURE] {
        assert_eq!(pos_of_cg(cg), (layout::W - 1 - cg.0, cg.1));
        assert_eq!(cg_of(pos_of_cg(cg)), cg);
        assert!(BattleMap::in_bounds(pos_of_cg(cg)));
    }
    assert!(BattleMap::in_bounds(pos_of_cg((-1, layout::H))));
}

#[test]
fn walkers_pay_by_ground_and_flyers_pay_one_move_everywhere() {
    let mut m = BattleMap::open();
    m.set((5, 5), Cell::Rough);
    m.set((6, 5), Cell::Blocked);
    assert_eq!(m.enter_cost((4, 5), false), Some(HalfInt::from_int(1)));
    assert_eq!(m.enter_cost((5, 5), false), Some(HalfInt::from_int(2)));
    assert_eq!(m.enter_cost((6, 5), false), None);
    assert_eq!(m.enter_cost((5, 5), true), Some(HalfInt::from_int(1)));
    assert_eq!(m.enter_cost((6, 5), true), Some(HalfInt::from_int(1)));
    assert_eq!(m.enter_cost((-1, 5), true), None, "nobody leaves the map");
}

#[test]
fn reach_follows_move_points_diagonals_and_obstacles() {
    let mut m = BattleMap::open();
    // Two moves: two squares straight on. A diagonal step costs half a move more (wiki "Movement
    // Type"), so two diagonal steps cost three, and a unit with any move left may take the second.
    let r = m.reachable((10, 10), HalfInt::from_int(2), false, &none);
    assert_eq!(r.get(&(12, 10)).map(|c| c.0), Some(HalfInt::from_int(2)));
    assert_eq!(r.get(&(11, 11)).map(|c| c.0), Some(HalfInt::from_raw(3)));
    assert_eq!(r.get(&(12, 12)).map(|c| c.0), Some(HalfInt::from_int(3)));
    assert!(!r.contains_key(&(13, 10)));
    // Fliers pay the same extra half move for a diagonal.
    let r = m.reachable((10, 10), HalfInt::from_int(2), true, &none);
    assert_eq!(r.get(&(11, 11)).map(|c| c.0), Some(HalfInt::from_raw(3)));
    assert_eq!(r.get(&(12, 10)).map(|c| c.0), Some(HalfInt::from_int(2)));
    // A wall of rocks forces a detour (or stops the unit).
    for y in 8..=12 {
        m.set((11, y), Cell::Blocked);
    }
    let r = m.reachable((10, 10), HalfInt::from_int(2), false, &none);
    assert!(
        !r.contains_key(&(12, 10)),
        "cannot cross the rocks in two moves"
    );
    // Flyers pass over them.
    let r = m.reachable((10, 10), HalfInt::from_int(2), true, &none);
    assert!(r.contains_key(&(12, 10)));
    // Occupied squares block the way.
    let m = BattleMap::open();
    let blocked = |p: Pos| p == (11, 10);
    let r = m.reachable((10, 10), HalfInt::from_int(1), false, &blocked);
    assert!(!r.contains_key(&(11, 10)));
    assert!(r.contains_key(&(11, 11)));
    assert!(r.contains_key(&(9, 10)));
}

#[test]
fn rough_ground_costs_double_and_a_last_move_may_overrun() {
    let mut m = BattleMap::open();
    m.set((11, 10), Cell::Rough);
    // One move (two half-moves) is enough to step into rough ground, ending at zero.
    let r = m.reachable((10, 10), HalfInt::from_int(1), false, &none);
    assert_eq!(r.get(&(11, 10)).map(|c| c.0), Some(HalfInt::from_int(2)));
    // But nothing lies beyond it.
    assert!(!r.contains_key(&(12, 10)));
}

#[test]
fn the_generated_map_is_playable_everywhere_and_follows_the_terrain() {
    let counts = |t, seed| {
        let mut r = Pcg64Mcg::seed_from_u64(seed);
        let m = BattleMap::generate(t, &mut r);
        let mut playable = 0;
        let mut dear = 0;
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                assert!(m.is_playable((x, y)), "({x},{y})");
                if m.is_playable((x, y)) {
                    playable += 1;
                    if m.enter_cost((x, y), false).is_some_and(|c| c > OPEN_COST) {
                        dear += 1;
                    }
                }
            }
        }
        (playable, dear)
    };
    assert_eq!(counts(None, 1).0, (GRID_W * GRID_H) as u32);
    let mountains: u32 = (0..20)
        .map(|s| counts(Some(TerrainClass::Mountains), s).1)
        .sum();
    let grass: u32 = (0..20)
        .map(|s| counts(Some(TerrainClass::Grassland), s).1)
        .sum();
    assert!(mountains > grass * 2, "{mountains} vs {grass}");
    let a = BattleMap::generate(Some(TerrainClass::Hills), &mut Pcg64Mcg::seed_from_u64(5));
    let b = BattleMap::generate(Some(TerrainClass::Hills), &mut Pcg64Mcg::seed_from_u64(5));
    assert_eq!(a, b);
}

#[test]
fn the_corners_the_original_shuts_can_be_entered_by_walkers_and_fliers() {
    let m = BattleMap::generate(
        Some(TerrainClass::Grassland),
        &mut Pcg64Mcg::seed_from_u64(1),
    );
    assert!(m.enter_cost((20, 0), true).is_some());
    assert!(m.enter_cost((20, 0), false).is_some());
    assert!(m.enter_cost((10, 10), true).is_some());
}

#[test]
fn the_distance_counts_diagonals_as_one() {
    assert_eq!(distance((0, 0), (3, 2)), 3);
}

fn walled() -> BattleMap {
    BattleMap::open().with_city(true)
}

fn walker() -> Mover {
    Mover::walker()
}

fn flier() -> Mover {
    Mover {
        flying: true,
        through_walls: true,
        ..Mover::default()
    }
}

/// The squares of the city area's edge and the far corner, read from the constants.
const X0: i32 = CITY_X0;
const Y0: i32 = CITY_Y0;
const X1: i32 = CITY_X0 + CITY_SIZE - 1;
const Y1: i32 = CITY_Y0 + CITY_SIZE - 1;

#[test]
fn a_city_has_wall_sites_round_its_inner_squares() {
    let m = walled();
    assert!(m.is_city() && m.is_walled());
    assert_eq!(m.standing_walls().len() as i32, 4 * (CITY_SIZE - 1));
    assert!(
        m.has_wall(m.gate()),
        "the gate square has a wall section too"
    );
    assert!(!m.has_wall((X0 + 2, Y0 + 2)), "the inside has none");
    assert!(is_wall_corner((X1, Y1)) && !is_wall_corner((X1, m.gate().1)));
    // The area with its walls is the ring and what it holds.
    assert!(in_city_area((X0, Y0)) && !in_city_area((X0 - 1, Y0)));
    // No walls: a city, but nothing to break or stand behind.
    let open = BattleMap::open().with_city(false);
    assert!(open.is_city() && !open.is_walled());
    assert!(open.standing_walls().is_empty());
    assert!(!BattleMap::open().is_city());
}

/// Setup: the city map the game builds, with the wall rules of the shipped `battle.ron`. Action: from a
/// square outside the city, step a walker, a flier and a teleporter onto each standing wall section
/// (the gate's, and the others), then break the section and step again. Outcome: each step is allowed
/// exactly when the rules say (`gate_enterable`, `section_enterable`, `passers_cross`); a broken
/// section is open to everyone.
#[test]
fn a_standing_wall_section_is_entered_as_the_battle_rules_say() {
    let rule = f4x_def::testdir::shipped().rules.battle.city_wall;
    let teleporter = Mover {
        through_walls: true,
        ..Mover::default()
    };
    let mut m = BattleMap::open().with_city(true).with_wall_rules(rule);
    let sections: Vec<Pos> = m
        .standing_walls()
        .into_iter()
        .filter(|&p| !is_wall_corner(p))
        .collect();
    assert!(sections.contains(&m.gate()) && sections.len() > 1);
    let enters = |m: &BattleMap, section: Pos, mover: Mover| {
        let from = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (section.0 + dx, section.1 + dy)))
            .find(|&p| BattleMap::in_bounds(p) && !in_city_area(p))
            .expect("a wall section has a square outside it");
        m.reachable_as(from, HalfInt::from_int(3), mover, &none)
            .contains_key(&section)
    };
    for &section in &sections {
        let walkers_may = if section == m.gate() {
            rule.gate_enterable
        } else {
            rule.section_enterable
        };
        let passers_may = rule.passers_cross || walkers_may;
        assert_eq!(
            enters(&m, section, walker()),
            walkers_may,
            "{section:?} on foot"
        );
        assert_eq!(
            enters(&m, section, flier()),
            passers_may,
            "{section:?} flying"
        );
        assert_eq!(
            enters(&m, section, teleporter),
            passers_may,
            "{section:?} teleporting"
        );
    }
    for &section in &sections {
        m.break_wall(section);
        assert!(
            enters(&m, section, walker()),
            "a broken section {section:?}"
        );
    }
}

#[test]
fn a_broken_wall_can_be_walked_over() {
    let mut m = walled();
    assert!(!m.is_breached());
    let (to, from) = ((X0, m.gate().1 - 1), (X0 - 1, m.gate().1 - 1));
    assert!(!m
        .reachable_as(from, HalfInt::from_int(2), walker(), &none)
        .contains_key(&to));
    m.break_wall(to);
    assert!(m.is_breached());
    assert!(m
        .reachable_as(from, HalfInt::from_int(2), walker(), &none)
        .contains_key(&to));
    // Breaking a wall that is not there does nothing.
    m.break_wall((X0 + 2, Y0 + 2));
    assert_eq!(m.wall((X0 + 2, Y0 + 2)), Wall::None);
    assert_eq!(m.standing_walls().len() as i32, 4 * (CITY_SIZE - 1) - 1);
}

/// Setup: the city map the game builds, with the tower rule of the shipped `battle.ron`. Action: move
/// each kind of unit (a walker, a flier, a teleporter) next to each standing tower and try to step onto
/// it; then break the tower and try again. Outcome: a standing tower is entered exactly when the rule
/// `city_wall.tower_enterable` says so; a broken one is always open.
#[test]
fn a_standing_tower_is_entered_as_the_battle_rules_say() {
    let rule = f4x_def::testdir::shipped().rules.battle.city_wall;
    let teleporter = Mover {
        through_walls: true,
        ..Mover::default()
    };
    let enters = |m: &BattleMap, tower: Pos, mover: Mover| {
        // Start on any square beside the tower that is not a wall or a tower itself.
        let from = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (tower.0 + dx, tower.1 + dy)))
            .find(|&p| p != tower && BattleMap::in_bounds(p) && !m.has_wall(p))
            .expect("a tower has a free square beside it");
        m.reachable_as(from, HalfInt::from_int(3), mover, &none)
            .contains_key(&tower)
    };
    let mut m = BattleMap::open().with_city(true).with_wall_rules(rule);
    let towers = m.towers();
    assert!(!towers.is_empty(), "a walled city has towers");
    for &tower in &towers {
        for mover in [walker(), flier(), teleporter] {
            assert_eq!(
                enters(&m, tower, mover),
                rule.tower_enterable,
                "{tower:?} {mover:?}"
            );
        }
    }
    for &tower in &towers {
        m.break_wall(tower);
        assert!(enters(&m, tower, walker()), "a broken tower {tower:?}");
    }
}

/// The squares a unit passes from its start to `to`, read back from the reach map.
fn path_to(reach: &HashMap<Pos, (HalfInt, Pos)>, from: Pos, to: Pos) -> Vec<Pos> {
    let mut path = vec![to];
    while *path.last().expect("not empty") != from {
        path.push(reach[path.last().expect("not empty")].1);
    }
    path
}

/// Setup: a city with all its walls standing, and a walker on a square inside it with plenty of moves.
/// Action: find everywhere it can go. Outcome: it can get out, and every way out leads through a gate;
/// the same from a unit standing on a wall section. With the gates blocked and the walls unbroken it
/// cannot get out at all; once a section is broken it can, through the breach.
#[test]
fn a_unit_inside_the_unbroken_walls_leaves_only_through_a_gate() {
    let mut m = walled();
    let squares = || (0..GRID_H).flat_map(|y| (0..GRID_W).map(move |x| (x, y)));
    let inside = squares()
        .find(|&p| in_city_area(p) && !is_wall_site(p))
        .expect("a city has an inside");
    let on_wall = squares()
        .find(|&p| m.has_wall(p) && !m.is_gate(p) && !is_wall_corner(p))
        .expect("a wall has sections");
    let far = HalfInt::from_int(40);
    for from in [inside, on_wall] {
        let reach = m.reachable_as(from, far, walker(), &none);
        let outside: Vec<Pos> = reach
            .keys()
            .copied()
            .filter(|&p| !in_city_area(p))
            .collect();
        assert!(!outside.is_empty(), "from {from:?} the way out is found");
        for out in outside {
            assert!(
                path_to(&reach, from, out).iter().any(|&p| m.is_gate(p)),
                "from {from:?} to {out:?} not through a gate: {:?}",
                path_to(&reach, from, out)
            );
        }
    }
    // With every gate blocked the unbroken walls keep the unit in.
    let gates_blocked = |p: Pos| m.is_gate(p);
    let reach = m.reachable_as(inside, far, walker(), &gates_blocked);
    assert!(reach.keys().all(|&p| in_city_area(p)), "walled in");
    // A breach is a way out.
    m.break_wall(on_wall);
    let gates_blocked = |p: Pos| m.is_gate(p);
    let reach = m.reachable_as(inside, far, walker(), &gates_blocked);
    assert!(
        reach.keys().any(|&p| !in_city_area(p)),
        "out through the breach"
    );
}

#[test]
fn ships_sail_the_ocean_swimmers_take_either_way_and_walkers_stay_ashore() {
    let ocean = BattleMap::generate(None, &mut Pcg64Mcg::seed_from_u64(1));
    let ship = Mover {
        sails: true,
        ..Mover::default()
    };
    let swimmer = Mover {
        swims: true,
        ..Mover::default()
    };
    let cell = (10, 10);
    assert_eq!(ocean.enter_cost_as(cell, ship), Some(HalfInt::from_int(1)));
    assert_eq!(
        ocean.enter_cost_as(cell, swimmer),
        Some(HalfInt::from_int(1))
    );
    assert_eq!(ocean.enter_cost_as(cell, walker()), None);
    assert_eq!(ocean.enter_cost_as(cell, flier()), Some(OPEN_COST));
    assert_eq!(ocean.enter_cost_as((20, 0), flier()), Some(OPEN_COST));
    // On land it is the other way about.
    let land = BattleMap::generate(
        Some(TerrainClass::Grassland),
        &mut Pcg64Mcg::seed_from_u64(1),
    );
    assert_eq!(land.enter_cost_as(cell, ship), None);
    assert!(land.enter_cost_as(cell, swimmer).is_some());
    // A ship can cross the ocean from edge to edge.
    let reach = ocean.reachable_as((14, 10), HalfInt::from_int(10), ship, &|_| false);
    assert!(reach.len() > 30);
    assert!(
        ocean
            .reachable_as((14, 10), HalfInt::from_int(10), walker(), &|_| false)
            .len()
            == 1
    );
}

#[test]
fn a_node_square_is_fought_on_the_ground_of_its_kind() {
    assert_eq!(set_of(Some(TerrainClass::NatureNode)), Set::Forest);
    assert_eq!(set_of(Some(TerrainClass::ChaosNode)), Set::Mountains);
    assert_eq!(set_of(Some(TerrainClass::SorceryNode)), Set::Plains);
}

fn sea_with_two_decks() -> (BattleMap, Vec<Pos>, Vec<Pos>) {
    let decks = layout::ship_decks(&[(3, 7)], &[(3, 7)]);
    let (a, d) = (
        decks.attackers[0].clone().expect("clone succeeds"),
        decks.defenders[0].clone().expect("clone succeeds"),
    );
    let mut spec = Spec::open(Set::Water);
    spec.ship_decks = a.iter().chain(&d).map(|&(x, y)| (20 - x, y)).collect();
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    (
        BattleMap::from_layout(layout::generate(&spec, &mut rng)),
        a,
        d,
    )
}

#[test]
fn a_walker_jumps_the_water_between_two_decks_for_two_moves() -> TestResult {
    let (m, a, d) = sea_with_two_decks();
    // The attackers' deck square nearest the other one, and the square across the gap.
    let (from, to) = a
        .iter()
        .flat_map(|&p| d.iter().map(move |&q| (p, q)))
        .find(|&(p, q)| {
            distance(p, q) == 2 && (p.0 - q.0).abs() != 1 && (p.1 - q.1).abs() != 1
                || (distance(p, q) == 2 && (p.0 - q.0).abs() == 2 && p.1 == q.1)
        })
        .ok_or("find is missing")?;
    assert!(m.is_ship_deck(from) && m.is_ship_deck(to));
    // The water in between is closed to walking.
    let gap = ((from.0 + to.0) / 2, (from.1 + to.1) / 2);
    assert_eq!(m.enter_cost(gap, false), None);
    // With the jump's two moves, it is reachable; with one move left it still is (any move
    // left lets a unit take a step); with none it is not.
    let reach = m.reachable(from, HalfInt::from_int(2), false, &none);
    assert_eq!(reach.get(&to).map(|c| c.0), Some(JUMP_COST));
    assert!(m
        .reachable(from, HalfInt::from_raw(1), false, &none)
        .contains_key(&to));
    assert!(!m
        .reachable(from, HalfInt::from_int(0), false, &none)
        .contains_key(&to));
    // The gap itself cannot be stood in, and a ship cannot jump.
    assert!(!reach.contains_key(&gap));
    let ship = Mover {
        sails: true,
        ..Mover::default()
    };
    assert!(!m
        .reachable_as(from, HalfInt::from_int(2), ship, &none)
        .contains_key(&to));
    // A unit that flies crosses the water, as it always does.
    assert!(m
        .reachable(from, HalfInt::from_int(2), true, &none)
        .contains_key(&gap));
    Ok(())
}

/// 1.50 note 456: on an ocean map no square is open to walkers only. Every square of the field is
/// sailable and swimmable, and none can be walked.
#[test]
fn every_square_of_an_ocean_map_is_open_to_ships_and_to_nobody_on_foot() {
    for seed in 0..5 {
        let m = BattleMap::generate(None, &mut Pcg64Mcg::seed_from_u64(seed));
        let ship = Mover {
            sails: true,
            ..Mover::default()
        };
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let sails = m.enter_cost_as((x, y), ship);
                let walks = m.enter_cost_as((x, y), walker());
                if m.get((x, y)) == Some(Cell::Void) {
                    continue;
                }
                assert!(sails.is_some(), "({x},{y}) seed {seed} is shut to ships");
                assert!(walks.is_none(), "({x},{y}) seed {seed} can be walked");
            }
        }
    }
}

/// 1.50 notes 685, 686: a field gets about the trees and rocks its terrain asks for (the draw of
/// the count, less what the patches' rounding leaves out), not a fraction of them.
#[test]
fn a_field_gets_the_trees_and_rocks_its_terrain_asks_for() {
    // (terrain, mean of the draw for the trees, for the rocks)
    for (class, trees, rocks) in [
        (TerrainClass::Grassland, 10.5, 2.0),
        (TerrainClass::Forest, 45.5, 4.5),
        (TerrainClass::Mountains, 20.5, 6.5),
    ] {
        let (mut t, mut r) = (0.0, 0.0);
        for seed in 0..300 {
            let m = BattleMap::generate(Some(class), &mut Pcg64Mcg::seed_from_u64(seed));
            let look = m.look().expect("a generated map has a look");
            // The original's count is of the cells of its band; the cells beyond get more.
            let in_band = |p: &&layout::Prop| layout::in_band(p.cell.0, p.cell.1);
            t += look.trees.iter().filter(in_band).count() as f32 / 300.0;
            r += look.rocks.iter().filter(in_band).count() as f32 / 300.0;
        }
        assert!(
            t > trees * 0.8 && t <= trees + 0.5,
            "{class:?}: {t} trees of {trees}"
        );
        assert!((r - rocks).abs() < 0.8, "{class:?}: {r} rocks of {rocks}");
    }
}

#[test]
fn the_original_battlefield_shuts_every_square_outside_the_original_band_and_nothing_else() {
    let mut r = Pcg64Mcg::seed_from_u64(1);
    let mut map = BattleMap::generate(Some(TerrainClass::Grassland), &mut r);
    let before = map.clone();
    map.shut_beyond_original();
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            let (cgx, cgy) = cg_of((x, y));
            if layout::in_band(cgx, cgy) {
                assert_eq!(map.get((x, y)), before.get((x, y)), "({x},{y})");
                assert_eq!(
                    map.enter_cost((x, y), false),
                    before.enter_cost((x, y), false)
                );
            } else {
                assert!(!map.is_playable((x, y)), "({x},{y})");
                assert_eq!(map.enter_cost((x, y), true), None, "fliers too");
            }
        }
    }
    // The picture of the field is the same.
    assert_eq!(map.look(), before.look());
}
