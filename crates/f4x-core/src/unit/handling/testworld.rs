//! A small hand-made world with the shipped content, for the tests of the unit handling (a copy
//! of the AI's test world): two islands in an ocean, one wizard on each, so that a unit cannot walk
//! to the other side.
//!
//! Tests: none, because this is test support; the modules that use it test it.

use super::{Handler, Host, Judgement, SettlerFate};
use crate::feature::entity::Placement;
use crate::feature::lair::Lair;
use crate::map::{Grid, Map, Plane, Position};
use crate::player::view::PlayerView;
use crate::player::PlayerId;
use f4x_def::content::Content;

use crate::world::testutil::Table;
use crate::world::{Command, Event, World};
use std::cell::RefCell;
use std::rc::Rc;

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

const WIDTH: u32 = 40;
const HEIGHT: u32 = 14;
/// The west island (player 0) spans columns 3 to 10, the east island (player 1) 28 to 35; both
/// span rows 3 to 10.
const WEST: std::ops::RangeInclusive<u32> = 3..=10;
const EAST: std::ops::RangeInclusive<u32> = 28..=35;
const ROWS: std::ops::RangeInclusive<u32> = 3..=10;

/// The shipped content, parsed once for all the tests of the process.
pub use f4x_def::testdir::shipped;

/// The world under test with its dice and the round being played.
pub struct Game {
    table: Table,
    round: u32,
}

impl std::ops::Deref for Game {
    type Target = World;

    fn deref(&self) -> &World {
        &self.table.world
    }
}

impl std::ops::DerefMut for Game {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.table.world
    }
}

impl Host for Game {
    fn command(&mut self, command: Command) -> Vec<Event> {
        self.table.apply(command)
    }

    fn round(&self) -> u32 {
        self.round
    }
}

impl Game {
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        self.table.apply(command)
    }

    pub fn command(&mut self, command: Command) -> Vec<Event> {
        self.table.apply(command)
    }

    pub fn set_round(&mut self, round: u32) {
        self.round = round;
    }

    /// The change of rounds for everyone at once, with nobody playing, in the order
    /// `f4x-master`'s `play_round` runs the world's turn operations.
    pub fn play_round(&mut self) -> Vec<Event> {
        let players = self.player_ids();
        let next = self.round + 1;
        let (world, rng) = self.table.world_and_rng();
        let events = world.advance(|world| {
            let mut events = Vec::new();
            for &p in &players {
                if !world.is_dormant(p) {
                    events.extend(world.end_player_turn(p, rng));
                }
            }
            events.extend(world.end_round(rng));
            events.extend(world.start_round(next, rng));
            for &p in &players {
                if !world.is_dormant(p) {
                    events.extend(world.start_player_turn(p, next, rng));
                }
            }
            events
        });
        self.round = next;
        events
    }
}

/// The choices of a plain owner, standing in for a computer player's: a city keeps one soldier,
/// and a second scout is wanted while it builds up.
#[derive(Default)]
pub struct Plain {
    /// The settlers that lost their site, for the tests.
    pub fates: RefCell<Vec<SettlerFate>>,
}

/// Until this round the owner of [`Plain`] builds up.
const BUILD_UP: u32 = 100;

impl Judgement for Plain {
    fn garrison(
        &self,
        view: &PlayerView,
        _at: Position,
        soldiers: &[hecs::Entity],
        _round: u32,
    ) -> Vec<hecs::Entity> {
        let mut kept = soldiers.to_vec();
        view.sort_by_serial(&mut kept);
        kept.truncate(1);
        kept
    }

    fn scouts_wanted(&self, view: &PlayerView, round: u32) -> usize {
        match view.my_cities().len() {
            0 => 0,
            _ => 1 + usize::from(self.building_up(view, round)),
        }
    }

    fn building_up(&self, _view: &PlayerView, round: u32) -> bool {
        round < BUILD_UP
    }

    fn settler_lost(&self, fate: SettlerFate) {
        self.fates.borrow_mut().push(fate);
    }
}

/// A handler for `player` with the choices of [`Plain`].
pub fn handler(player: PlayerId) -> Handler {
    Handler::judged(player, Rc::new(Plain::default()))
}

/// A copy of the shipped content, for a `World`, which owns its content.
pub fn content() -> Content {
    shipped().clone()
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
pub fn game(city0: (u32, u32), race: u16) -> Game {
    game_seeded(city0, race, 7)
}

/// [`game`] whose dice start from `seed`.
pub fn game_seeded(city0: (u32, u32), race: u16, seed: u64) -> Game {
    build_game(city0, race, seed, Plane::Arcanus, &[])
}

/// [`game`] with the sea between the islands filled but for the column of x 27: the two
/// islands are one land mass with a strait of one sea square (rows 3 to 10) in it, and the sea
/// round the land is open.
pub fn game_with_strait(city0: (u32, u32), race: u16) -> Game {
    build_game_in(city0, race, 7, Plane::Arcanus, &[], true)
}

fn build_game(
    city0: (u32, u32),
    race: u16,
    seed: u64,
    rival_plane: Plane,
    rivers: &[(u32, u32)],
) -> Game {
    build_game_in(city0, race, seed, rival_plane, rivers, false)
}

fn build_game_in(
    city0: (u32, u32),
    race: u16,
    seed: u64,
    rival_plane: Plane,
    rivers: &[(u32, u32)],
    strait: bool,
) -> Game {
    let content = content();
    let ocean = content.terrains.id_numbered(8).expect("ocean");
    let grass = content.terrains.id_numbered(4).expect("grassland");
    let build = |rivers: &[(u32, u32)]| {
        let mut grid = Grid::new(WIDTH, HEIGHT, ocean);
        for y in ROWS {
            for x in WEST.chain(EAST) {
                grid.set(x, y, grass);
            }
            if strait {
                for x in (*WEST.end() + 1)..*EAST.start() - 1 {
                    grid.set(x, y, grass);
                }
            }
        }
        for &(x, y) in rivers {
            grid.set_river(x, y, true);
        }
        grid
    };
    let race_id = content
        .rules
        .races
        .id_numbered(race)
        .expect("the race exists");
    let mut world = Table::empty(seed);
    world.set_content(content);
    // The tests of the AI play with the default stack limit, which is higher than a battle side;
    // the v1.31 rule set of the content takes the original's nine, with no reserve to test.
    world.scenario_change_settings(|s| s.rules.max_tile = crate::world::setting::DEFAULT_MAX_TILE);
    world.set_map(Map::new(build(rivers), build(&[])));
    world.add_player(P0, "West");
    world.add_player(P1, "East");
    for (owner, (x, y), name) in [(P0, city0, "Westport"), (P1, ENEMY_CITY, "Eastport")] {
        let events = world.apply(Command::FoundCity {
            owner,
            pos: Position {
                plane: if owner == P1 {
                    rival_plane
                } else {
                    Plane::Arcanus
                },
                ..at(x, y)
            },
            name: name.into(),
            race: Some(race_id),
            population: 8,
        });
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, crate::world::Event::Rejected(_))),
            "city {name} not founded: {events:?}"
        );
    }
    Game {
        table: world,
        round: 1,
    }
}

/// Spawn `count` units of `unit` at `pos` for `owner`.
pub fn spawn(
    world: &mut World,
    owner: PlayerId,
    pos: Position,
    unit: u16,
    count: usize,
) -> Vec<hecs::Entity> {
    let id = world
        .content()
        .rules
        .units
        .id_numbered(unit)
        .unwrap_or_else(|| panic!("no unit {unit}"));
    (0..count)
        .map(|_| world.spawn_unit(owner, pos, id))
        .collect()
}

/// An intact encounter site at `pos` with these guards (unit id, count) and 300 gold. The players
/// in `fought_by` have fought for it before, so they know its guards.
pub fn spawn_site(
    world: &mut World,
    pos: Position,
    guards: Vec<(f4x_def::rules::UnitId, u32)>,
    fought_by: Vec<PlayerId>,
) -> Result<hecs::Entity, &'static str> {
    let feature = world
        .content()
        .features
        .id_numbered(13)
        .ok_or("monster_lair is shipped")?;
    Ok(world.spawn_feature(Placement {
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
            hurt: Vec::new(),
        }),
    }))
}

/// A tower at `pos`, guarded by one zombie while `intact`.
pub fn spawn_tower(
    world: &mut World,
    pos: Position,
    intact: bool,
) -> Result<hecs::Entity, &'static str> {
    let feature = world
        .content()
        .features
        .id_numbered(21)
        .ok_or("tower is shipped")?;
    Ok(world.spawn_feature(Placement {
        at: pos,
        feature,
        node: None,
        lair: Some(Lair {
            intact,
            guards: vec![(f4x_def::rules::UnitId::new(174) /* zombies */, 1)],
            gold: 0,
            mana: 0,
            items: Vec::new(),
            prisoner: false,
            special: 0,
            fought_by: Vec::new(),
            hurt: Vec::new(),
        }),
    }))
}

/// Give the city at `pos` the buildings (by id), in the order given.
pub fn build(world: &mut World, pos: Position, buildings: &[u16]) {
    let city = world.economy().city_at(pos).expect("a city there");
    for id in buildings {
        let b = world
            .content()
            .rules
            .buildings
            .id_numbered(*id)
            .unwrap_or_else(|| panic!("no building {id}"));
        world.scenario_set_building(city, b, true);
    }
}

/// The west island filled with cities of player 0, as close as the rules let them stand.
pub fn fill_west(w: &mut dyn Host) {
    let race = w.content().rules.races.id_numbered(8);
    for x in 3..=10 {
        for y in 3..=10 {
            w.command(Command::FoundCity {
                owner: P0,
                pos: at(x, y),
                name: format!("C{x}{y}"),
                race,
                population: 5,
            });
        }
    }
}

/// The first square, row by row, in the columns given where a city could be founded.
fn site_in(world: &World, columns: std::ops::RangeInclusive<u32>) -> Position {
    let view = PlayerView::new(world, P0);
    ROWS.flat_map(|y| columns.clone().map(move |x| at(x, y)))
        .find(|&p| view.can_found_city(p))
        .expect("a place for a city")
}

/// A site for a city of player 0 on its own island.
pub fn site_at_home(world: &World) -> Position {
    site_in(world, WEST)
}

/// A site for a city on the other island, which no walk of player 0 leads to.
pub fn site_over_the_sea(world: &World) -> Position {
    site_in(world, EAST)
}

/// Give a settler its site, as whoever chooses sites does: the settle role with the site, and the
/// tag for a ship when no walk leads there.
pub fn give_site(handler: &mut Handler, world: &World, settler: hecs::Entity, site: Position) {
    super::roles::register(handler, world, 1);
    let serial = PlayerView::new(world, handler.player).serial_of(settler);
    handler
        .roles
        .claim(serial, super::roles::Role::Settle, 1, Some(site))
        .expect("a settler free to take a site");
    if world.find_path(&[settler], site).is_none() {
        handler
            .roles
            .set_crossing(serial, Some(super::roles::Crossing::Sea));
    }
}
