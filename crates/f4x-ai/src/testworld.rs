//! A small hand-made world with the shipped content, for the unit tests of the AI modules: two
//! islands in an ocean, one wizard on each, so that a unit cannot walk to the other side.

use f4x_core::content::Content;
use f4x_core::feature::entity::Placement;
use f4x_core::feature::lair::Lair;
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;

use f4x_core::game::{Command, Game};

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

const WIDTH: u32 = 40;
const HEIGHT: u32 = 14;
/// The west island (player 0) spans columns 3 to 10, the east island (player 1) 28 to 35; both
/// span rows 3 to 10.
const WEST: std::ops::RangeInclusive<u32> = 3..=10;
const EAST: std::ops::RangeInclusive<u32> = 28..=35;
const ROWS: std::ops::RangeInclusive<u32> = 3..=10;

/// The shipped content, parsed once for all the tests of the process; each call hands out a copy
/// because a `Game` owns its content.
pub fn content() -> Content {
    static SHIPPED: std::sync::OnceLock<Content> = std::sync::OnceLock::new();
    SHIPPED
        .get_or_init(|| {
            f4x_data::load_content(std::path::Path::new("../../content"))
                .expect("the rules data loads")
        })
        .clone()
}

pub fn at(x: u32, y: u32) -> Position {
    Position {
        plane: Plane::Arcanus,
        x,
        y,
    }
}

/// The capital of player 0, on the east shore of the west island.
pub const COAST_CITY: (u32, u32) = (10, 6);
/// A city of player 0 away from the sea.
pub const INLAND_CITY: (u32, u32) = (6, 6);
/// The capital of player 1, on the west shore of the east island.
pub const ENEMY_CITY: (u32, u32) = (28, 6);

/// Two islands, a city for each player at the given squares of the player-0 island and
/// the east island, and nothing else.
pub fn game(city0: (u32, u32), race: &str) -> Game {
    let content = content();
    let ocean = content.terrains.id_of("ocean").expect("ocean");
    let grass = content.terrains.id_of("grassland").expect("grassland");
    let build = || {
        let mut grid = Grid::new(WIDTH, HEIGHT, ocean);
        for y in ROWS {
            for x in WEST.chain(EAST) {
                grid.set(x, y, grass);
            }
        }
        grid
    };
    let race_id = content.rules.races.id_of(race).expect("the race exists");
    let mut game = Game::new(7);
    game.set_content(content);
    game.set_map(Map::new(build(), build()));
    game.add_player(P0, "West");
    game.add_player(P1, "East");
    for (owner, (x, y), name) in [(P0, city0, "Westport"), (P1, ENEMY_CITY, "Eastport")] {
        let events = game.apply(Command::FoundCity {
            owner,
            pos: at(x, y),
            name: name.into(),
            race: Some(race_id),
            population: 8,
        });
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, f4x_core::game::Event::Rejected(_))),
            "city {name} not founded: {events:?}"
        );
    }
    game
}

/// Spawn `count` units of `unit` at `pos` for `owner`.
pub fn spawn(
    game: &mut Game,
    owner: PlayerId,
    pos: Position,
    unit: &str,
    count: usize,
) -> Vec<hecs::Entity> {
    let id = game
        .content()
        .rules
        .units
        .id_of(unit)
        .unwrap_or_else(|| panic!("no unit {unit}"));
    (0..count)
        .map(|_| game.spawn_unit(owner, pos, id))
        .collect()
}

/// An intact encounter site at `pos` with these guards (unit id, count) and 300 gold. The players
/// in `fought_by` have fought for it before, so they know its guards.
pub fn spawn_site(
    game: &mut Game,
    pos: Position,
    guards: Vec<(String, u32)>,
    fought_by: Vec<PlayerId>,
) -> Result<hecs::Entity, &'static str> {
    let feature = game
        .content()
        .features
        .id_of("monster_lair")
        .ok_or("monster_lair is shipped")?;
    Ok(game.spawn_feature(Placement {
        at: pos,
        feature,
        node: None,
        lair: Some(Lair {
            intact: true,
            guards,
            gold: 300,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by,
        }),
    }))
}

/// Give the city at `pos` the buildings (by id), in the order given.
pub fn build(game: &mut Game, pos: Position, buildings: &[&str]) {
    let city = game.economy().city_at(pos).expect("a city there");
    for id in buildings {
        let b = game
            .content()
            .rules
            .buildings
            .id_of(id)
            .unwrap_or_else(|| panic!("no building {id}"));
        game.scenario_set_building(city, b, true);
    }
}
