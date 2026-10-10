//! Hand-built content for unit tests (the shipped files are tested in f4x-data).

use crate::content::*;
use crate::map::Plane;

pub fn content() -> Content {
    let terrain = |id: &str, class, walkable, can_found_city| TerrainDef {
        id: id.into(),
        name: id.into(),
        class,
        walkable,
        can_found_city,
        move_cost: 1,
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
