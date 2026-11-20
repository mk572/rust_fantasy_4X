use super::*;
use crate::game::testutil::shipped;
use crate::map::{Grid, Map, Plane};
use f4x_def::half_int::HalfInt;
use f4x_def::quarter_int::QuarterInt;

const P0: PlayerId = PlayerId(0);

fn world() -> Game {
    let c = shipped();
    let grass = c
        .terrains
        .id_numbered(4 /* grassland */)
        .expect("id_of succeeds");
    let ocean = c
        .terrains
        .id_numbered(8 /* ocean */)
        .expect("id_of succeeds");
    let build = || {
        let mut g = Grid::new(16, 10, grass);
        for y in 0..10 {
            g.set(0, y, ocean);
        }
        g
    };
    let mut game = Game::new(5);
    game.set_content(c);
    game.map = Some(Map::new(build(), build()));
    game.add_player(P0, "Merlin");
    game
}

fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

#[test]
fn unexplored_land_counts_for_nothing_and_is_flagged() {
    let mut g = world();
    let before = g.site_survey(P0, at(8, 5));
    assert!(before.unexplored);
    assert_eq!(before.max_size, 0);
    g.explore(P0, at(8, 5), HalfInt::from_raw(7));
    let after = g.site_survey(P0, at(8, 5));
    assert!(!after.unexplored);
    assert!(after.max_size > 0);
    assert_eq!(after.blocked, None);
}

#[test]
fn the_shore_adds_ten_percent_gold_and_water_cannot_be_settled() {
    let mut g = world();
    g.explore(P0, at(1, 5), HalfInt::from_raw(7));
    assert_eq!(g.site_survey(P0, at(1, 5)).gold_percent, 10);
    assert_eq!(g.site_survey(P0, at(8, 5)).gold_percent, 0);
    assert!(g.site_survey(P0, at(0, 5)).blocked.is_some());
}

/// Task 494: a shared square of 0.5 food gives a quarter, and the quarters of the area are added
/// up before they are rounded down (design.md, "Half-units").
#[test]
fn shared_half_food_squares_add_up_in_quarters_before_the_size_is_rounded_down() {
    let mut g = world();
    let c = shipped();
    let swamp = c
        .terrains
        .id_numbered(10 /* swamp */)
        .expect("id_of succeeds");
    let map = g.map.as_mut().expect("the map");
    for y in 0..10 {
        for x in 1..16 {
            map.plane_mut(Plane::Arcanus).set(x, y, swamp);
        }
    }
    g.explore(P0, at(8, 5), HalfInt::from_raw(40));
    let eco = g.economy();
    let ctx = crate::city::economy::SquareCtx::SITE;
    let grid = g.map.as_ref().expect("the map").plane(Plane::Arcanus);
    let whole = eco.square_yield(grid, 8, 5, false, ctx).terrain_food;
    let shared = eco.square_yield(grid, 8, 5, true, ctx).terrain_food;
    assert_eq!(whole, QuarterInt::from_half(HalfInt::HALF));
    assert_eq!(shared, QuarterInt::QUARTER);
    let area = eco.city_area(at(8, 5));
    let quarters: i32 = area
        .iter()
        .map(|&(_, is_shared)| if is_shared { shared } else { whole }.raw())
        .sum();
    assert_eq!(g.site_survey(P0, at(8, 5)).max_size as i32, quarters / 4);
}
