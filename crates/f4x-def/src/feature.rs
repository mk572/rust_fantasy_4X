//! Terrain feature definitions: what a feature is, where it may spawn and what it gives a city
//! that works its tile. Loading and validation from files is `load`.
//!
//! Tests: none, because it only declares the feature definitions; `load` loads and validates them.

use crate::half_int::HalfInt;
use serde::{Deserialize, Serialize};

use crate::map::{FeatureId, Plane, TerrainClass};

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
    pub gold: HalfInt,
    pub mana: HalfInt,
    /// Food (wild game); not affected by mining.
    pub food: HalfInt,
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
    pub kind: FeatureKind,
    /// The realm of a magic node.
    #[serde(default)]
    pub realm: Option<crate::rules::RealmId>,
    /// A temple or ruin: the guards of the site draw on life and death.
    #[serde(default)]
    pub holy: bool,
    /// The realm of the book a cleared site gives (a random one where there is none).
    #[serde(default)]
    pub reward: Option<crate::rules::RealmId>,
    /// The mineral Transmute makes of this one (its number).
    #[serde(default)]
    pub transmute: Option<u16>,
    #[serde(default)]
    pub city: CityBonus,
    /// Empty for towers, which are placed by dedicated rules.
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
}

/// The loaded feature table, sorted by number; a [`FeatureId`] is a definition's number.
pub type Features = crate::table::Table<FeatureDef, FeatureId>;

impl crate::table::Numbered for FeatureDef {
    fn number(&self) -> u16 {
        self.num
    }
}

impl crate::table::TableId for FeatureId {
    fn from_number(num: u16) -> Self {
        Self(num)
    }
    fn number(self) -> u16 {
        self.0
    }
}

impl Features {
    pub fn first_of_kind(&self, kind: FeatureKind) -> Option<FeatureId> {
        self.first_where(|d| d.kind == kind)
    }

    /// Features of `kind` that can spawn on `terrain` on `plane`, with summed weights.
    pub fn weighted(
        &self,
        kind: FeatureKind,
        terrain: TerrainClass,
        plane: Plane,
    ) -> Vec<(FeatureId, u32)> {
        self.defs()
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
