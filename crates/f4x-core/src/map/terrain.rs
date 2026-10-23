//! Terrain definitions and classes: the terrain table the map is made of. Loading and validation
//! from files lives in `f4x-data`.

use serde::{Deserialize, Serialize};

use crate::map::{Plane, TerrainId};

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
