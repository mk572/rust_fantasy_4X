//! Hand-built content for unit tests (the shipped files are tested in f4x-data).

use std::collections::BTreeMap;

use crate::content::*;
use crate::map::Plane;
use crate::rules::{
    AbilityDef, LevelDef, LevelPathDef, MovementType, Registry, Rules, Stat, UnitDef, UnitKind,
};

pub fn content() -> Content {
    let terrain = |id: &str, class: TerrainClass, walkable, can_found_city| TerrainDef {
        id: id.into(),
        name: id.into(),
        class,
        walkable,
        can_found_city,
        move_cost: match class {
            TerrainClass::Forest | TerrainClass::Hills | TerrainClass::Swamp => 2,
            TerrainClass::Mountains => 3,
            _ => 1,
        },
        food: 1,
        planes: Plane::ALL.to_vec(),
    };
    let terrains = Terrains::new(vec![
        terrain("ocean", TerrainClass::Ocean, false, false),
        terrain("grassland", TerrainClass::Grassland, true, true),
        terrain("forest", TerrainClass::Forest, true, true),
        terrain("hills", TerrainClass::Hills, true, true),
        terrain("mountains", TerrainClass::Mountains, true, false),
        terrain("swamp", TerrainClass::Swamp, true, true),
        terrain("desert", TerrainClass::Desert, true, true),
        terrain("tundra", TerrainClass::Tundra, true, true),
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
        ..Default::default()
    }
}

/// A unit definition with the stats movement and combat tests need.
pub fn unit(id: &str, movement: MovementType, moves: i32, abilities: &[&str]) -> UnitDef {
    let mut stats = BTreeMap::new();
    for (stat, v) in [
        (Stat::Melee, 3),
        (Stat::Defense, 2),
        (Stat::Resistance, 4),
        (Stat::MaxHp, 2),
        (Stat::Figures, 3),
        (Stat::Moves, moves),
        (Stat::Sight, 1),
    ] {
        stats.insert(stat, v);
    }
    UnitDef {
        id: id.into(),
        name: id.into(),
        kind: UnitKind::Fantastic,
        race: None,
        realm: None,
        cost: 10,
        upkeep: Default::default(),
        stats,
        movement,
        abilities: abilities.iter().map(|a| a.to_string()).collect(),
        requires: vec![],
        level_path: Some("xp".into()),
        hero: None,
    }
}

/// Test content plus a few units: walker (2 moves), scout (flying, 3), boat (sailing, 4),
/// ranger (forester + mountaineer), pathfinder, spirit (non-corporeal), mage (wind walking),
/// traveller (planar travel).
pub fn content_with_units() -> Content {
    let mut c = content();
    let units = vec![
        unit("walker", MovementType::Walking, 2, &[]),
        unit("slow", MovementType::Walking, 1, &[]),
        unit("scout", MovementType::Flying, 3, &[]),
        unit("boat", MovementType::Sailing, 4, &[]),
        unit(
            "ranger",
            MovementType::Walking,
            2,
            &["forester", "mountaineer"],
        ),
        unit("pathfinder", MovementType::Walking, 2, &["pathfinding"]),
        unit("spirit", MovementType::Walking, 2, &["non_corporeal"]),
        unit("mage", MovementType::Walking, 2, &["wind_walking"]),
        unit("traveller", MovementType::Walking, 2, &["planar_travel"]),
        unit("hardy", MovementType::Walking, 2, &["tough"]),
    ];
    let ability = |id: &str, effects| AbilityDef {
        id: id.into(),
        name: id.into(),
        description: String::new(),
        effects,
    };
    let abilities = [
        "forester",
        "mountaineer",
        "pathfinding",
        "non_corporeal",
        "wind_walking",
        "planar_travel",
        "flying",
    ]
    .into_iter()
    .map(|a| ability(a, vec![]))
    .chain([ability(
        "tough",
        vec![crate::rules::effects::EffectRule {
            when: crate::rules::effects::Trigger::Passive,
            condition: crate::rules::effects::Condition::Always,
            target: crate::rules::effects::TargetSel::This,
            effect: crate::rules::effects::Effect::Modify {
                stat: Stat::Defense,
                op: crate::rules::ModOp::Add,
                value: 2,
            },
        }],
    )])
    .collect();
    let level_path = LevelPathDef {
        id: "xp".into(),
        levels: vec![
            LevelDef {
                name: "Recruit".into(),
                xp: 0,
                effects: vec![],
            },
            LevelDef {
                name: "Veteran".into(),
                xp: 20,
                effects: vec![crate::rules::effects::EffectRule {
                    when: crate::rules::effects::Trigger::Passive,
                    condition: crate::rules::effects::Condition::Always,
                    target: crate::rules::effects::TargetSel::This,
                    effect: crate::rules::effects::Effect::Modify {
                        stat: Stat::Melee,
                        op: crate::rules::ModOp::Add,
                        value: 1,
                    },
                }],
            },
        ],
    };
    c.rules = Rules {
        units: Registry::new(units),
        abilities: Registry::new(abilities),
        level_paths: Registry::new(vec![level_path]),
        ..Default::default()
    };
    c
}
