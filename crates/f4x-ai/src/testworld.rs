//! A small hand-made world with the shipped content, for the unit tests of the AI modules: two
//! islands in an ocean, one wizard on each, so that a unit cannot walk to the other side.
//!
//! Tests: none, because this is test support; the modules that use it test it.

use f4x_core::feature::entity::Placement;
use f4x_core::feature::lair::Lair;
use f4x_core::map::{Grid, Map, Plane, Position};
use f4x_core::player::PlayerId;
use f4x_def::content::Content;

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

/// The shipped content, parsed once for all the tests of the process.
pub use f4x_def::testdir::shipped;

/// A copy of the shipped content, for a `Game`, which owns its content.
pub fn content() -> Content {
    shipped().clone()
}

/// A new game as the game itself sets it up (wizards, cities, heroes), on the shipped content.
pub fn fresh_game() -> Result<Game, Box<dyn std::error::Error>> {
    Ok(f4x_newgame::setup::new_game(
        shipped(),
        &f4x_newgame::setup::NewGame {
            seed: 7,
            ..Default::default()
        },
    )?)
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
    build_game(city0, race, 7, Plane::Myrror)
}

/// [`game`] whose dice start from `seed`.
pub fn game_seeded(city0: (u32, u32), race: u16, seed: u64) -> Game {
    build_game(city0, race, seed, Plane::Arcanus)
}

fn build_game(city0: (u32, u32), race: u16, seed: u64, rival_plane: Plane) -> Game {
    let content = content();
    let ocean = content.terrains.id_numbered(8).expect("ocean");
    let grass = content.terrains.id_numbered(4).expect("grassland");
    let build = || {
        let mut grid = Grid::new(WIDTH, HEIGHT, ocean);
        for y in ROWS {
            for x in WEST.chain(EAST) {
                grid.set(x, y, grass);
            }
        }
        grid
    };
    let race_id = content
        .rules
        .races
        .id_numbered(race)
        .expect("the race exists");
    let mut game = Game::new(seed);
    game.set_content(content);
    game.set_map(Map::new(build(), build()));
    game.add_player(P0, "West");
    game.add_player(P1, "East");
    for (owner, (x, y), name) in [(P0, city0, "Westport"), (P1, ENEMY_CITY, "Eastport")] {
        let events = game.apply(Command::FoundCity {
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
    unit: u16,
    count: usize,
) -> Vec<hecs::Entity> {
    let id = game
        .content()
        .rules
        .units
        .id_numbered(unit)
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
    guards: Vec<(f4x_def::rules::UnitId, u32)>,
    fought_by: Vec<PlayerId>,
) -> Result<hecs::Entity, &'static str> {
    let feature = game
        .content()
        .features
        .id_numbered(13)
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
            hurt: Vec::new(),
        }),
    }))
}

/// A tower at `pos`, guarded by one zombie while `intact`.
pub fn spawn_tower(
    game: &mut Game,
    pos: Position,
    intact: bool,
) -> Result<hecs::Entity, &'static str> {
    let feature = game
        .content()
        .features
        .id_numbered(21)
        .ok_or("tower is shipped")?;
    Ok(game.spawn_feature(Placement {
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
pub fn build(game: &mut Game, pos: Position, buildings: &[u16]) {
    let city = game.economy().city_at(pos).expect("a city there");
    for id in buildings {
        let b = game
            .content()
            .rules
            .buildings
            .id_numbered(*id)
            .unwrap_or_else(|| panic!("no building {id}"));
        game.scenario_set_building(city, b, true);
    }
}

/// A battle of the real game between shipped units: `ours` of player 0 and `theirs` of player 1,
/// each a unit number of the content and the square it stands on. The game sets it up
/// (`BeginBattle`), so the units have their real numbers, rules and owners; only the squares are
/// chosen, on open ground, so that the squares a test names are free, and the units are numbered
/// in the order of the lists. The dice start from `seed`.
pub fn real_battle(
    ours: &[(u16, f4x_core::battle::map::Pos)],
    theirs: &[(u16, f4x_core::battle::map::Pos)],
    seed: u64,
) -> f4x_core::battle::Battle {
    use f4x_core::battle::{map::BattleMap, Side};
    let mut g = game_seeded(COAST_CITY, 7, seed);
    let mut spawn_all = |owner: PlayerId, x: u32, list: &[(u16, f4x_core::battle::map::Pos)]| {
        list.iter()
            .map(|&(number, _)| spawn(&mut g, owner, at(x, 4), number, 1)[0])
            .collect::<Vec<_>>()
    };
    let attackers = spawn_all(P0, 4, ours);
    let defenders = spawn_all(P1, 5, theirs);
    g.apply(Command::BeginBattle {
        attackers,
        defenders,
    });
    let mut b = g.battle().expect("a battle began").clone();
    b.map = BattleMap::open();
    let mut placed = vec![false; b.units.len()];
    let mut order = Vec::new();
    for (side, list) in [(Side::Attacker, ours), (Side::Defender, theirs)] {
        for &(number, cell) in list {
            let kind = g.content().rules.units.id_numbered(number);
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
pub fn learn_a_battle_spell(g: &mut Game) -> Result<(), Box<dyn std::error::Error>> {
    let id = g
        .content()
        .rules
        .spells
        .id_numbered(5)
        .ok_or("a battle spell is in the content")?;
    let spell = g.content().rules.spells.get(id);
    assert!(spell
        .contexts
        .contains(&f4x_def::rules::CastContext::Battle));
    g.cheat_learn_spell(P0, id);
    g.wizard_mut(P0)
        .ok_or("player 0 is a wizard")?
        .put(Some(f4x_core::wizard::sheet::Skill {
            points: 400,
            left: 10,
        }));
    Ok(())
}

pub fn set_mana(g: &mut Game, mana: i32) -> Result<(), Box<dyn std::error::Error>> {
    g.treasury_mut(P0).ok_or("a treasury")?.mana = f4x_def::half_int::HalfInt::from_int(mana);
    Ok(())
}

/// The west island filled with cities of player 0, as close as the rules let them stand.
pub fn fill_west(g: &mut Game) {
    let race = g.content().rules.races.id_numbered(8);
    for x in 3..=10 {
        for y in 3..=10 {
            g.apply(Command::FoundCity {
                owner: P0,
                pos: at(x, y),
                name: format!("C{x}{y}"),
                race,
                population: 5,
            });
        }
    }
}
