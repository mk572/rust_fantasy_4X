use super::*;
use crate::game::testutil::content_with_magic;
use crate::map::{Grid, Map, Plane};
use crate::player::PlayerId;

use crate::game::Command;
use crate::rules::Registry;
use crate::unit::MovesLeft;

const P0: PlayerId = PlayerId(0);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

fn world() -> Game {
    let mut c = content_with_magic();
    let mut units = c.rules.units.defs().to_vec();
    for u in units.iter_mut() {
        match u.id.as_str() {
            "walker" => {
                u.stats.insert(Stat::Construction, 1);
            }
            "slow" => {
                u.stats.insert(Stat::Construction, 2);
            }
            _ => {}
        }
    }
    c.rules.units = Registry::new_sorted(units);
    let grass = c.terrains.id_of("grassland").unwrap();
    let mut g = Game::new(4);
    g.content = c;
    g.map = Some(Map::new(Grid::new(24, 14, grass), Grid::new(24, 14, grass)));
    g.add_player(P0, "W");
    g
}

fn engineer(g: &mut Game, id: &str, pos: Position) -> hecs::Entity {
    let u = g.content.rules.units.id_of(id).unwrap();
    g.spawn_unit(P0, pos, u)
}

fn set_terrain(g: &mut Game, pos: Position, class: TerrainClass) {
    let id = g.content.terrains.first_of_class(class, pos.plane).unwrap();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(pos.plane)
        .set(pos.x, pos.y, id);
}

fn has_road(g: &Game, pos: Position) -> bool {
    g.map
        .as_ref()
        .unwrap()
        .plane(pos.plane)
        .has_road(pos.x as i32, pos.y as i32)
}

#[test]
fn terrain_decides_how_long_a_road_takes() {
    let mut g = world();
    for (class, turns) in [
        (TerrainClass::Grassland, 3),
        (TerrainClass::Desert, 4),
        (TerrainClass::Forest, 6),
        (TerrainClass::Hills, 6),
        (TerrainClass::Tundra, 6),
        (TerrainClass::Mountains, 8),
        (TerrainClass::Swamp, 8),
    ] {
        set_terrain(&mut g, at(3, 3), class);
        assert_eq!(g.turns_to_build_road(at(3, 3)), Some(turns), "{class:?}");
    }
    set_terrain(&mut g, at(3, 3), TerrainClass::Ocean);
    assert_eq!(g.turns_to_build_road(at(3, 3)), None);
    set_terrain(&mut g, at(3, 3), TerrainClass::Grassland);
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_road(3, 3, true);
    assert_eq!(
        g.turns_to_build_road(at(3, 3)),
        Some(0),
        "a road is already there"
    );
}

#[test]
fn a_river_square_needs_two_more_turns_for_its_bridge() {
    let mut g = world();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_river(3, 3, true);
    set_terrain(&mut g, at(3, 3), TerrainClass::Grassland);
    assert_eq!(g.turns_to_build_road(at(3, 3)), Some(5));
    set_terrain(&mut g, at(3, 3), TerrainClass::Forest);
    assert_eq!(g.turns_to_build_road(at(3, 3)), Some(8));
    assert_eq!(
        g.turns_to_build_road(at(4, 3)),
        Some(3),
        "no river next door"
    );
}

#[test]
fn a_road_on_a_node_takes_the_time_of_its_own_ground() {
    let mut g = world();
    for (class, turns) in [
        (TerrainClass::SorceryNode, 4),
        (TerrainClass::NatureNode, 5),
        (TerrainClass::ChaosNode, 5),
    ] {
        set_terrain(&mut g, at(3, 3), class);
        assert_eq!(g.turns_to_build_road(at(3, 3)), Some(turns), "{class:?}");
    }
}

#[test]
fn one_engineer_needs_three_turns_on_grassland() {
    let mut g = world();
    let e = engineer(&mut g, "walker", at(5, 5));
    let events = g.apply(Command::BuildRoad {
        units: vec![e],
        path: vec![],
    });
    assert!(events.is_empty(), "nothing finishes at once: {events:?}");
    assert!(!has_road(&g, at(5, 5)));
    g.process_road_work();
    g.process_road_work();
    assert!(!has_road(&g, at(5, 5)), "two turns are not enough");
    let events = g.process_road_work();
    assert!(has_road(&g, at(5, 5)));
    assert!(events.iter().any(|e| matches!(e, Event::RoadBuilt { .. })));
    assert!(g.world.get::<&RoadWork>(e).is_err(), "the work is over");
}

#[test]
fn a_strong_stack_divides_the_time_but_never_below_one_turn() {
    let mut g = world();
    let a = engineer(&mut g, "walker", at(5, 5));
    let b = engineer(&mut g, "slow", at(5, 5));
    // 3 turns / (1 + 2) points = 1 turn.
    g.apply(Command::BuildRoad {
        units: vec![a, b],
        path: vec![],
    });
    let events = g.process_road_work();
    assert!(has_road(&g, at(5, 5)), "{events:?}");
    // A mountain takes 8 / 3 = 2 turns.
    set_terrain(&mut g, at(8, 8), TerrainClass::Mountains);
    let c = engineer(&mut g, "walker", at(8, 8));
    let d = engineer(&mut g, "slow", at(8, 8));
    g.apply(Command::BuildRoad {
        units: vec![c, d],
        path: vec![],
    });
    g.process_road_work();
    assert!(!has_road(&g, at(8, 8)));
    g.process_road_work();
    assert!(has_road(&g, at(8, 8)));
}

#[test]
fn engineers_work_along_a_path() {
    let mut g = world();
    let e = engineer(&mut g, "slow", at(5, 5)); // 2 points: grassland takes 1 turn
    g.apply(Command::BuildRoad {
        units: vec![e],
        path: vec![at(5, 5), at(6, 5), at(7, 5)],
    });
    for _ in 0..4 {
        let _ = g.world.insert_one(e, MovesLeft(4));
        g.process_road_work();
    }
    for x in 5..=7 {
        assert!(has_road(&g, at(x, 5)), "x = {x}");
    }
    assert_eq!(g.units().unit_position(e), Some(at(7, 5)));
}

#[test]
fn bad_orders_are_refused_and_moving_away_cancels() {
    let mut g = world();
    let spirit = engineer(&mut g, "boat", at(5, 5));
    assert_eq!(g.build_road(&[spirit], vec![]), Err(RoadError::NoBuilders));
    assert_eq!(g.build_road(&[], vec![]), Err(RoadError::BadSelection));
    let e = engineer(&mut g, "walker", at(5, 5));
    assert_eq!(
        g.build_road(&[e], vec![at(5, 5), at(9, 9)]),
        Err(RoadError::BadPath)
    );
    set_terrain(&mut g, at(6, 5), TerrainClass::Ocean);
    assert_eq!(
        g.build_road(&[e], vec![at(5, 5), at(6, 5)]),
        Err(RoadError::CannotBuildHere)
    );
    g.build_road(&[e], vec![]).unwrap();
    let _ = g.world.insert_one(e, at(5, 6));
    g.process_road_work();
    assert!(
        g.world.get::<&RoadWork>(e).is_err(),
        "moved away: cancelled"
    );
    assert!(!has_road(&g, at(5, 5)));
}

#[test]
fn enchanted_roads_and_myrror_roads_are_free() {
    let mut g = world();
    let w = engineer(&mut g, "spirit", at(5, 5)); // the 2-move walker without roads
    let _ = w;
    let walker = engineer(&mut g, "pathfinder", at(5, 5));
    let _ = g.world.insert_one(walker, MovesLeft(2));
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        for x in 5..=7 {
            grid.set_road(x, 5, true);
        }
    }
    assert_eq!(g.check_enchant_road(at(6, 6)), Ok(()));
    assert_eq!(
        g.check_enchant_road(at(15, 10)),
        Err(TargetError::NoRoad),
        "no road within two squares"
    );
    let events = g.enchant_roads(at(6, 6));
    assert!(!events.is_empty());
    let boot = engineer(&mut g, "hardy", at(5, 5));
    let _ = g.world.insert_one(boot, MovesLeft(2));
    g.move_stack(&[boot], at(6, 5)).unwrap();
    assert_eq!(
        g.units().unit_moves_left(boot),
        2,
        "an enchanted road costs nothing"
    );
    g.move_stack(&[boot], at(7, 5)).unwrap();
    assert_eq!(g.units().unit_moves_left(boot), 2);
    // Plain roads cost half a move; Myrror's are enchanted already.
    let plain = engineer(&mut g, "hardy", at(10, 8));
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Arcanus);
        grid.set_road(10, 8, true);
        grid.set_road(11, 8, true);
    }
    let _ = g.world.insert_one(plain, MovesLeft(2));
    g.move_stack(&[plain], at(11, 8)).unwrap();
    assert_eq!(g.units().unit_moves_left(plain), 1);
    let on_myrror = Position {
        plane: Plane::Myrror,
        x: 10,
        y: 8,
    };
    let m = engineer(&mut g, "hardy", on_myrror);
    {
        let grid = g.map.as_mut().unwrap().plane_mut(Plane::Myrror);
        grid.set_road(10, 8, true);
        grid.set_road(11, 8, true);
    }
    let _ = g.world.insert_one(m, MovesLeft(2));
    g.move_stack(&[m], Position { x: 11, ..on_myrror }).unwrap();
    assert_eq!(g.units().unit_moves_left(m), 2);
}

#[test]
fn road_work_and_enchanted_roads_survive_a_save() {
    let mut g = world();
    let e = engineer(&mut g, "walker", at(5, 5));
    g.build_road(&[e], vec![at(5, 5), at(6, 5)]).unwrap();
    g.process_road_work();
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_road(2, 2, true);
    g.map
        .as_mut()
        .unwrap()
        .plane_mut(Plane::Arcanus)
        .set_enchanted_road(2, 2, true);
    let content = g.content.clone();
    let back = g.snapshot().restore(content).unwrap();
    let e2 = back.units().entity_of_serial(g.serial_of(e)).unwrap();
    let work = back.world.get::<&RoadWork>(e2).unwrap();
    assert_eq!(
        *work,
        RoadWork {
            at: at(5, 5),
            left: 2,
            rest: vec![at(6, 5)]
        }
    );
    assert!(back
        .map
        .as_ref()
        .unwrap()
        .plane(Plane::Arcanus)
        .has_enchanted_road(2, 2));
}

#[test]
fn road_turns_count_the_stack_and_say_none_on_water() {
    let mut g = world();
    let a = engineer(&mut g, "walker", at(5, 5));
    let b = engineer(&mut g, "slow", at(5, 5));
    set_terrain(&mut g, at(6, 5), TerrainClass::Ocean);
    set_terrain(&mut g, at(7, 5), TerrainClass::Mountains);
    assert_eq!(g.road_turns(&[a], at(5, 5)), Some(3));
    assert_eq!(g.road_turns(&[a, b], at(5, 5)), Some(1));
    assert_eq!(g.road_turns(&[a], at(7, 5)), Some(8));
    assert_eq!(g.road_turns(&[a], at(6, 5)), None, "no road on water");
}
