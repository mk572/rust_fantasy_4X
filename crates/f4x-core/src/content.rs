//! Content schema types shared by the simulation and the data loaders.
//! Loading and validation from files lives in `f4x-data`.

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
