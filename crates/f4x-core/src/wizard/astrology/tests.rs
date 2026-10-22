use super::*;
use crate::game::testutil::content_with_economy;
use crate::map::{Grid, Map};
use crate::player::{Player, PlayerId};

fn world() -> Game {
    let c = content_with_economy();
    let grass = c.terrains.id_of("grassland").unwrap();
    let map = Map::new(Grid::new(12, 8, grass), Grid::new(12, 8, grass));
    let mut game = Game::new(3);
    game.content = c;
    game.map = Some(map);
    game.players.push(Player::new(PlayerId(0), "Merlin"));
    game.players.push(Player::new(PlayerId(1), "Raven"));
    game
}

#[test]
fn each_turn_adds_a_point_to_every_wizards_history() {
    let mut g = world();
    g.record_history();
    g.record_history();
    assert!(g.players.iter().all(|p| p.history.len() == 2));
}

#[test]
fn the_history_keeps_the_last_288_turns() {
    let mut g = world();
    for _ in 0..HISTORY_TURNS + 5 {
        g.record_history();
    }
    assert!(g.players.iter().all(|p| p.history.len() == HISTORY_TURNS));
}

#[test]
fn scores_are_cut_back_to_their_limits() {
    let g = world();
    for (_, a, power) in g.astrology_table() {
        assert!(a.power <= 200 && a.strength <= 200 && a.research <= 200);
        assert!(power <= 160);
    }
}
