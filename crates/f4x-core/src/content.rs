//! Content schema types shared by the simulation and the data loaders.
//! Loading and validation from files lives in `f4x-data`.

use serde::{Deserialize, Serialize};

use crate::map::{FeatureId, Plane, TerrainId};

/// Coarse terrain category. Map generation works on classes; concrete
/// definitions (and modded variants) are picked from the data files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerrainClass {
    Ocean,
    Grassland,
    Forest,
    Hills,
    Mountains,
    Swamp,
    Desert,
    Tundra,
}

/// Every class the map generator may place; content must define each one per plane.
pub const REQUIRED_CLASSES: [TerrainClass; 8] = [
    TerrainClass::Ocean,
    TerrainClass::Grassland,
    TerrainClass::Forest,
    TerrainClass::Hills,
    TerrainClass::Mountains,
    TerrainClass::Swamp,
    TerrainClass::Desert,
    TerrainClass::Tundra,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainDef {
    pub id: String,
    pub name: String,
    pub class: TerrainClass,
    pub walkable: bool,
    pub can_found_city: bool,
    pub move_cost: u8,
    pub food: u8,
    pub planes: Vec<Plane>,
}

/// The loaded terrain table; a [`TerrainId`] is an index into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Terrains {
    defs: Vec<TerrainDef>,
}

impl Terrains {
    pub fn new(defs: Vec<TerrainDef>) -> Self {
        Self { defs }
    }

    pub fn defs(&self) -> &[TerrainDef] {
        &self.defs
    }

    pub fn get(&self, id: TerrainId) -> Option<&TerrainDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn id_of(&self, name: &str) -> Option<TerrainId> {
        self.defs
            .iter()
            .position(|d| d.id == name)
            .map(|i| TerrainId(i as u16))
    }

    /// First definition of `class` that is available on `plane`.
    pub fn first_of_class(&self, class: TerrainClass, plane: Plane) -> Option<TerrainId> {
        self.defs
            .iter()
            .position(|d| d.class == class && d.planes.contains(&plane))
            .map(|i| TerrainId(i as u16))
    }
}

/// What a feature is; drives placement, and which features block city founding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureKind {
    /// Resource deposit or special on rough/forest terrain; a city may sit on it.
    Mineral,
    /// Lair, temple, ruins and similar guarded sites.
    Encounter,
    /// Plane-spanning tower; same coordinates on every plane.
    Tower,
    /// Magic node.
    Node,
}

impl FeatureKind {
    /// Cities cannot be founded on these.
    pub fn blocks_city(self) -> bool {
        !matches!(self, FeatureKind::Mineral)
    }
}

/// "On these terrain classes, on these planes, spawn with this relative weight."
/// Weights are relative among all rules matching one (terrain class, plane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRule {
    pub terrain: Vec<TerrainClass>,
    pub planes: Vec<Plane>,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDef {
    pub id: String,
    pub name: String,
    pub kind: FeatureKind,
    /// Empty for towers, which are placed by dedicated rules.
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
}

/// The loaded feature table; a [`FeatureId`] is an index into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Features {
    defs: Vec<FeatureDef>,
}

impl Features {
    pub fn new(defs: Vec<FeatureDef>) -> Self {
        Self { defs }
    }

    pub fn defs(&self) -> &[FeatureDef] {
        &self.defs
    }

    pub fn get(&self, id: FeatureId) -> Option<&FeatureDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn id_of(&self, name: &str) -> Option<FeatureId> {
        self.defs
            .iter()
            .position(|d| d.id == name)
            .map(|i| FeatureId(i as u16))
    }

    pub fn first_of_kind(&self, kind: FeatureKind) -> Option<FeatureId> {
        self.defs
            .iter()
            .position(|d| d.kind == kind)
            .map(|i| FeatureId(i as u16))
    }

    /// Features of `kind` that can spawn on `terrain` on `plane`, with summed weights.
    pub fn weighted(
        &self,
        kind: FeatureKind,
        terrain: TerrainClass,
        plane: Plane,
    ) -> Vec<(FeatureId, u32)> {
        self.defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == kind)
            .filter_map(|(i, d)| {
                let w: u32 = d
                    .spawn
                    .iter()
                    .filter(|r| r.terrain.contains(&terrain) && r.planes.contains(&plane))
                    .map(|r| r.weight)
                    .sum();
                (w > 0).then_some((FeatureId(i as u16), w))
            })
            .collect()
    }
}

/// All loaded game content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Content {
    pub terrains: Terrains,
    pub features: Features,
    pub rules: crate::rules::Rules,
}
