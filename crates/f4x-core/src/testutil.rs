//! Hand-built content and worlds for unit tests (the shipped files are tested in f4x-data).

use crate::city::{PlayerId, Position};
use crate::content::*;
use crate::map::{Grid, Map, Plane};
use crate::rules::*;
use crate::{Command, Event, Game};

pub const RACES: &str = r#"[
    (id: "sunfolk", name: "Sunfolk", effects: [(target: City, effect: Modify(stat: CityGrowthPercent, op: Add, value: 10))]),
    (id: "ironclans", name: "Ironclans", effects: [(target: City, effect: Modify(stat: CityProduction, op: Add, value: 1))]),
]"#;

pub const UNITS: &str = r#"[
    (id: "spearman", name: "Spearman", kind: Normal, race: Some("sunfolk"), cost: 30, upkeep: (gold: 1),
     stats: {Melee: 3, Defense: 3, Resistance: 4, MaxHp: 2, Figures: 3, Moves: 2},
     movement: Walking, abilities: ["armoured"], level_path: Some("lp")),
    (id: "pikeman", name: "Pikeman", kind: Normal, race: Some("sunfolk"), cost: 40, upkeep: (gold: 1),
     stats: {Melee: 3, Defense: 4, Resistance: 4, MaxHp: 1, Figures: 6, Moves: 1},
     movement: Walking, requires: ["barracks"]),
    (id: "axeman", name: "Axeman", kind: Normal, race: Some("ironclans"), cost: 40,
     stats: {Melee: 4, Defense: 3, Resistance: 5, MaxHp: 1, Figures: 4, Moves: 1}, movement: Walking),
    (id: "bat", name: "Bat", kind: Fantastic, cost: 10, upkeep: (mana: 1),
     stats: {Melee: 1, Defense: 1, Resistance: 3, MaxHp: 1, Figures: 1, Moves: 4}, movement: Flying),
    (id: "raft", name: "Raft", kind: Fantastic, cost: 10,
     stats: {Melee: 0, Defense: 1, Resistance: 3, MaxHp: 1, Figures: 1, Moves: 2}, movement: Sailing),
    (id: "hero", name: "Hero", kind: Hero, cost: 0,
     stats: {Melee: 4, Defense: 3, Resistance: 6, MaxHp: 4, Figures: 1, Moves: 1}, movement: Walking,
     level_path: Some("lp"), hero: Some((slots: [Weapon, Armor], hire_cost: 100))),
]"#;

pub const ABILITIES: &str = r#"[
    (id: "armoured", name: "Armoured", effects: [(effect: Modify(stat: Defense, op: Add, value: 1))]),
]"#;

pub const BUILDINGS: &str = r#"[
    (id: "barracks", name: "Barracks", cost: 20, upkeep_gold: 1),
    (id: "armoury", name: "Armoury", cost: 30, upkeep_gold: 2, requires: ["barracks"], effects: [
        (target: City, effect: Modify(stat: UnitCost, op: Percent, value: -50)),
        (target: Owner, effect: Modify(stat: ManaIncome, op: Add, value: 5)),
    ]),
    (id: "forge", name: "Forge", cost: 30, races: ["ironclans"], effects: [
        (target: City, effect: Modify(stat: CityProduction, op: Add, value: 2)),
    ]),
]"#;

pub const ITEMS: &str = r#"[
    (id: "sword", name: "Sword", slot: Weapon, cost: 1, effects: [(effect: Modify(stat: Melee, op: Add, value: 2))]),
    (id: "axe", name: "Axe", slot: Weapon, cost: 1, effects: [(effect: Modify(stat: Melee, op: Add, value: 1))]),
    (id: "shield", name: "Shield", slot: Armor, cost: 1, effects: [(effect: Modify(stat: Defense, op: Add, value: 2))]),
    (id: "ring", name: "Ring", slot: Misc, cost: 1, effects: [(effect: Modify(stat: Resistance, op: Add, value: 1))]),
]"#;

pub const LEVEL_PATHS: &str = r#"[
    (id: "lp", levels: [
        (name: "Recruit", xp: 0, effects: []),
        (name: "Veteran", xp: 10, effects: [(effect: Modify(stat: Melee, op: Add, value: 1))]),
        (name: "Elite", xp: 30, effects: [(effect: Modify(stat: Melee, op: Add, value: 1))]),
    ]),
]"#;

fn parse<T: serde::de::DeserializeOwned>(s: &str) -> T {
    ron::from_str(s).expect("test content parses")
}

pub fn rules() -> Rules {
    Rules {
        races: Registry::new(parse(RACES)),
        units: Registry::new(parse(UNITS)),
        abilities: Registry::new(parse(ABILITIES)),
        buildings: Registry::new(parse(BUILDINGS)),
        items: Registry::new(parse(ITEMS)),
        level_paths: Registry::new(parse(LEVEL_PATHS)),
        ..Rules::default()
    }
}

pub fn content() -> Content {
    let terrain = |id: &str, class, walkable, can_found_city, move_cost| TerrainDef {
        id: id.into(),
        name: id.into(),
        class,
        walkable,
        can_found_city,
        move_cost,
        food: 1,
        planes: Plane::ALL.to_vec(),
    };
    let terrains = Terrains::new(vec![
        terrain("ocean", TerrainClass::Ocean, false, false, 0),
        terrain("grassland", TerrainClass::Grassland, true, true, 1),
        terrain("forest", TerrainClass::Forest, true, true, 2),
        terrain("hills", TerrainClass::Hills, true, true, 2),
        terrain("mountains", TerrainClass::Mountains, true, false, 3),
        terrain("swamp", TerrainClass::Swamp, true, true, 2),
        terrain("desert", TerrainClass::Desert, true, true, 1),
        terrain("tundra", TerrainClass::Tundra, true, true, 2),
    ]);
    let land = vec![
        TerrainClass::Grassland,
        TerrainClass::Forest,
        TerrainClass::Hills,
        TerrainClass::Mountains,
        TerrainClass::Swamp,
        TerrainClass::Desert,
        TerrainClass::Tundra,
    ];
    let feature = |id: &str, kind, terrain: Vec<TerrainClass>, planes: Vec<Plane>| FeatureDef {
        id: id.into(),
        name: id.into(),
        kind,
        spawn: if terrain.is_empty() {
            vec![]
        } else {
            vec![SpawnRule {
                terrain,
                planes,
                weight: 1,
            }]
        },
    };
    let features = Features::new(vec![
        feature(
            "iron",
            FeatureKind::Mineral,
            vec![TerrainClass::Hills, TerrainClass::Mountains],
            Plane::ALL.to_vec(),
        ),
        feature(
            "adamantium",
            FeatureKind::Mineral,
            vec![TerrainClass::Hills, TerrainClass::Mountains],
            vec![Plane::Myrror],
        ),
        feature(
            "lair",
            FeatureKind::Encounter,
            land.clone(),
            Plane::ALL.to_vec(),
        ),
        feature("tower", FeatureKind::Tower, vec![], vec![]),
        feature("node", FeatureKind::Node, land, Plane::ALL.to_vec()),
    ]);
    Content {
        terrains,
        features,
        rules: rules(),
        ..Default::default()
    }
}

/// A 20x12 grassland world (both planes) with ocean along x=0, a mountain at (5,5),
/// forest at (6,5), a lair at (8,5) and an iron mineral at (12,5).
pub fn game() -> Game {
    let c = content();
    let id = |n: &str| c.terrains.id_of(n).unwrap();
    let build = || {
        let mut g = Grid::new(20, 12, id("grassland"));
        for y in 0..12 {
            g.set(0, y, id("ocean"));
        }
        g.set(5, 5, id("mountains"));
        g.set(6, 5, id("forest"));
        g.set_feature(8, 5, c.features.id_of("lair"));
        g.set_feature(12, 5, c.features.id_of("iron"));
        g
    };
    let mut game = Game::new(1);
    game.map = Some(Map::new(build(), build()));
    game.content = c;
    game
}

pub fn at(plane: Plane, x: u32, y: u32) -> Position {
    Position { plane, x, y }
}

pub fn unit_id(g: &Game, name: &str) -> UnitId {
    g.content.rules.units.id_of(name).unwrap()
}

pub fn building_id(g: &Game, name: &str) -> BuildingId {
    g.content.rules.buildings.id_of(name).unwrap()
}

pub fn race_id(g: &Game, name: &str) -> RaceId {
    g.content.rules.races.id_of(name).unwrap()
}

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

/// Found a city for `owner` of `race` and return its entity.
pub fn found(g: &mut Game, owner: PlayerId, race: &str, pos: Position) -> hecs::Entity {
    let race = race_id(g, race);
    let events = g.apply(Command::FoundCity {
        owner,
        race,
        pos,
        name: "Town".into(),
    });
    match events[..] {
        [Event::CityFounded { city, .. }] => city,
        ref other => panic!("founding failed: {other:?}"),
    }
}
