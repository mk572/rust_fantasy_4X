use super::*;
use crate::game::testutil::content_with_units;
use crate::game::{Command, Event, Rejection};
use crate::map::{Grid, Map, TerrainId};
use crate::player::PlayerId;
use crate::unit::MovesLeft;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// 12x6 world. Row 2: grass x0-1, forest x2, hills x3, mountains x4, grass x5-6, ocean x7-8,
/// grass x9-11. Everything else grassland. A tower at (1,4) on both planes.
fn world() -> Game {
    let c = content_with_units();
    let id = |n: &str| c.terrains.id_of(n).unwrap();
    let build = || {
        let mut g = Grid::new(12, 6, id("grassland"));
        g.set(2, 2, id("forest"));
        g.set(3, 2, id("hills"));
        g.set(4, 2, id("mountains"));
        g.set(7, 2, id("ocean"));
        g.set(8, 2, id("ocean"));
        g.set(1, 4, id("grassland"));
        g
    };
    let map = Map::new(build(), build());
    let mut game = Game::new(1);
    game.content = c;
    game.map = Some(map);
    game.put_feature(at(1, 4), "tower");
    for id in (0..4).map(PlayerId) {
        game.add_player(id, "");
    }
    game
}

fn unit(game: &mut Game, owner: PlayerId, name: &str, pos: Position) -> hecs::Entity {
    let id = game.content.rules.units.id_of(name).unwrap();
    game.spawn_unit(owner, pos, id)
}

fn cost(game: &Game, u: hecs::Entity, from: Position, to: Position) -> Result<u32, MoveError> {
    game.economy()
        .step_cost(u, from, to, StackTraits::default())
}

#[test]
fn terrain_costs_for_a_walker() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(1, 2));
    assert_eq!(cost(&g, w, at(1, 2), at(1, 1)), Ok(2), "grass is one move");
    assert_eq!(cost(&g, w, at(1, 2), at(2, 2)), Ok(4), "forest is two");
    assert_eq!(
        cost(&g, w, at(3, 2), at(4, 2)),
        Ok(6),
        "mountains are three"
    );
    assert_eq!(
        cost(&g, w, at(6, 2), at(7, 2)),
        Err(MoveError::Impassable),
        "no walking on water"
    );
}

#[test]
fn a_diagonal_step_costs_one_and_a_half_times_a_straight_one_when_the_setting_says_so() {
    let mut g = world();
    let walker = unit(&mut g, P0, "walker", at(5, 4));
    assert_eq!(
        cost(&g, walker, at(5, 4), at(6, 3)),
        Ok(2),
        "the same by default"
    );
    g.settings.basic.diagonal_extra_cost = true;
    assert_eq!(cost(&g, walker, at(5, 4), at(6, 4)), Ok(2));
    assert_eq!(cost(&g, walker, at(5, 4), at(6, 3)), Ok(3));
    // Forest costs 2 moves = 4 half-moves, diagonally 6.
    assert_eq!(cost(&g, walker, at(3, 1), at(2, 2)), Ok(6));
    // Across the sideways wrap too.
    assert_eq!(cost(&g, walker, at(0, 4), at(11, 3)), Ok(3));
}

#[test]
fn abilities_change_costs() {
    let mut g = world();
    let ranger = unit(&mut g, P0, "ranger", at(1, 2));
    assert_eq!(
        cost(&g, ranger, at(1, 2), at(2, 2)),
        Ok(2),
        "forester: forest costs one"
    );
    assert_eq!(
        cost(&g, ranger, at(3, 2), at(4, 2)),
        Ok(2),
        "mountaineer: mountains cost one"
    );
    let pf = unit(&mut g, P0, "pathfinder", at(1, 2));
    assert_eq!(
        cost(&g, pf, at(3, 2), at(4, 2)),
        Ok(1),
        "pathfinding: half a move anywhere"
    );
    let ghost = unit(&mut g, P0, "spirit", at(1, 2));
    assert_eq!(
        cost(&g, ghost, at(3, 2), at(4, 2)),
        Ok(1),
        "non-corporeal: half a move on land"
    );
    assert_eq!(
        cost(&g, ghost, at(6, 2), at(7, 2)),
        Err(MoveError::Impassable),
        "but not on water"
    );
    let scout = unit(&mut g, P0, "scout", at(1, 2));
    assert_eq!(
        cost(&g, scout, at(6, 2), at(7, 2)),
        Ok(2),
        "flying: one move over water"
    );
    assert_eq!(
        cost(&g, scout, at(3, 2), at(4, 2)),
        Ok(2),
        "and over mountains"
    );
}

#[test]
fn boats_stay_on_water() {
    let mut g = world();
    let boat = unit(&mut g, P0, "boat", at(7, 2));
    assert_eq!(cost(&g, boat, at(7, 2), at(8, 2)), Ok(2));
    assert_eq!(
        cost(&g, boat, at(8, 2), at(9, 2)),
        Err(MoveError::Impassable)
    );
}

#[test]
fn roads_halve_the_cost_between_road_tiles() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(0, 3));
    let map = g.map.as_mut().unwrap();
    map.plane_mut(Plane::Arcanus).set_road(0, 3, true);
    map.plane_mut(Plane::Arcanus).set_road(1, 3, true);
    assert_eq!(cost(&g, w, at(0, 3), at(1, 3)), Ok(1));
    assert_eq!(cost(&g, w, at(1, 3), at(2, 3)), Ok(2), "road ends");
}

#[test]
fn moving_spends_moves_and_explores() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(1, 2));
    assert_eq!(g.units().unit_moves_left(w), 4);
    let before = g.fog.explored_count(P0, Plane::Arcanus);
    assert_eq!(g.move_stack(&[w], at(2, 2)), Ok(Entry::Free));
    assert_eq!(g.units().unit_position(w), Some(at(2, 2)));
    assert_eq!(
        g.units().unit_moves_left(w),
        0,
        "forest used all four half-moves"
    );
    assert!(g.fog.explored_count(P0, Plane::Arcanus) > before);
    assert_eq!(g.move_stack(&[w], at(2, 1)), Err(MoveError::NoMovesLeft));
}

#[test]
fn a_unit_may_always_take_a_step_it_cannot_fully_afford() {
    let mut g = world();
    let slow = unit(&mut g, P0, "slow", at(3, 2));
    assert_eq!(g.units().unit_moves_left(slow), 2);
    assert_eq!(
        g.move_stack(&[slow], at(4, 2)),
        Ok(Entry::Free),
        "mountains cost 6, it has 2"
    );
    assert_eq!(g.units().unit_moves_left(slow), 0);
}

#[test]
fn stacks_must_be_together_and_adjacent() {
    let mut g = world();
    let a = unit(&mut g, P0, "walker", at(1, 1));
    let b = unit(&mut g, P0, "walker", at(5, 1));
    assert_eq!(
        g.move_stack(&[a, b], at(2, 1)),
        Err(MoveError::BadSelection)
    );
    assert_eq!(g.move_stack(&[a], at(4, 1)), Err(MoveError::NotAdjacent));
    assert_eq!(g.move_stack(&[], at(1, 1)), Err(MoveError::NoUnits));
}

#[test]
fn the_world_wraps_sideways() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(0, 1));
    assert_eq!(g.move_stack(&[w], at(11, 1)), Ok(Entry::Free));
}

#[test]
fn enemies_block_and_ask_for_a_fight() {
    let mut g = world();
    let mine = unit(&mut g, P0, "walker", at(1, 1));
    let theirs = unit(&mut g, P1, "walker", at(2, 1));
    let events: Vec<_> = g
        .apply(Command::MoveStack {
            units: vec![mine],
            to: at(2, 1),
        })
        .into_iter()
        .filter(|e| !matches!(e, Event::Diplomacy(_)))
        .collect();
    assert_eq!(
        events,
        vec![Event::AttackRequested {
            attackers: vec![mine],
            from: at(1, 1),
            to: at(2, 1),
            defenders: vec![theirs]
        }]
    );
    assert_eq!(
        g.units().unit_position(mine),
        Some(at(1, 1)),
        "nobody moved"
    );
    assert_eq!(g.units().unit_moves_left(mine), 4, "and nothing was spent");
}

#[test]
fn a_tile_holds_twenty_units() {
    let mut g = world();
    for _ in 0..crate::unit::MAX_TILE_LIMIT {
        unit(&mut g, P0, "walker", at(2, 1));
    }
    let mover = unit(&mut g, P0, "walker", at(1, 1));
    assert_eq!(g.move_stack(&[mover], at(2, 1)), Err(MoveError::StackFull));
}

#[test]
fn the_setting_lowers_the_units_a_tile_holds() {
    let mut g = world();
    g.settings.rules.max_tile = 12;
    for _ in 0..12 {
        unit(&mut g, P0, "walker", at(2, 1));
    }
    let mover = unit(&mut g, P0, "walker", at(1, 1));
    assert_eq!(g.max_tile(), 12);
    assert_eq!(g.move_stack(&[mover], at(2, 1)), Err(MoveError::StackFull));
    g.settings.rules.max_tile = 13;
    assert!(g.move_stack(&[mover], at(2, 1)).is_ok());
}

#[test]
fn wind_walking_carries_the_whole_stack_over_water() {
    let mut g = world();
    let mage = unit(&mut g, P0, "mage", at(6, 2));
    let walker = unit(&mut g, P0, "walker", at(6, 2));
    assert_eq!(
        g.move_stack(&[walker], at(7, 2)),
        Err(MoveError::Impassable),
        "alone it cannot"
    );
    assert_eq!(g.move_stack(&[mage, walker], at(7, 2)), Ok(Entry::Free));
    assert_eq!(g.units().unit_position(walker), Some(at(7, 2)));
}

#[test]
fn a_stack_with_a_wind_walker_moves_as_far_as_the_wind_walker_can() {
    let mut g = world();
    let mage = unit(&mut g, P0, "mage", at(5, 4));
    let walker = unit(&mut g, P0, "walker", at(5, 4));
    // The Wind Walker has three moves (the spell's Min), the walker only the one it was given.
    let _ = g.world.insert_one(mage, MovesLeft(6));
    let _ = g.world.insert_one(walker, MovesLeft(2));
    for x in [6, 7, 8] {
        assert_eq!(g.move_stack(&[mage, walker], at(x, 4)), Ok(Entry::Free));
    }
    assert_eq!(g.units().unit_position(walker), Some(at(8, 4)));
    assert_eq!(
        g.units().unit_moves_left(walker),
        0,
        "it has what the walker has left"
    );
    assert_eq!(
        g.move_stack(&[mage, walker], at(9, 4)),
        Err(MoveError::NoMovesLeft)
    );
}

#[test]
fn fliers_and_wind_walkers_see_two_squares_at_least() {
    let mut g = world();
    let mage = unit(&mut g, P0, "mage", at(5, 4));
    let walker = unit(&mut g, P0, "walker", at(5, 4));
    assert_eq!(g.units().unit_sight(walker), 3);
    assert_eq!(g.units().unit_sight(mage), 5);
}

#[test]
fn water_walking_carries_only_the_walker_over_water() {
    let mut g = world();
    let strider = unit(&mut g, P0, "strider", at(6, 2));
    let walker = unit(&mut g, P0, "walker", at(6, 2));
    assert_eq!(
        g.move_stack(&[walker], at(7, 2)),
        Err(MoveError::Impassable)
    );
    assert_eq!(g.move_stack(&[strider], at(7, 2)), Ok(Entry::Free));
    assert_eq!(g.units().unit_position(strider), Some(at(7, 2)));
    assert_eq!(g.units().unit_position(walker), Some(at(6, 2)));
}

#[test]
fn shifting_planes_needs_a_tower_or_planar_travel() {
    let mut g = world();
    let on_tower = unit(&mut g, P0, "walker", at(1, 4));
    let off_tower = unit(&mut g, P0, "walker", at(3, 4));
    let myrror = Position {
        plane: Plane::Myrror,
        ..at(1, 4)
    };
    assert_eq!(g.shift_plane(&[on_tower]), Ok(myrror));
    assert_eq!(g.units().unit_moves_left(on_tower), 2, "costs one move");
    assert_eq!(
        g.shift_plane(&[off_tower]),
        Err(MoveError::CannotShiftPlane)
    );
    let t = unit(&mut g, P0, "traveller", at(3, 4));
    assert!(g.shift_plane(&[t]).is_ok(), "planar travel works anywhere");
    assert_eq!(g.units().unit_position(t).unwrap().plane, Plane::Myrror);
}

#[test]
fn a_shift_into_a_blocked_tile_is_refused() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(1, 4));
    unit(
        &mut g,
        P1,
        "walker",
        Position {
            plane: Plane::Myrror,
            ..at(1, 4)
        },
    );
    assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));
}

#[test]
fn paths_go_around_water_and_across_the_seam() {
    let mut g = world();
    // Make the water a wall: ocean down the whole of columns 7..=8.
    let ocean = g.content.terrains.id_of("ocean").unwrap();
    for y in 0..6 {
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(7, y, ocean);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(8, y, ocean);
    }
    let w = unit(&mut g, P0, "walker", at(5, 2));
    // East is walled off, but the world wraps: west across the seam reaches x=9.
    let path = g.find_path(&[w], at(9, 2)).expect("a way round the seam");
    assert_eq!(path.last(), Some(&at(9, 2)));
    assert!(path.len() >= 8, "the long way round: {path:?}");
    let scout = unit(&mut g, P0, "scout", at(5, 2));
    assert_eq!(
        g.find_path(&[scout], at(9, 2)).unwrap().len(),
        4,
        "flying goes straight over"
    );
    // Walled both sides: no path for a walker once the seam is closed too.
    for y in 0..6 {
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(11, y, ocean);
        g.map
            .as_mut()
            .unwrap()
            .plane_mut(Plane::Arcanus)
            .set(0, y, ocean);
    }
    assert_eq!(g.find_path(&[w], at(9, 2)), None);
}

#[test]
fn path_prefers_cheap_terrain() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(1, 2));
    // Straight east along row 2 runs through forest, hills and mountains; row 1 is grass.
    let path = g.find_path(&[w], at(6, 2)).unwrap();
    assert!(
        path.iter().all(|p| p.y != 2 || p.x == 6),
        "avoids the rough row: {path:?}"
    );
}

#[test]
fn goto_orders_continue_over_turns() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(0, 1));
    let events = g.apply(Command::GoTo {
        units: vec![w],
        to: at(5, 1),
    });
    let moved = events
        .iter()
        .filter(|e| matches!(e, Event::UnitsMoved { .. }))
        .count();
    assert_eq!(moved, 2, "two grass tiles with four half-moves");
    assert_eq!(g.units().unit_position(w).unwrap().x, 2);
    assert!(
        g.world.get::<&crate::unit::Order>(w).is_ok(),
        "order remains"
    );
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_position(w).unwrap().x, 4);
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_position(w), Some(at(5, 1)));
    assert!(
        g.world.get::<&crate::unit::Order>(w).is_err(),
        "arrived: order cleared"
    );
}

#[test]
fn end_turn_refills_moves() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(1, 1));
    g.world.insert_one(w, MovesLeft(0)).unwrap();
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_moves_left(w), 4);
}

#[test]
fn rejections_are_reported() {
    let mut g = world();
    let w = unit(&mut g, P0, "walker", at(6, 2));
    assert_eq!(
        g.apply(Command::MoveStack {
            units: vec![w],
            to: at(7, 2)
        }),
        vec![Event::Rejected(Rejection::Move(MoveError::Impassable))]
    );
}

#[test]
fn stats_include_abilities_and_levels() {
    use crate::rules::Stat;
    let mut g = world();
    let hardy = unit(&mut g, P0, "hardy", at(1, 1));
    let plain = unit(&mut g, P0, "walker", at(2, 1));
    assert_eq!(g.units().unit_stat(hardy, Stat::Defense), 4, "tough adds 2");
    assert_eq!(g.units().unit_stat(plain, Stat::Defense), 2);
    assert_eq!(g.units().unit_stat(plain, Stat::Melee), 3);
    g.world
        .insert_one(plain, crate::unit::Experience(25))
        .unwrap();
    assert_eq!(g.units().unit_level(plain), 1);
    assert_eq!(g.units().unit_stat(plain, Stat::Melee), 4, "veteran adds 1");
    assert_eq!(g.units().unit_hits(plain), (6, 6));
    g.world.insert_one(plain, crate::unit::Hurt(2)).unwrap();
    assert_eq!(g.units().unit_hits(plain), (4, 6));
    let _ = TerrainId(0);
}

#[test]
fn a_pathfinder_gives_the_whole_stack_road_speed() {
    let mut g = world();
    let walker = g.content.rules.units.id_of("walker").unwrap();
    let pathfinder = g.content.rules.units.id_of("pathfinder").unwrap();
    let player = PlayerId(0);
    let alone = g.spawn_unit(player, at(1, 2), walker);
    let plain = g
        .economy()
        .step_cost(alone, at(1, 2), at(2, 2), StackTraits::default())
        .unwrap();
    assert_eq!(plain, 4, "forest costs two moves");
    let guide = g.spawn_unit(player, at(1, 2), pathfinder);
    let traits = g.economy().stack_traits(&[alone, guide]);
    assert!(traits.pathfinding);
    assert_eq!(
        g.economy().step_cost(alone, at(1, 2), at(2, 2), traits),
        Ok(1)
    );
    // The ability only counts for the stack it is in.
    assert_eq!(
        g.economy().step_cost(
            alone,
            at(1, 2),
            at(2, 2),
            g.economy().stack_traits(&[alone])
        ),
        Ok(4)
    );
}

#[test]
fn a_step_is_planned_from_a_hand_made_world_without_a_game() {
    let c = content_with_units();
    let (grass, forest) = (
        c.terrains.id_of("grassland").unwrap(),
        c.terrains.id_of("forest").unwrap(),
    );
    let build = || {
        let mut g = Grid::new(12, 6, grass);
        g.set(3, 2, forest);
        g
    };
    let map = Map::new(build(), build());
    let walker = c.rules.units.id_of("walker").unwrap();
    let mut world = hecs::World::new();
    let owner = world.spawn(crate::player::bundle(P0));
    let unit = crate::unit::spawn(
        &mut world,
        &c,
        crate::game::serial::Serial(1),
        owner,
        at(2, 2),
        walker,
    );
    let economy = Economy::new(&world, Some(&map), &c);
    let (costs, entry) = economy.plan_step(&[unit], at(3, 2)).unwrap();
    assert_eq!(
        (costs, entry),
        (vec![4], Entry::Free),
        "a forest costs two moves"
    );
    let (costs, _) = economy.plan_step(&[unit], at(1, 2)).unwrap();
    assert_eq!(costs, vec![2], "grassland costs one");
    assert_eq!(
        economy.plan_step(&[unit], at(5, 2)),
        Err(MoveError::NotAdjacent)
    );
}
