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
    /// The base terrain of a sorcery node's square (`tt_SorceryNode` in the original).
    SorceryNode,
    /// The base terrain of a nature node's square (`tt_NatureNode`).
    NatureNode,
    /// The base terrain of a chaos node's square (`tt_ChaosNode`). Not the volcano a spell
    /// raises: that is a mountain with an owner.
    ChaosNode,
}

impl TerrainClass {
    /// The base terrain class under a node of this realm (`"sorcery"`, `"nature"`, `"chaos"`).
    pub fn of_node_realm(realm: &str) -> Option<Self> {
        match realm {
            "sorcery" => Some(Self::SorceryNode),
            "nature" => Some(Self::NatureNode),
            "chaos" => Some(Self::ChaosNode),
            _ => None,
        }
    }

    /// Forest for the rules that ask: a nature node counts as forest (`Square_Is_Forest`).
    pub fn is_forest(self) -> bool {
        matches!(self, Self::Forest | Self::NatureNode)
    }

    /// Mountains for the rules that ask: a chaos node counts as mountains (`Square_Is_Mountain`).
    pub fn is_mountain(self) -> bool {
        matches!(self, Self::Mountains | Self::ChaosNode)
    }

    /// Whether this is the base terrain of a magic node's square.
    pub fn is_node(self) -> bool {
        matches!(self, Self::SorceryNode | Self::NatureNode | Self::ChaosNode)
    }
}

/// Every class the map generator may place; content must define each one per plane.
pub const REQUIRED_CLASSES: [TerrainClass; 11] = [
    TerrainClass::Ocean,
    TerrainClass::Grassland,
    TerrainClass::Forest,
    TerrainClass::Hills,
    TerrainClass::Mountains,
    TerrainClass::Swamp,
    TerrainClass::Desert,
    TerrainClass::Tundra,
    TerrainClass::SorceryNode,
    TerrainClass::NatureNode,
    TerrainClass::ChaosNode,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainDef {
    pub id: String,
    pub name: String,
    pub class: TerrainClass,
    pub walkable: bool,
    pub can_found_city: bool,
    pub move_cost: u8,
    /// Food a city gets from the tile, in half-food units (3 = one and a half food).
    pub food: u8,
    /// Percent added to a city's production per worked tile of this terrain.
    #[serde(default)]
    pub production_percent: i32,
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

/// All loaded game content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Content {
    pub terrains: Terrains,
    pub features: Features,
    pub rules: crate::rules::Rules,
    /// Difficulty levels and the defaults of the settings screens (`settings.ron`).
    pub settings: crate::game::setting::SettingsDef,
    /// Every player-facing message, in the language of `settings.language`.
    pub texts: crate::text::Texts,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nature_node_is_a_forest_and_a_chaos_node_a_mountain() {
        assert!(TerrainClass::NatureNode.is_forest() && TerrainClass::Forest.is_forest());
        assert!(TerrainClass::ChaosNode.is_mountain() && TerrainClass::Mountains.is_mountain());
        assert!(!TerrainClass::SorceryNode.is_forest() && !TerrainClass::SorceryNode.is_mountain());
    }

    #[test]
    fn node_realms_name_their_base_terrain() {
        for (realm, class) in [
            ("sorcery", TerrainClass::SorceryNode),
            ("nature", TerrainClass::NatureNode),
            ("chaos", TerrainClass::ChaosNode),
        ] {
            assert_eq!(TerrainClass::of_node_realm(realm), Some(class));
            assert!(class.is_node());
        }
        assert_eq!(TerrainClass::of_node_realm("life"), None);
        assert!(!TerrainClass::Hills.is_node());
    }
}
