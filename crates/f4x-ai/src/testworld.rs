//! A small hand-made world with the shipped content, for the unit tests of the AI modules: two
//! islands in an ocean, one wizard on each, so that a unit cannot walk to the other side.
//!
//! Tests: none, because this is test support; the modules that use it test it.

use f4x_core::feature::entity::Placement;
use f4x_core::feature::lair::Lair;
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;
use f4x_def::content::Content;
use f4x_master::Game;

use f4x_core::world::testutil::Table;
use f4x_core::world::{Command, World};

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

/// A copy of the shipped content, for a `World`, which owns its content.
pub fn content() -> Content {
    shipped().clone()
}

/// A new game as the game itself sets it up (wizards, cities, heroes), on the shipped content.
pub fn fresh_game() -> Result<Game, Box<dyn std::error::Error>> {
    Ok(Game::from(f4x_newgame::setup::new_game(
        shipped(),
        &f4x_newgame::setup::NewGame {
            seed: 7,
            ..Default::default()
        },
    )?))
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

/// [`game`] with the east wizard's city on Myrror: no rival on the plane of the west wizard.
pub fn game_rival_across(city0: (u32, u32), race: u16) -> Game {
    build_game(city0, race, 7, Plane::Myrror, &[])
}

/// [`game`] whose dice start from `seed`.
pub fn game_seeded(city0: (u32, u32), race: u16, seed: u64) -> Game {
    build_game(city0, race, seed, Plane::Arcanus, &[])
}

/// [`game`] with a river on each of the squares (of Arcanus).
pub fn game_with_rivers(city0: (u32, u32), race: u16, rivers: &[(u32, u32)]) -> Game {
    build_game(city0, race, 7, Plane::Arcanus, rivers)
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
    world.scenario_change_settings(|s| {
        s.rules.max_tile = f4x_core::world::setting::DEFAULT_MAX_TILE
    });
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
                .any(|e| matches!(e, f4x_core::world::Event::Rejected(_))),
            "city {name} not founded: {events:?}"
        );
    }
    Game::from(world)
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

/// A battle of the real game between shipped units: `ours` of player 0 and `theirs` of player 1,
/// each a unit number of the content and the square it stands on. The game sets it up
/// (`BeginBattle`), so the units have their real numbers, rules and owners; only the squares are
/// chosen, on open ground, so that the squares a test names are free, and the units are numbered
/// in the order of the lists. The dice start from `seed`.
pub fn real_battle(
    ours: &[(u16, f4x_battle::map::Pos)],
    theirs: &[(u16, f4x_battle::map::Pos)],
    seed: u64,
) -> f4x_battle::battle::Battle {
    use f4x_battle::map::BattleMap;
    use f4x_battle::Side;
    let mut w = game_seeded(COAST_CITY, 7, seed);
    let mut spawn_all = |owner: PlayerId, x: u32, list: &[(u16, f4x_battle::map::Pos)]| {
        list.iter()
            .map(|&(number, _)| spawn(&mut w, owner, at(x, 4), number, 1)[0])
            .collect::<Vec<_>>()
    };
    let attackers = spawn_all(P0, 4, ours);
    let defenders = spawn_all(P1, 5, theirs);
    w.command(Command::BeginBattle {
        attackers,
        defenders,
    });
    let mut b = w.battle().expect("a battle began").clone();
    b.map = BattleMap::open();
    let mut placed = vec![false; b.units.len()];
    let mut order = Vec::new();
    for (side, list) in [(Side::Attacker, ours), (Side::Defender, theirs)] {
        for &(number, cell) in list {
            let kind = w.content().rules.units.id_numbered(number);
            let at = b
                .units
                .iter()
                .position(|u| u.side == side && u.unit_id == kind && !placed[u.id])
                .expect("the battle has the unit");
            placed[at] = true;
            b.units[at].cell = cell;
            order.push(at);
        }
    }
    let mut units: Vec<_> = order.iter().map(|&i| b.units[i].clone()).collect();
    for (id, u) in units.iter_mut().enumerate() {
        u.id = id;
    }
    b.units = units;
    b
}

/// Teach the wizard a spell it can cast in a battle (`fire_bolt`, 15 mana).
pub fn learn_a_battle_spell(w: &mut World) -> Result<(), Box<dyn std::error::Error>> {
    let id = w
        .content()
        .rules
        .spells
        .id_numbered(5)
        .ok_or("a battle spell is in the content")?;
    let spell = w.content().rules.spells.get(id);
    assert!(spell
        .contexts
        .contains(&f4x_def::rules::CastContext::Battle));
    w.cheat_learn_spell(P0, id);
    w.wizard_mut(P0)
        .ok_or("player 0 is a wizard")?
        .put(Some(f4x_core::wizard::sheet::Skill {
            points: 400,
            left: 10,
        }));
    Ok(())
}

pub fn set_mana(w: &mut World, mana: i32) -> Result<(), Box<dyn std::error::Error>> {
    w.treasury_mut(P0).ok_or("a treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
    Ok(())
}

/// The west island filled with cities of player 0, as close as the rules let them stand.
pub fn fill_west(w: &mut Game) {
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
