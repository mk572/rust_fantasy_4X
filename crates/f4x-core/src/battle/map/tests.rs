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
                    if m.enter_cost((x, y), false).unwrap() > OPEN_COST {
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
    let def = city_deployment(false, 12, true);
    assert_eq!(def.len(), 10, "the two corner towers are skipped");
    assert_eq!(def[0], GATE, "the first defender holds the gate");
    assert!(def.iter().all(|&p| in_city_area(p) && !is_wall_corner(p)));
    let open = city_deployment(false, 12, false);
    assert_eq!(open.len(), 12);
    let att = city_deployment(true, 9, true);
    assert_eq!(att.len(), 9);
    assert!(att.iter().all(|&p| !in_city_area(p) && p.0 < CITY_X0));
    let mut all = att.clone();
    all.extend(def);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 19, "nobody shares a square");
}
