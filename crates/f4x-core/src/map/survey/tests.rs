use super::*;
use crate::game::testutil::content_with_economy;
use crate::map::{Grid, Map, Plane};
use crate::player::Player;

const P0: PlayerId = PlayerId(0);

fn world() -> Game {
    let c = content_with_economy();
    let grass = c.terrains.id_of("grassland").unwrap();
    let ocean = c.terrains.id_of("ocean").unwrap();
    let build = || {
        let mut g = Grid::new(16, 10, grass);
        for y in 0..10 {
            g.set(0, y, ocean);
        }
        g
    };
    let mut game = Game::new(5);
    game.content = c;
    game.map = Some(Map::new(build(), build()));
    game.players.push(Player::new(P0, "Merlin"));
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
    g.explore(P0, at(8, 5), 3);
    let after = g.site_survey(P0, at(8, 5));
    assert!(!after.unexplored);
    assert!(after.max_size > 0);
    assert_eq!(after.blocked, None);
}

#[test]
fn the_shore_adds_ten_percent_gold_and_water_cannot_be_settled() {
    let mut g = world();
    g.explore(P0, at(1, 5), 3);
    assert_eq!(g.site_survey(P0, at(1, 5)).gold_percent, 10);
    assert_eq!(g.site_survey(P0, at(8, 5)).gold_percent, 0);
    assert!(g.site_survey(P0, at(0, 5)).blocked.is_some());
}
