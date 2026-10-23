//! Terrain feature definitions: what a feature is, where it may spawn and what it gives a city
//! that works its tile. Loading and validation from files lives in `f4x-data`.

use serde::{Deserialize, Serialize};

use crate::map::terrain::TerrainClass;
use crate::map::{FeatureId, Plane};

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

/// "On these terrain classes, on these planes, spawn with this relative weight."
/// Weights are relative among all rules matching one (terrain class, plane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRule {
    pub terrain: Vec<TerrainClass>,
    pub planes: Vec<Plane>,
    pub weight: u32,
}

/// What a feature adds to a city that works its tile. Gold, mana and cost reduction are
/// doubled for races that mine well and increased by half with a miners' guild; every
/// value is halved when two cities share the tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CityBonus {
    pub gold: i32,
    pub mana: i32,
    /// Whole food (wild game); not affected by mining.
    pub food: i32,
    /// Percent off the cost of military units built in the city (stacks up to a limit).
    pub unit_cost_percent: i32,
    /// Added to the chance an outpost grows each turn.
    pub outpost_growth: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDef {
    pub id: String,
    pub name: String,
    pub kind: FeatureKind,
    #[serde(default)]
    pub city: CityBonus,
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
