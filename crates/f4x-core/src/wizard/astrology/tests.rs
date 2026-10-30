use super::*;
use crate::game::testutil::content_with_economy;
use crate::map::{Grid, Map};
use crate::player::PlayerId;
use crate::testdir::TestResult;

fn world() -> Game {
    let c = content_with_economy();
    let grass = c.terrains.id_of("grassland").expect("id_of succeeds");
    let map = Map::new(Grid::new(12, 8, grass), Grid::new(12, 8, grass));
    let mut game = Game::new(3);
    game.content = c;
    game.map = Some(map);
    game.add_player(PlayerId(0), "Merlin");
    game.add_player(PlayerId(1), "Raven");
    game
}

#[test]
fn each_turn_adds_a_point_to_every_wizards_history() -> TestResult {
    let mut g = world();
    g.record_history();
    g.record_history();
    assert!(g.player_ids().iter().all(|&id| g
        .player_component::<crate::player::History>(id)
        .expect("value succeeds")
        .0
        .len()
        == 2));
    Ok(())
}

#[test]
fn the_history_keeps_the_last_288_turns() -> TestResult {
    let mut g = world();
    for _ in 0..HISTORY_TURNS + 5 {
        g.record_history();
    }
    assert!(g.player_ids().iter().all(|&id| g
        .player_component::<crate::player::History>(id)
        .expect("value succeeds")
        .0
        .len()
        == HISTORY_TURNS));
    Ok(())
}

#[test]
fn scores_are_cut_back_to_their_limits() {
    let g = world();
    for (_, a, power) in g.astrology_table() {
        assert!(a.power <= 200 && a.strength <= 200 && a.research <= 200);
        assert!(power <= 160);
    }
}
