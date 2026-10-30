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
    /// The feature's number, the id the game passes around (ours: counted from 1 in the
    /// alphabetical order of the ids).
    pub num: u16,
    /// From `text/<language>/name/` by `num`, not from the feature file.
    #[serde(default)]
    pub name: String,
    pub kind: FeatureKind,
    #[serde(default)]
    pub city: CityBonus,
    /// Empty for towers, which are placed by dedicated rules.
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
}

/// The loaded feature table, sorted by number; a [`FeatureId`] is a definition's number.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Features {
    defs: Vec<FeatureDef>,
}

impl Features {
    pub fn new(mut defs: Vec<FeatureDef>) -> Self {
        defs.sort_by_key(|d| d.num);
        Self { defs }
    }

    pub fn defs(&self) -> &[FeatureDef] {
        &self.defs
    }

    /// The definitions, to put the names of a language on them. Their numbers stay as they are.
    pub fn defs_mut(&mut self) -> &mut [FeatureDef] {
        &mut self.defs
    }

    /// The feature with the number `num` (what a save and the name tables call it).
    pub fn id_numbered(&self, num: u16) -> Option<FeatureId> {
        self.get(FeatureId(num)).map(|_| FeatureId(num))
    }

    pub fn get(&self, id: FeatureId) -> Option<&FeatureDef> {
        self.defs
            .binary_search_by_key(&id.0, |d| d.num)
            .ok()
            .map(|i| &self.defs[i])
    }

    pub fn id_of(&self, name: &str) -> Option<FeatureId> {
        self.defs
            .iter()
            .find(|d| d.id == name)
            .map(|d| FeatureId(d.num))
    }

    pub fn first_of_kind(&self, kind: FeatureKind) -> Option<FeatureId> {
        self.defs
            .iter()
            .find(|d| d.kind == kind)
            .map(|d| FeatureId(d.num))
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
            .filter(|d| d.kind == kind)
            .filter_map(|d| {
                let w: u32 = d
                    .spawn
                    .iter()
                    .filter(|r| r.terrain.contains(&terrain) && r.planes.contains(&plane))
                    .map(|r| r.weight)
                    .sum();
                (w > 0).then_some((FeatureId(d.num), w))
            })
            .collect()
    }
}
