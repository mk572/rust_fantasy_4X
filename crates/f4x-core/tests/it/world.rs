//! The shipped content and a small game on it, for the tests that set up their own situation.

use std::sync::OnceLock;

use f4x_core::content::Content;
use f4x_core::game::Game;
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;

/// The number of grassland in `content/terrain.ron`.
const GRASSLAND: u16 = 4;

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

/// The shipped content, parsed once for the whole test run.
pub fn shipped() -> &'static Content {
    static CONTENT: OnceLock<Content> = OnceLock::new();
    CONTENT.get_or_init(|| {
        f4x_data::load_content(std::path::Path::new("../../content"))
            .expect("the shipped content loads")
    })
}

pub fn at(plane: Plane, x: u32, y: u32) -> Position {
    Position { plane, x, y }
}

/// A game on the shipped rules with a flat grassland map of `width` by `height` squares on both
/// planes and a player for each of `players`.
pub fn flat_game(
    seed: u64,
    width: u32,
    height: u32,
    players: &[PlayerId],
) -> Result<Game, &'static str> {
    let grass = shipped()
        .terrains
        .id_numbered(GRASSLAND)
        .ok_or("grassland is not in the content")?;
    let mut g = Game::new(seed);
    g.set_content(shipped().clone());
    g.set_map(Map::new(
        Grid::new(width, height, grass),
        Grid::new(width, height, grass),
    ));
    for &p in players {
        g.add_player(p, "Wizard");
    }
    Ok(g)
}
