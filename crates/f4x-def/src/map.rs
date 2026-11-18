//! The map's vocabulary and the terrain table: the planes, the numbers of terrains and features,
//! the terrain classes and the terrain definitions.

use crate::half_int::HalfInt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumIter)]
pub enum Plane {
    Arcanus,
    Myrror,
}

impl Plane {
    /// Both planes, Arcanus first.
    pub fn all() -> impl Iterator<Item = Plane> {
        <Plane as strum::IntoEnumIterator>::iter()
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The number (`num`) of a terrain definition in the loaded [`crate::map::terrain::Terrains`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerrainId(pub u16);

/// The number (`num`) of a feature definition in the loaded [`crate::feature::def::Features`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureId(pub u16);

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
    /// The base terrain class under a node of this realm (sorcery, nature or chaos).
    pub fn of_node_realm(realm: crate::rules::RealmId) -> Option<Self> {
        match realm.num() {
            1 /* sorcery */ => Some(Self::SorceryNode),
            0 /* nature */ => Some(Self::NatureNode),
            2 /* chaos */ => Some(Self::ChaosNode),
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
pub const REQUIRED_CLASSES: &[TerrainClass] = &[
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
    /// The terrain's number, the id the game passes around (ours: counted from 1 in the
    /// alphabetical order of the ids; the original has no table of terrain types by name).
    pub num: u16,
    pub class: TerrainClass,
    pub walkable: bool,
    pub can_found_city: bool,
    pub move_cost: u8,
    /// Food a city gets from the tile (1.5 = one and a half food).
    pub food: HalfInt,
    /// Percent added to a city's production per worked tile of this terrain.
    #[serde(default)]
    pub production_percent: i32,
    pub planes: Vec<Plane>,
}

/// The loaded terrain table, sorted by number; a [`TerrainId`] is a definition's number.
pub type Terrains = crate::table::Table<TerrainDef, TerrainId>;

impl crate::table::Numbered for TerrainDef {
    fn number(&self) -> u16 {
        self.num
    }
}

impl crate::table::TableId for TerrainId {
    fn from_number(num: u16) -> Self {
        Self(num)
    }
    fn number(self) -> u16 {
        self.0
    }
}

impl Terrains {
    /// First definition of `class` that is available on `plane`.
    pub fn first_of_class(&self, class: TerrainClass, plane: Plane) -> Option<TerrainId> {
        self.first_where(|d| d.class == class && d.planes.contains(&plane))
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
        use crate::rules::RealmId;
        for (realm, class) in [
            (1 /* sorcery */, TerrainClass::SorceryNode),
            (0 /* nature */, TerrainClass::NatureNode),
            (2 /* chaos */, TerrainClass::ChaosNode),
        ] {
            assert_eq!(
                TerrainClass::of_node_realm(RealmId::new(realm)),
                Some(class)
            );
            assert!(class.is_node());
        }
        assert_eq!(
            TerrainClass::of_node_realm(RealmId::new(3 /* life */)),
            None
        );
        assert!(!TerrainClass::Hills.is_node());
    }
}
