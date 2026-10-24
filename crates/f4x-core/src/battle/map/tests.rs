use super::*;
use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

fn none(_: Pos) -> bool {
    false
}

#[test]
fn the_map_has_the_original_size() {
    let m = BattleMap::open();
    assert_eq!((GRID_W, GRID_H), (21, 22));
    assert_eq!(m.get((0, 0)), Some(Cell::Open));
    assert_eq!(m.get((20, 21)), Some(Cell::Open));
    assert_eq!(m.get((21, 0)), None);
    assert_eq!(m.get((0, -1)), None);
}

#[test]
fn walkers_pay_by_ground_and_flyers_pay_one_move_everywhere() {
    let mut m = BattleMap::open();
    m.set((5, 5), Cell::Rough);
    m.set((6, 5), Cell::Blocked);
    assert_eq!(m.enter_cost((4, 5), false), Some(2));
    assert_eq!(m.enter_cost((5, 5), false), Some(4));
    assert_eq!(m.enter_cost((6, 5), false), None);
    assert_eq!(m.enter_cost((5, 5), true), Some(2));
    assert_eq!(m.enter_cost((6, 5), true), Some(2));
    assert_eq!(m.enter_cost((-1, 5), true), None, "nobody leaves the map");
}

#[test]
fn reach_follows_move_points_diagonals_and_obstacles() {
    let mut m = BattleMap::open();
    // Two moves = four half-moves: two squares in any direction, diagonals included.
    let r = m.reachable((10, 10), 4, false, &none);
    assert_eq!(r.get(&(12, 12)).map(|c| c.0), Some(4));
    assert!(!r.contains_key(&(13, 10)));
    // A wall of rocks forces a detour (or stops the unit).
    for y in 8..=12 {
        m.set((11, y), Cell::Blocked);
    }
    let r = m.reachable((10, 10), 4, false, &none);
    assert!(
        !r.contains_key(&(12, 10)),
        "cannot cross the rocks in two moves"
    );
    // Flyers pass over them.
    let r = m.reachable((10, 10), 4, true, &none);
    assert!(r.contains_key(&(12, 10)));
    // Occupied squares block the way.
    let m = BattleMap::open();
    let blocked = |p: Pos| p == (11, 10);
    let r = m.reachable((10, 10), 2, false, &blocked);
    assert!(!r.contains_key(&(11, 10)));
    assert!(r.contains_key(&(11, 11)));
    assert!(r.contains_key(&(9, 10)));
}

#[test]
fn rough_ground_costs_double_and_a_last_move_may_overrun() {
    let mut m = BattleMap::open();
    m.set((11, 10), Cell::Rough);
    // One move (two half-moves) is enough to step into rough ground, ending at zero.
    let r = m.reachable((10, 10), 2, false, &none);
    assert_eq!(r.get(&(11, 10)).map(|c| c.0), Some(4));
    // But nothing lies beyond it.
    assert!(!r.contains_key(&(12, 10)));
}

#[test]
fn the_generated_map_is_the_original_band_mirrored_and_follows_the_terrain() {
    let counts = |t, seed| {
        let mut r = Pcg64Mcg::seed_from_u64(seed);
        let m = BattleMap::generate(t, &mut r);
        let mut playable = 0;
        let mut dear = 0;
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                // The rules' frame is the original's mirrored.
                assert_eq!(
                    m.is_playable((x, y)),
                    layout::in_band(GRID_W - 1 - x, y),
                    "({x},{y})"
                );
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
    assert_eq!(counts(None, 1).0, 200);
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
fn fliers_cannot_enter_the_corners_outside_the_band() {
    let m = BattleMap::generate(
        Some(TerrainClass::Grassland),
        &mut Pcg64Mcg::seed_from_u64(1),
    );
    assert_eq!(m.enter_cost((20, 0), true), None);
    assert_eq!(m.enter_cost((20, 0), false), None);
    assert!(m.enter_cost((10, 10), true).is_some());
}

#[test]
fn deployment_blocks_are_three_columns_of_four_on_opposite_sides_and_never_overlap() {
    let att = deployment(true, 12);
    let def = deployment(false, 12);
    assert_eq!(att.len(), 12);
    assert!(att
        .iter()
        .all(|p| p.0 < CITY_X0 && layout::in_band(GRID_W - 1 - p.0, p.1)));
    assert!(def
        .iter()
        .all(|p| p.0 >= CITY_X0 && layout::in_band(GRID_W - 1 - p.0, p.1)));
    let mut seen: Vec<Pos> = att.iter().chain(&def).copied().collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 24);
    assert_eq!(deployment(true, 1), vec![(6, 12)]);
    assert!(
        !def.contains(&STRUCTURE),
        "defenders keep off a structure's square"
    );
    // A stack beyond twelve spills into columns further back, still in the band.
    let many = deployment(true, 20);
    assert_eq!(many.len(), 20);
    assert!(many.iter().all(|p| layout::in_band(GRID_W - 1 - p.0, p.1)));
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

#[test]
fn a_city_has_twelve_wall_sites_round_four_inner_squares() {
    let m = walled();
    assert!(m.is_city() && m.is_walled());
    assert_eq!(m.standing_walls().len(), 12);
    assert!(
        m.has_wall((12, 12)),
        "the gate square has a wall section too"
    );
    assert!(!m.has_wall((13, 11)), "the inside has none");
    assert!(is_wall_corner((15, 13)) && !is_wall_corner((15, 12)));
    // No walls: a city, but nothing to break or stand behind.
    let open = BattleMap::open().with_city(false);
    assert!(open.is_city() && !open.is_walled());
    assert!(open.standing_walls().is_empty());
    assert!(!BattleMap::open().is_city());
}

#[test]
fn walkers_outside_cannot_enter_a_wall_except_by_the_gate() {
    let m = walled();
    let r = m.reachable_as((10, 12), 20, walker(), &none);
    // The gate is open, and from it the inside.
    assert!(r.contains_key(&GATE));
    assert!(r.contains_key(&(13, 12)), "inside, by the gate");
    // Every other wall section is shut, as are the corner towers.
    for w in m.standing_walls() {
        assert_eq!(r.contains_key(&w), w == GATE, "{w:?}");
    }
    // Around the walls the field is open.
    assert!(r.contains_key(&(11, 10)) && r.contains_key(&(16, 9)));
}

#[test]
fn a_broken_wall_can_be_walked_over() {
    let mut m = walled();
    assert!(!m.is_breached());
    let to = (12, 11);
    assert!(!m
        .reachable_as((11, 11), 4, walker(), &none)
        .contains_key(&to));
    m.break_wall(to);
    assert!(m.is_breached());
    assert!(m
        .reachable_as((11, 11), 4, walker(), &none)
        .contains_key(&to));
    // Breaking a wall that is not there does nothing.
    m.break_wall((13, 11));
    assert_eq!(m.wall((13, 11)), Wall::None);
    assert_eq!(m.standing_walls().len(), 11);
}

#[test]
fn fliers_pass_the_wall_but_nobody_enters_a_corner_tower() {
    let m = walled();
    let r = m.reachable_as((10, 11), 6, flier(), &none);
    assert!(r.contains_key(&(12, 11)), "over the wall");
    assert!(r.contains_key(&(13, 11)));
    assert!(
        !r.contains_key(&(12, 10)) && !r.contains_key(&(12, 13)),
        "the towers"
    );
    let teleporter = Mover {
        flying: false,
        through_walls: true,
        ..Mover::default()
    };
    let r = m.reachable_as((10, 11), 6, teleporter, &none);
    assert!(r.contains_key(&(12, 11)));
    assert!(!r.contains_key(&(12, 10)));
    // A broken tower can be entered.
    let mut m = m;
    m.break_wall((12, 10));
    assert!(m
        .reachable_as((10, 10), 4, walker(), &none)
        .contains_key(&(12, 10)));
}

#[test]
fn a_unit_inside_can_only_leave_by_the_gate() {
    let m = walled();
    // Standing at the gate, it may go out; from the inside it may not step through a wall.
    let inside = m.reachable_as((13, 12), 20, walker(), &none);
    assert!(inside.contains_key(&(11, 12)), "out of the gate");
    assert!(inside.contains_key(&(10, 12)));
    // The squares beside the wall's outside are shut to it, except in front of the gate.
    assert!(!inside.contains_key(&(11, 11)) && !inside.contains_key(&(11, 13)));
    assert!(
        !inside.contains_key(&(16, 12)),
        "the far side is walled off"
    );
    // Standing on the wall at the side it can walk the wall but not step off it.
    let on_wall = m.reachable_as((14, 10), 4, walker(), &none);
    assert!(on_wall.contains_key(&(13, 10)));
    assert!(!on_wall.contains_key(&(14, 9)));
    // With a breach it can leave there.
    let mut m = m;
    m.break_wall((14, 10));
    let on_wall = m.reachable_as((14, 11), 4, walker(), &none);
    assert!(on_wall.contains_key(&(14, 9)), "out through the breach");
    assert!(
        !on_wall.contains_key(&(13, 9)),
        "other sections still stand"
    );
}

#[test]
fn city_battles_deploy_the_defenders_inside_and_the_attackers_outside() {
    let def = city_deployment(false, 12, true, false);
    assert_eq!(def.len(), 10, "the two corner towers are skipped");
    assert_eq!(def[0], GATE, "the first defender holds the gate");
    assert!(def.iter().all(|&p| in_city_area(p) && !is_wall_corner(p)));
    let open = city_deployment(false, 12, false, false);
    assert_eq!(open.len(), 12);
    let att = city_deployment(true, 9, true, false);
    assert_eq!(att.len(), 9);
    assert!(att.iter().all(|&p| !in_city_area(p) && p.0 < CITY_X0));
    let mut all = att.clone();
    all.extend(def);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 19, "nobody shares a square");
}

#[test]
fn defenders_at_a_wizards_fortress_keep_off_the_fortress_square() {
    let def = city_deployment(false, 12, false, true);
    assert_eq!(def.len(), 11);
    assert!(!def.contains(&STRUCTURE));
    assert!(city_deployment(true, 12, false, true).len() == 12);
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
    assert_eq!(ocean.enter_cost_as(cell, ship), Some(2));
    assert_eq!(ocean.enter_cost_as(cell, swimmer), Some(2));
    assert_eq!(ocean.enter_cost_as(cell, walker()), None);
    assert_eq!(ocean.enter_cost_as(cell, flier()), Some(OPEN_COST));
    assert_eq!(ocean.enter_cost_as((20, 0), flier()), None);
    // On land it is the other way about.
    let land = BattleMap::generate(
        Some(TerrainClass::Grassland),
        &mut Pcg64Mcg::seed_from_u64(1),
    );
    assert_eq!(land.enter_cost_as(cell, ship), None);
    assert!(land.enter_cost_as(cell, swimmer).is_some());
    // A ship can cross the ocean from edge to edge.
    let reach = ocean.reachable_as((14, 10), 20, ship, &|_| false);
    assert!(reach.len() > 30);
    assert!(ocean.reachable_as((14, 10), 20, walker(), &|_| false).len() == 1);
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
        decks.attackers[0].clone().unwrap(),
        decks.defenders[0].clone().unwrap(),
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
fn a_walker_jumps_the_water_between_two_decks_for_two_moves() {
    let (m, a, d) = sea_with_two_decks();
    // The attackers' deck square nearest the other one, and the square across the gap.
    let (from, to) = a
        .iter()
        .flat_map(|&p| d.iter().map(move |&q| (p, q)))
        .find(|&(p, q)| {
            distance(p, q) == 2 && (p.0 - q.0).abs() != 1 && (p.1 - q.1).abs() != 1
                || (distance(p, q) == 2 && (p.0 - q.0).abs() == 2 && p.1 == q.1)
        })
        .unwrap();
    assert!(m.is_ship_deck(from) && m.is_ship_deck(to));
    // The water in between is closed to walking.
    let gap = ((from.0 + to.0) / 2, (from.1 + to.1) / 2);
    assert_eq!(m.enter_cost(gap, false), None);
    // With the jump's two moves, it is reachable; with one move left it still is (any move
    // left lets a unit take a step); with none it is not.
    let reach = m.reachable(from, 4, false, &none);
    assert_eq!(reach.get(&to).map(|c| c.0), Some(JUMP_COST));
    assert!(m.reachable(from, 1, false, &none).contains_key(&to));
    assert!(!m.reachable(from, 0, false, &none).contains_key(&to));
    // The gap itself cannot be stood in, and a ship cannot jump.
    assert!(!reach.contains_key(&gap));
    let ship = Mover {
        sails: true,
        ..Mover::default()
    };
    assert!(!m.reachable_as(from, 4, ship, &none).contains_key(&to));
    // A unit that flies crosses the water, as it always does.
    assert!(m.reachable(from, 4, true, &none).contains_key(&gap));
}
