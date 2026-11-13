use super::*;
use crate::game::testutil::{num, shipped};
use crate::game::{Command, Event, Rejection};
use crate::map::{Grid, Map, TerrainId};
use crate::player::PlayerId;
use crate::unit::MovesLeft;
use f4x_def::testdir::TestResult;

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
    let c = shipped();
    let id = |n: u16| c.terrains.id_numbered(n).expect("id_numbered succeeds");
    let build = || {
        let mut g = Grid::new(12, 6, id(num::terrain::GRASSLAND));
        g.set(2, 2, id(num::terrain::FOREST));
        g.set(3, 2, id(num::terrain::HILLS));
        g.set(4, 2, id(num::terrain::MOUNTAINS));
        g.set(7, 2, id(num::terrain::OCEAN));
        g.set(8, 2, id(num::terrain::OCEAN));
        g.set(1, 4, id(num::terrain::GRASSLAND));
        g
    };
    let map = Map::new(build(), build());
    let mut game = Game::new(1);
    game.content = c;
    game.map = Some(map);
    game.put_feature(at(1, 4), num::feature::TOWER);
    for id in (0..4).map(PlayerId) {
        game.add_player(id, "");
    }
    game
}

fn unit(game: &mut Game, owner: PlayerId, name: u16, pos: Position) -> hecs::Entity {
    let id = game
        .content
        .rules
        .units
        .id_numbered(name)
        .expect("id_of succeeds");
    game.spawn_unit(owner, pos, id)
}

fn cost(game: &Game, u: hecs::Entity, from: Position, to: Position) -> Result<HalfInt, MoveError> {
    game.economy()
        .step_cost(u, from, to, StackTraits::default())
}

#[test]
fn terrain_costs_for_a_walker() {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 2));
    assert_eq!(
        cost(&g, w, at(1, 2), at(1, 1)),
        Ok(HalfInt::from_int(1)),
        "grass is one move"
    );
    assert_eq!(
        cost(&g, w, at(1, 2), at(2, 2)),
        Ok(HalfInt::from_int(2)),
        "forest is two"
    );
    assert_eq!(
        cost(&g, w, at(3, 2), at(4, 2)),
        Ok(HalfInt::from_int(4)),
        "mountains are four"
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
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 4));
    assert_eq!(
        cost(&g, walker, at(5, 4), at(6, 3)),
        Ok(HalfInt::from_int(1)),
        "the same by default"
    );
    g.settings.basic.diagonal_extra_cost = true;
    assert_eq!(
        cost(&g, walker, at(5, 4), at(6, 4)),
        Ok(HalfInt::from_int(1))
    );
    assert_eq!(
        cost(&g, walker, at(5, 4), at(6, 3)),
        Ok(HalfInt::from_raw(3))
    );
    // Forest costs 2 moves = 4 half-moves, diagonally 6.
    assert_eq!(
        cost(&g, walker, at(3, 1), at(2, 2)),
        Ok(HalfInt::from_int(3))
    );
    // Across the sideways wrap too.
    assert_eq!(
        cost(&g, walker, at(0, 4), at(11, 3)),
        Ok(HalfInt::from_raw(3))
    );
}

#[test]
fn abilities_change_costs() {
    let mut g = world();
    let ranger = unit(&mut g, P0, 16 /* ranger */, at(1, 2));
    assert_eq!(
        cost(&g, ranger, at(1, 2), at(2, 2)),
        Ok(HalfInt::from_int(1)),
        "forester: forest costs one"
    );
    assert_eq!(
        cost(&g, ranger, at(3, 2), at(4, 2)),
        Ok(HalfInt::from_int(1)),
        "mountaineer: mountains cost one"
    );
    let pf = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 2));
    g.grant_temp_ability(crate::rules::exec::Target::Unit(pf), Ability::new(50), None);
    assert_eq!(
        cost(&g, pf, at(3, 2), at(4, 2)),
        Ok(HalfInt::from_raw(1)),
        "pathfinding: half a move anywhere"
    );
    let ghost = unit(&mut g, P0, 154 /* magic_spirit */, at(1, 2));
    assert_eq!(
        cost(&g, ghost, at(3, 2), at(4, 2)),
        Ok(HalfInt::from_raw(1)),
        "non-corporeal: half a move on land"
    );
    assert_eq!(
        cost(&g, ghost, at(6, 2), at(7, 2)),
        Err(MoveError::Impassable),
        "but not on water"
    );
    let scout = unit(&mut g, P0, 103 /* high_elf_pegasai */, at(1, 2));
    assert_eq!(
        cost(&g, scout, at(6, 2), at(7, 2)),
        Ok(HalfInt::from_int(1)),
        "flying: one move over water"
    );
    assert_eq!(
        cost(&g, scout, at(3, 2), at(4, 2)),
        Ok(HalfInt::from_int(1)),
        "and over mountains"
    );
}

#[test]
fn boats_stay_on_water() {
    let mut g = world();
    let boat = unit(&mut g, P0, 38 /* warship */, at(7, 2));
    assert_eq!(cost(&g, boat, at(7, 2), at(8, 2)), Ok(HalfInt::from_int(1)));
    assert_eq!(
        cost(&g, boat, at(8, 2), at(9, 2)),
        Err(MoveError::Impassable)
    );
}

#[test]
fn roads_halve_the_cost_between_road_tiles() -> TestResult {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(0, 3));
    let map = g.map.as_mut().ok_or("as_mut is missing")?;
    map.plane_mut(Plane::Arcanus).set_road(0, 3, true);
    map.plane_mut(Plane::Arcanus).set_road(1, 3, true);
    assert_eq!(cost(&g, w, at(0, 3), at(1, 3)), Ok(HalfInt::from_raw(1)));
    assert_eq!(
        cost(&g, w, at(1, 3), at(2, 3)),
        Ok(HalfInt::from_int(1)),
        "road ends"
    );
    Ok(())
}

#[test]
fn moving_spends_moves_and_explores() {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 2));
    assert_eq!(g.units().unit_moves_left(w), 2);
    let before = g.fog.explored_count(P0, Plane::Arcanus);
    assert_eq!(g.move_stack(&[w], at(2, 2)), Ok(Entry::Free));
    assert_eq!(g.units().unit_position(w), Some(at(2, 2)));
    assert_eq!(g.units().unit_moves_left(w), 0, "forest used both moves");
    assert!(g.fog.explored_count(P0, Plane::Arcanus) > before);
    assert_eq!(g.move_stack(&[w], at(2, 1)), Err(MoveError::NoMovesLeft));
}

#[test]
fn a_unit_may_always_take_a_step_it_cannot_fully_afford() {
    let mut g = world();
    let slow = unit(&mut g, P0, 104 /* high_men_spearmen */, at(3, 2));
    assert_eq!(g.units().unit_moves_left(slow), 1);
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
    let a = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    let b = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 1));
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
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(0, 1));
    assert_eq!(g.move_stack(&[w], at(11, 1)), Ok(Entry::Free));
}

#[test]
fn enemies_block_and_ask_for_a_fight() {
    let mut g = world();
    let mine = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    let theirs = unit(&mut g, P1, 107 /* high_men_cavalry */, at(2, 1));
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
    assert_eq!(g.units().unit_moves_left(mine), 2, "and nothing was spent");
}

#[test]
fn a_tile_holds_twenty_units() {
    let mut g = world();
    for _ in 0..crate::unit::MAX_TILE_LIMIT {
        unit(&mut g, P0, 107 /* high_men_cavalry */, at(2, 1));
    }
    let mover = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    assert_eq!(g.move_stack(&[mover], at(2, 1)), Err(MoveError::StackFull));
}

#[test]
fn a_path_goes_round_a_square_that_has_no_room_for_the_stack() -> TestResult {
    let mut g = world();
    for _ in 0..g.max_tile() {
        unit(&mut g, P0, 107 /* high_men_cavalry */, at(2, 1));
    }
    let mover = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    let path = g.find_path(&[mover], at(4, 1)).ok_or("no path")?;
    assert!(
        !path.contains(&at(2, 1)),
        "through the full square: {path:?}"
    );
    // The stack is not stopped at the edge of the full square: the order moves it.
    let events = g.apply(Command::GoTo {
        units: vec![mover],
        to: at(4, 1),
    });
    assert!(
        events.iter().any(|e| matches!(e, Event::UnitsMoved { .. })),
        "{events:?}"
    );
    Ok(())
}

#[test]
fn an_order_stopped_by_a_refused_step_says_why() -> TestResult {
    let mut g = world();
    let mover = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    // The path was planned when the square had room; it filled up since.
    g.world
        .insert_one(mover, crate::unit::Order::GoTo(vec![at(2, 1), at(3, 1)]))?;
    for _ in 0..g.max_tile() {
        unit(&mut g, P0, 107 /* high_men_cavalry */, at(2, 1));
    }
    let events = g.follow_orders(&[mover]);
    assert_eq!(
        events,
        vec![Event::OrderFailed {
            player: P0,
            units: vec![mover],
            at: at(1, 1),
            to: at(2, 1),
            why: MoveError::StackFull,
        }]
    );
    assert!(g.world.get::<&crate::unit::Order>(mover).is_err());
    // Running out of moves is no failure: the order stays for the next turn.
    let mut g = world();
    let mover = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    g.world.insert_one(
        mover,
        crate::unit::Order::GoTo(vec![at(2, 1), at(3, 1), at(4, 1)]),
    )?;
    let events = g.follow_orders(&[mover]);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::OrderFailed { .. })),
        "{events:?}"
    );
    Ok(())
}

#[test]
fn the_setting_lowers_the_units_a_tile_holds() {
    let mut g = world();
    g.settings.rules.max_tile = 12;
    for _ in 0..12 {
        unit(&mut g, P0, 107 /* high_men_cavalry */, at(2, 1));
    }
    let mover = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    assert_eq!(g.max_tile(), 12);
    assert_eq!(g.move_stack(&[mover], at(2, 1)), Err(MoveError::StackFull));
    g.settings.rules.max_tile = 13;
    assert!(g.move_stack(&[mover], at(2, 1)).is_ok());
}

#[test]
fn wind_walking_carries_the_whole_stack_over_water() {
    let mut g = world();
    let mage = unit(&mut g, P0, 15 /* wind_mage */, at(6, 2));
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(6, 2));
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
    let mage = unit(&mut g, P0, 15 /* wind_mage */, at(5, 4));
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 4));
    // The Wind Walker has three moves (the spell's Min), the walker only the one it was given.
    let _ = g.world.insert_one(mage, MovesLeft(HalfInt::from_int(3)));
    let _ = g.world.insert_one(walker, MovesLeft(HalfInt::from_int(1)));
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

/// Insecticide IW017: the stack's allowance is the Wind Walker's whichever unit has more, so a
/// swift unit with more moves left cannot take the stack further and nothing is left to select.
#[test]
fn a_swifter_unit_does_not_take_a_wind_walking_stack_further() {
    let mut g = world();
    let mage = unit(&mut g, P0, 15 /* wind_mage */, at(5, 4));
    let swift = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 4));
    let _ = g.world.insert_one(mage, MovesLeft(HalfInt::from_int(1)));
    let _ = g.world.insert_one(swift, MovesLeft(HalfInt::from_int(3)));
    assert_eq!(g.move_stack(&[mage, swift], at(6, 4)), Ok(Entry::Free));
    assert_eq!(g.units().unit_moves_left(mage), 0);
    assert_eq!(
        g.units().unit_moves_left(swift),
        0,
        "the stack shows one allowance, the Wind Walker's"
    );
    assert_eq!(
        g.move_stack(&[mage, swift], at(7, 4)),
        Err(MoveError::NoMovesLeft)
    );
}

#[test]
fn fliers_and_wind_walkers_see_two_squares_at_least() {
    let mut g = world();
    let mage = unit(&mut g, P0, 15 /* wind_mage */, at(5, 4));
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 4));
    assert_eq!(g.units().unit_sight(walker), HalfInt::from_raw(3));
    assert_eq!(g.units().unit_sight(mage), HalfInt::from_raw(5));
}

#[test]
fn water_walking_carries_only_the_walker_over_water() {
    let mut g = world();
    let strider = unit(&mut g, P0, 107 /* high_men_cavalry */, at(6, 2));
    g.grant_temp_ability(
        crate::rules::exec::Target::Unit(strider),
        crate::rules::Ability::new(71),
        None,
    );
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(6, 2));
    assert_eq!(
        g.move_stack(&[walker], at(7, 2)),
        Err(MoveError::Impassable)
    );
    assert_eq!(g.move_stack(&[strider], at(7, 2)), Ok(Entry::Free));
    assert_eq!(g.units().unit_position(strider), Some(at(7, 2)));
    assert_eq!(g.units().unit_position(walker), Some(at(6, 2)));
}

#[test]
fn shifting_planes_needs_a_tower_or_planar_travel() -> TestResult {
    let mut g = world();
    let on_tower = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 4));
    let off_tower = unit(&mut g, P0, 107 /* high_men_cavalry */, at(3, 4));
    let myrror = Position {
        plane: Plane::Myrror,
        ..at(1, 4)
    };
    assert_eq!(g.shift_plane(&[on_tower]), Ok(myrror));
    assert_eq!(g.units().unit_moves_left(on_tower), 1, "costs one move");
    assert_eq!(
        g.shift_plane(&[off_tower]),
        Err(MoveError::CannotShiftPlane)
    );
    let t = unit(&mut g, P0, 171 /* shadow_demons */, at(3, 4));
    assert!(g.shift_plane(&[t]).is_ok(), "planar travel works anywhere");
    assert_eq!(
        g.units()
            .unit_position(t)
            .ok_or("unit_position is missing")?
            .plane,
        Plane::Myrror
    );
    Ok(())
}

#[test]
fn a_shift_into_a_blocked_tile_is_refused() {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 4));
    unit(
        &mut g,
        P1,
        107, /* high_men_cavalry */
        Position {
            plane: Plane::Myrror,
            ..at(1, 4)
        },
    );
    assert_eq!(g.shift_plane(&[w]), Err(MoveError::CannotShiftPlane));
}

#[test]
fn paths_go_around_water_and_across_the_seam() -> TestResult {
    let mut g = world();
    // Make the water a wall: ocean down the whole of columns 7..=8.
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .ok_or("ocean is not in the content")?;
    for y in 0..6 {
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(7, y, ocean);
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(8, y, ocean);
    }
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 2));
    // East is walled off, but the world wraps: west across the seam reaches x=9.
    let path = g.find_path(&[w], at(9, 2)).expect("a way round the seam");
    assert_eq!(path.last(), Some(&at(9, 2)));
    assert!(path.len() >= 8, "the long way round: {path:?}");
    let scout = unit(&mut g, P0, 103 /* high_elf_pegasai */, at(5, 2));
    assert_eq!(
        g.find_path(&[scout], at(9, 2))
            .ok_or("find_path is missing")?
            .len(),
        4,
        "flying goes straight over"
    );
    // Walled both sides: no path for a walker once the seam is closed too.
    for y in 0..6 {
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(11, y, ocean);
        g.map
            .as_mut()
            .ok_or("as_mut is missing")?
            .plane_mut(Plane::Arcanus)
            .set(0, y, ocean);
    }
    assert_eq!(g.find_path(&[w], at(9, 2)), None);
    Ok(())
}

#[test]
fn path_prefers_cheap_terrain() -> TestResult {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 2));
    // Straight east along row 2 runs through forest, hills and mountains; row 1 is grass.
    let path = g.find_path(&[w], at(6, 2)).ok_or("find_path is missing")?;
    assert!(
        path.iter().all(|p| p.y != 2 || p.x == 6),
        "avoids the rough row: {path:?}"
    );
    Ok(())
}

#[test]
fn goto_orders_continue_over_turns() -> TestResult {
    let mut g = world();
    // A hero eats no food, and the treasury pays its gold.
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold =
        f4x_def::half_int::HalfInt::from_int(1000);
    let w = unit(&mut g, P0, 0 /* dwarf */, at(0, 1));
    let events = g.apply(Command::GoTo {
        units: vec![w],
        to: at(5, 1),
    });
    let moved = events
        .iter()
        .filter(|e| matches!(e, Event::UnitsMoved { .. }))
        .count();
    assert_eq!(moved, 2, "two grass tiles with two moves");
    assert_eq!(
        g.units()
            .unit_position(w)
            .ok_or("unit_position is missing")?
            .x,
        2
    );
    assert!(
        g.world.get::<&crate::unit::Order>(w).is_ok(),
        "order remains"
    );
    g.apply(Command::EndTurn);
    assert_eq!(
        g.units()
            .unit_position(w)
            .ok_or("unit_position is missing")?
            .x,
        4
    );
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_position(w), Some(at(5, 1)));
    assert!(
        g.world.get::<&crate::unit::Order>(w).is_err(),
        "arrived: order cleared"
    );
    Ok(())
}

#[test]
fn end_turn_refills_moves() -> TestResult {
    let mut g = world();
    g.treasury_mut(P0).ok_or("treasury_mut is missing")?.gold =
        f4x_def::half_int::HalfInt::from_int(1000);
    let w = unit(&mut g, P0, 0 /* dwarf */, at(1, 1));
    g.world.insert_one(w, MovesLeft(HalfInt::from_int(0)))?;
    g.apply(Command::EndTurn);
    assert_eq!(g.units().unit_moves_left(w), 2);
    Ok(())
}

#[test]
fn rejections_are_reported() {
    let mut g = world();
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(6, 2));
    assert_eq!(
        g.apply(Command::MoveStack {
            units: vec![w],
            to: at(7, 2)
        }),
        vec![Event::Rejected(Rejection::Move(MoveError::Impassable))]
    );
}

#[test]
fn stats_include_abilities_and_levels() -> TestResult {
    use crate::rules::Stat;
    let mut g = world();
    let plain = unit(&mut g, P0, 107 /* high_men_cavalry */, at(2, 1));
    let (melee, defense) = (
        g.units().unit_stat(plain, Stat::Melee),
        g.units().unit_stat(plain, Stat::Defense),
    );
    // The real level path: Regular (20 xp) adds one to melee.
    g.world.insert_one(plain, crate::unit::Experience(25))?;
    assert_eq!(g.units().unit_level(plain), 1);
    assert_eq!(g.units().unit_stat(plain, Stat::Melee), melee + 1);
    assert_eq!(g.units().unit_stat(plain, Stat::Defense), defense);
    // Veteran (60 xp) adds one to defense.
    g.world.insert_one(plain, crate::unit::Experience(60))?;
    assert_eq!(g.units().unit_stat(plain, Stat::Defense), defense + 1);
    let (hits, max) = g.units().unit_hits(plain);
    assert_eq!(hits, max);
    g.world.insert_one(plain, crate::unit::Hurt(2))?;
    assert_eq!(g.units().unit_hits(plain), (max - 2, max));
    let _ = TerrainId(0);
    Ok(())
}

#[test]
fn a_pathfinder_gives_the_whole_stack_road_speed() -> TestResult {
    let mut g = world();
    let walker = g
        .content
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
    let player = PlayerId(0);
    let alone = g.spawn_unit(player, at(1, 2), walker);
    let plain = g
        .economy()
        .step_cost(alone, at(1, 2), at(2, 2), StackTraits::default())
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(plain, 2, "forest costs two moves");
    let guide = g.spawn_unit(player, at(1, 2), walker);
    g.grant_temp_ability(
        crate::rules::exec::Target::Unit(guide),
        Ability::new(50),
        None,
    );
    let traits = g.economy().stack_traits(&[alone, guide]);
    assert!(traits.pathfinding);
    assert_eq!(
        g.economy().step_cost(alone, at(1, 2), at(2, 2), traits),
        Ok(HalfInt::from_raw(1))
    );
    // The ability only counts for the stack it is in.
    assert_eq!(
        g.economy().step_cost(
            alone,
            at(1, 2),
            at(2, 2),
            g.economy().stack_traits(&[alone])
        ),
        Ok(HalfInt::from_int(2))
    );
    Ok(())
}

#[test]
fn a_step_is_planned_from_a_hand_made_world_without_a_game() -> TestResult {
    let c = shipped();
    let (grass, forest) = (
        c.terrains
            .id_numbered(num::terrain::GRASSLAND)
            .ok_or("grassland is not in the content")?,
        c.terrains
            .id_numbered(num::terrain::FOREST)
            .ok_or("forest is not in the content")?,
    );
    let build = || {
        let mut g = Grid::new(12, 6, grass);
        g.set(3, 2, forest);
        g
    };
    let map = Map::new(build(), build());
    let walker = c
        .rules
        .units
        .id_numbered(107)
        .ok_or("high_men_cavalry is not in the content")?;
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
    let (costs, entry) = economy
        .plan_step(&[unit], at(3, 2))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        (costs, entry),
        (vec![HalfInt::from_int(2)], Entry::Free),
        "a forest costs two moves"
    );
    let (costs, _) = economy
        .plan_step(&[unit], at(1, 2))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(costs, vec![HalfInt::from_int(1)], "grassland costs one");
    assert_eq!(
        economy.plan_step(&[unit], at(5, 2)),
        Err(MoveError::NotAdjacent)
    );
    Ok(())
}

#[test]
fn a_square_has_eight_neighbours_wrapping_east_and_west_and_stopping_at_the_poles() {
    let g = world();
    let near = |x, y| crate::unit::pathing::neighbours(&g, at(x, y));
    assert_eq!(near(5, 2).len(), 8);
    let seam = near(0, 2);
    assert_eq!(seam.len(), 8);
    assert!(seam.contains(&at(11, 1)) && seam.contains(&at(11, 3)));
    assert_eq!(near(6, 0).len(), 5, "no row above the top");
    assert_eq!(near(6, 5).len(), 5, "and none below the bottom");
    assert!(near(0, 0).contains(&at(11, 1)));
    assert!(
        crate::unit::pathing::neighbours(&Game::new(1), at(1, 1)).is_empty(),
        "no map"
    );
}

#[test]
fn a_path_takes_the_short_way_over_the_seam_in_both_directions() -> TestResult {
    let mut g = world();
    // Row 1 is open grass; the world is 12 squares wide, so x = 11 and x = 1 are two steps apart.
    let east = unit(&mut g, P0, 107 /* high_men_cavalry */, at(11, 1));
    let path = g
        .find_path(&[east], at(1, 1))
        .ok_or("no way over the seam")?;
    assert_eq!(path.len(), 2, "two steps: {path:?}");
    assert_eq!(path[0].x, 0, "over the seam first: {path:?}");
    assert_eq!(path.last(), Some(&at(1, 1)));
    let west = unit(&mut g, P0, 107 /* high_men_cavalry */, at(1, 1));
    let back = g.find_path(&[west], at(11, 1)).ok_or("no way back")?;
    assert_eq!(back.len(), 2, "two steps: {back:?}");
    assert_eq!(back[0].x, 0, "over the seam first: {back:?}");
    assert_eq!(back.last(), Some(&at(11, 1)));
    // A diagonal over the seam too.
    let diagonal = g.find_path(&[east], at(0, 2)).ok_or("no diagonal")?;
    assert_eq!(diagonal, vec![at(0, 2)]);
    Ok(())
}

#[test]
fn only_the_seam_leads_to_the_other_side_when_the_middle_is_walled() -> TestResult {
    let mut g = world();
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .ok_or("ocean is not in the content")?;
    // Water in columns 5 and 6: from x = 8 the squares at x = 2 are reached only the other way round.
    for x in [5, 6] {
        for y in 0..6 {
            g.map
                .as_mut()
                .ok_or("no map")?
                .plane_mut(Plane::Arcanus)
                .set(x, y, ocean);
        }
    }
    // A hero eats no food, and the treasury pays its gold (as in the goto test).
    let w = unit(&mut g, P0, 0 /* dwarf */, at(8, 1));
    let path = g.find_path(&[w], at(2, 1)).ok_or("no way over the seam")?;
    assert_eq!(path.last(), Some(&at(2, 1)));
    assert!(
        path.contains(&at(0, 1)) || path.contains(&at(0, 0)) || path.contains(&at(0, 2)),
        "it crosses the seam: {path:?}"
    );
    assert!(g.reachable_squares(&[w]).contains(&at(2, 1)));
    assert!(g.reaches_any(&[w], &[at(2, 1)]));
    // The order walks the stack over the seam, turn after turn.
    g.treasury_mut(P0).ok_or("no treasury")?.gold = f4x_def::half_int::HalfInt::from_int(1000);
    let mut events = g.apply(Command::GoTo {
        units: vec![w],
        to: at(2, 1),
    });
    for _ in 0..8 {
        events.extend(g.apply(Command::EndTurn));
    }
    let moved: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::UnitsMoved { .. } | Event::OrderFailed { .. }))
        .collect();
    assert_eq!(g.units().unit_position(w), Some(at(2, 1)), "{moved:?}");
    Ok(())
}

/// The 12x6 world with every square of both planes the given terrain.
fn flat_world(terrain: u16) -> Game {
    let mut g = world();
    let id = g
        .content
        .terrains
        .id_numbered(terrain)
        .expect("the terrain exists");
    let map = g.map.as_mut().expect("the world has a map");
    for plane in [Plane::Arcanus, Plane::Myrror] {
        for y in 0..6 {
            for x in 0..12 {
                map.plane_mut(plane).set(x, y, id);
            }
        }
    }
    g
}

#[test]
fn on_open_ground_every_path_has_the_length_of_the_wrapped_distance() -> TestResult {
    let mut g = flat_world(num::terrain::GRASSLAND);
    let width = 12;
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(0, 0));
    for sy in 0..6 {
        for sx in 0..12 {
            let start = at(sx, sy);
            g.world
                .insert_one(walker, start)
                .map_err(|e| format!("{e:?}"))?;
            for gy in 0..6 {
                for gx in 0..12 {
                    let goal = at(gx, gy);
                    let path = g.find_path(&[walker], goal);
                    if goal == start {
                        assert_eq!(path, None, "no path to where it stands");
                        continue;
                    }
                    let path = path.ok_or_else(|| format!("no path {start:?} to {goal:?}"))?;
                    let dx = sx.abs_diff(gx);
                    let expected = dx.min(width - dx).max(sy.abs_diff(gy)) as usize;
                    assert_eq!(path.len(), expected, "{start:?} to {goal:?}: {path:?}");
                    assert_eq!(path.last(), Some(&goal));
                    let mut from = start;
                    for &step in &path {
                        assert!(adjacent(width, from, step), "{from:?} to {step:?}");
                        from = step;
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn on_open_ground_the_walk_reaches_every_square_from_any_column() -> TestResult {
    let mut g = flat_world(num::terrain::GRASSLAND);
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(0, 3));
    for sx in 0..12 {
        g.world
            .insert_one(walker, at(sx, 3))
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(g.reachable_squares(&[walker]).len(), 72, "from column {sx}");
        // The nearest square over the seam is found by the early stop too.
        let over = at((sx + 11) % 12, 5);
        assert!(g.reaches_any(&[walker], &[over]));
    }
    Ok(())
}

#[test]
fn a_stack_steps_over_the_seam_in_both_directions() -> TestResult {
    let mut g = flat_world(num::terrain::GRASSLAND);
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(11, 1));
    g.move_stack(&[w], at(0, 1)).map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.units().unit_position(w), Some(at(0, 1)));
    g.move_stack(&[w], at(11, 2))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(g.units().unit_position(w), Some(at(11, 2)), "diagonal back");
    Ok(())
}

#[test]
fn ships_sail_over_the_seam() -> TestResult {
    let mut g = flat_world(num::terrain::OCEAN);
    let boat = unit(&mut g, P0, 35 /* trireme */, at(10, 3));
    let path = g.find_path(&[boat], at(2, 3)).ok_or("no sea way")?;
    assert_eq!(
        path.len(),
        4,
        "10 to 2 over the seam is four squares: {path:?}"
    );
    assert!(path.iter().any(|p| p.x == 11 || p.x == 0));
    assert!(g.reachable_squares(&[boat]).contains(&at(1, 5)));
    Ok(())
}

#[test]
fn a_road_over_the_seam_halves_the_cost_of_the_step() -> TestResult {
    let mut g = flat_world(num::terrain::GRASSLAND);
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(11, 1));
    let plain = cost(&g, w, at(11, 1), at(0, 1)).map_err(|e| format!("{e:?}"))?;
    for x in [11, 0] {
        g.map
            .as_mut()
            .ok_or("no map")?
            .plane_mut(Plane::Arcanus)
            .set_road(x, 1, true);
    }
    let road = cost(&g, w, at(11, 1), at(0, 1)).map_err(|e| format!("{e:?}"))?;
    assert!(road < plain, "road {road:?} against {plain:?}");
    assert_eq!(
        cost(&g, w, at(0, 1), at(11, 1)).map_err(|e| format!("{e:?}"))?,
        road,
        "the same both ways"
    );
    Ok(())
}

#[test]
fn a_hostile_stack_on_the_seam_is_gone_round_not_through() -> TestResult {
    let mut g = flat_world(num::terrain::GRASSLAND);
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(11, 1));
    g.make_contact(P0, P1);
    g.declare_war(P0, P1);
    unit(&mut g, P1, 107 /* high_men_cavalry */, at(0, 1));
    let path = g.find_path(&[w], at(1, 1)).ok_or("no way round")?;
    assert!(!path.contains(&at(0, 1)), "through the enemy: {path:?}");
    assert_eq!(path.len(), 2, "round it over the seam: {path:?}");
    // And the walk does not enter it either.
    assert!(!g.reachable_squares(&[w]).contains(&at(0, 1)));
    Ok(())
}

#[test]
fn a_walker_reaches_the_land_round_the_water_and_a_flier_all_of_it() -> TestResult {
    let mut g = world();
    let ocean = g
        .content
        .terrains
        .id_numbered(num::terrain::OCEAN)
        .ok_or("ocean is not in the content")?;
    // A wall of water in columns 7 and 8 and again in 11 and 0: the seam is closed too.
    for x in [7, 8, 11, 0] {
        for y in 0..6 {
            g.map
                .as_mut()
                .ok_or("no map")?
                .plane_mut(Plane::Arcanus)
                .set(x, y, ocean);
        }
    }
    let walker = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 2));
    let flier = unit(&mut g, P0, 103 /* high_elf_pegasai */, at(5, 2));
    let walked = g.reachable_squares(&[walker]);
    assert!(walked.contains(&at(5, 2)), "the start is included");
    assert!(walked.contains(&at(1, 5)) && walked.contains(&at(6, 0)));
    assert!(!walked.contains(&at(9, 2)), "behind the water");
    assert!(!walked.contains(&at(7, 3)), "not into the water");
    assert!(g.reachable_squares(&[flier]).contains(&at(9, 2)));
    assert!(g.reachable_squares(&[]).is_empty());
    // The walk and the path agree: the squares are the ones a path is found to.
    for square in &walked {
        if *square != at(5, 2) {
            assert!(g.find_path(&[walker], *square).is_some(), "{square:?}");
        }
    }
    assert!(g.find_path(&[walker], at(9, 2)).is_none());
    // The stopping search gives the answer of the whole one, for every square of the plane.
    for x in 0..12 {
        for y in 0..6 {
            let square = at(x, y);
            for u in [walker, flier] {
                assert_eq!(
                    g.reaches_any(&[u], &[square]),
                    g.reachable_squares(&[u]).contains(&square),
                    "{square:?}"
                );
            }
        }
    }
    assert!(!g.reaches_any(&[walker], &[]), "no targets, no way");
    assert!(
        g.reaches_any(&[walker], &[at(9, 2), at(1, 5)]),
        "one of two"
    );
    assert!(
        !g.reaches_any(&[walker], &[at(9, 2), at(10, 2)]),
        "none of two"
    );
    // Two units of a kind on a square share a signature; another kind does not.
    let twin = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 2));
    assert_eq!(g.reach_signature(&[walker]), g.reach_signature(&[twin]));
    assert_ne!(g.reach_signature(&[walker]), g.reach_signature(&[flier]));
    assert_eq!(
        g.reach_signature(&[walker, flier]),
        g.reach_signature(&[flier, walker]),
        "the order of the stack does not matter"
    );
    Ok(())
}

#[test]
fn a_hostile_stack_stops_the_walk_but_can_be_the_goal() -> TestResult {
    let mut g = world();
    let me = unit(&mut g, P0, 107 /* high_men_cavalry */, at(5, 3));
    let foe = unit(&mut g, P1, 107 /* high_men_cavalry */, at(7, 3));
    let walked = g.reachable_squares(&[me]);
    assert!(!walked.contains(&at(7, 3)), "not into the foe's square");
    assert!(walked.contains(&at(8, 3)), "but round it");
    let path = g.find_path(&[me], at(7, 3)).ok_or("no path to the foe")?;
    assert_eq!(path.last(), Some(&at(7, 3)), "attacking it is the goal");
    assert!(g.world.contains(foe));
    Ok(())
}

#[test]
fn a_path_takes_as_many_turns_as_the_moves_it_costs() -> TestResult {
    let mut g = world();
    // The turns follow from the unit's allowance and the half moves a grassland step costs.
    let w = unit(&mut g, P0, 107 /* high_men_cavalry */, at(0, 1));
    let allowance = g.units().unit_max_moves(w);
    let straight: Vec<_> = (1..=6).map(|x| at(x, 1)).collect();
    assert_eq!(g.path_turns(&[w], &[]), 0, "nowhere to go");
    assert_eq!(g.path_turns(&[], &straight), 0, "nobody to go");
    let one = g.path_turns(&[w], &straight[..1]);
    assert_eq!(one, 0, "the first step is paid for by this turn's moves");
    let long = g.path_turns(&[w], &straight);
    let step = 2; // a grassland step, in half moves (raw)
    let expected = (straight.len() as u32 * step).saturating_sub(1) / allowance.raw() as u32;
    assert_eq!(long, expected, "allowance {allowance}");
    // With no moves left the first step waits for next turn.
    let _ = g.world.insert_one(w, MovesLeft(HalfInt::from_int(0)));
    assert_eq!(g.path_turns(&[w], &straight[..1]), 1);
    Ok(())
}
