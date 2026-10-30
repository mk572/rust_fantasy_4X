//! Two-plane tile map. The world wraps horizontally and is bounded vertically.

pub mod corruption;
pub mod fog;
pub mod mapgen;
pub mod road;
pub mod survey;
pub mod terrain;
pub mod vision;

use crate::feature::entity::FeatureIndex;
use serde::{Deserialize, Serialize};

/// Where an entity stands. Components on map objects such as cities and units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub plane: Plane,
    pub x: u32,
    pub y: u32,
}

impl Position {
    /// The same square on the other plane.
    pub fn on_other_plane(self) -> Self {
        Self {
            plane: match self.plane {
                Plane::Arcanus => Plane::Myrror,
                Plane::Myrror => Plane::Arcanus,
            },
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Plane {
    Arcanus,
    Myrror,
}

impl Plane {
    pub const ALL: [Plane; 2] = [Plane::Arcanus, Plane::Myrror];

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

/// One plane: a base terrain layer plus an optional mineral per tile, with the layers of roads,
/// rivers, lakes and corruption. Cities, towers, nodes, encounter sites and volcanoes are
/// entities, not tiles (`feature::entity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    width: u32,
    height: u32,
    tiles: Vec<TerrainId>,
    minerals: Vec<Option<FeatureId>>,
    /// Roads make movement between two road tiles cost half a move.
    #[serde(default)]
    roads: Vec<bool>,
    /// A river runs through the tile: a gold bonus for a city there, nothing else.
    #[serde(default)]
    rivers: Vec<bool>,
    /// Enchanted roads cost walking units no movement at all.
    #[serde(default)]
    enchanted_roads: Vec<bool>,
    /// Corrupted land: no food, production or minerals for a city working it.
    #[serde(default)]
    corrupted: Vec<bool>,
    /// Lake squares: ocean-class water that is not part of any sea.
    #[serde(default)]
    lakes: Vec<bool>,
}

impl Grid {
    pub fn new(width: u32, height: u32, fill: TerrainId) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        let len = (width * height) as usize;
        Self {
            width,
            height,
            tiles: vec![fill; len],
            minerals: vec![None; len],
            roads: vec![false; len],
            rivers: vec![false; len],
            enchanted_roads: vec![false; len],
            lakes: vec![false; len],
            corrupted: vec![false; len],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Wrap an x coordinate into `0..width`.
    pub fn wrap_x(&self, x: i32) -> u32 {
        x.rem_euclid(self.width as i32) as u32
    }

    /// Terrain at `(x, y)`; x wraps, y outside the map yields `None`.
    pub fn get(&self, x: i32, y: i32) -> Option<TerrainId> {
        if y < 0 || y >= self.height as i32 {
            return None;
        }
        Some(self.tiles[(y as u32 * self.width + self.wrap_x(x)) as usize])
    }

    pub fn set(&mut self, x: u32, y: u32, terrain: TerrainId) {
        let i = self.index(x, y);
        self.tiles[i] = terrain;
    }

    /// Mineral on the tile at `(x, y)` (x wraps, y outside the map yields `None`).
    pub fn mineral(&self, x: i32, y: i32) -> Option<FeatureId> {
        if y < 0 || y >= self.height as i32 {
            return None;
        }
        self.minerals[(y as u32 * self.width + self.wrap_x(x)) as usize]
    }

    pub fn set_mineral(&mut self, x: u32, y: u32, mineral: Option<FeatureId>) {
        let i = self.index(x, y);
        self.minerals[i] = mineral;
    }

    /// Rebuild a grid from saved parts. Returns `None` if the lengths do not match.
    pub fn from_parts(
        width: u32,
        height: u32,
        tiles: Vec<TerrainId>,
        minerals: Vec<Option<FeatureId>>,
        roads: Vec<bool>,
    ) -> Option<Self> {
        let len = (width as usize).checked_mul(height as usize)?;
        let ok = width > 0
            && height > 0
            && tiles.len() == len
            && minerals.len() == len
            && roads.len() == len;
        ok.then_some(Self {
            width,
            height,
            tiles,
            minerals,
            roads,
            rivers: vec![false; len],
            enchanted_roads: vec![false; len],
            lakes: vec![false; len],
            corrupted: vec![false; len],
        })
    }

    /// The same grid with the given river layer; ignored if its length is wrong.
    pub fn with_rivers(mut self, rivers: Vec<bool>) -> Self {
        if rivers.len() == self.tiles.len() {
            self.rivers = rivers;
        }
        self
    }

    /// Terrain per tile, row by row.
    pub fn tiles(&self) -> &[TerrainId] {
        &self.tiles
    }

    /// Mineral per tile, row by row.
    pub fn minerals(&self) -> &[Option<FeatureId>] {
        &self.minerals
    }

    /// Road flag per tile, row by row (all false if roads were never set).
    pub fn roads(&self) -> Vec<bool> {
        let mut roads = self.roads.clone();
        roads.resize(self.tiles.len(), false);
        roads
    }

    /// Whether the tile at `(x, y)` has a road (x wraps, y outside the map is road-free).
    pub fn has_road(&self, x: i32, y: i32) -> bool {
        if y < 0 || y >= self.height as i32 {
            return false;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        self.roads.get(i).copied().unwrap_or(false)
    }

    pub fn set_road(&mut self, x: u32, y: u32, road: bool) {
        let i = self.index(x, y);
        if self.roads.len() != self.tiles.len() {
            self.roads = vec![false; self.tiles.len()];
        }
        self.roads[i] = road;
    }

    /// The same grid with the given corruption layer; ignored if its length is wrong.
    pub fn with_corruption(mut self, corrupted: Vec<bool>) -> Self {
        if corrupted.len() == self.tiles.len() {
            self.corrupted = corrupted;
        }
        self
    }

    /// Corruption flag per tile, row by row.
    pub fn corrupted(&self) -> Vec<bool> {
        let mut v = self.corrupted.clone();
        v.resize(self.tiles.len(), false);
        v
    }

    /// Whether `(x, y)` is corrupted (x wraps, y outside the map is clean).
    pub fn is_corrupted(&self, x: i32, y: i32) -> bool {
        if y < 0 || y >= self.height as i32 {
            return false;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        self.corrupted.get(i).copied().unwrap_or(false)
    }

    pub fn set_corrupted(&mut self, x: u32, y: u32, on: bool) {
        let i = self.index(x, y);
        if self.corrupted.len() != self.tiles.len() {
            self.corrupted = vec![false; self.tiles.len()];
        }
        self.corrupted[i] = on;
    }

    /// The same grid with the given lake layer; ignored if its length is wrong.
    pub fn with_lakes(mut self, lakes: Vec<bool>) -> Self {
        if lakes.len() == self.tiles.len() {
            self.lakes = lakes;
        }
        self
    }

    /// Lake flag per tile, row by row.
    pub fn lakes(&self) -> Vec<bool> {
        let mut v = self.lakes.clone();
        v.resize(self.tiles.len(), false);
        v
    }

    /// Whether `(x, y)` is a lake (x wraps, y outside the map has none).
    pub fn is_lake(&self, x: i32, y: i32) -> bool {
        if y < 0 || y >= self.height as i32 {
            return false;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        self.lakes.get(i).copied().unwrap_or(false)
    }

    pub fn set_lake(&mut self, x: u32, y: u32, lake: bool) {
        let i = self.index(x, y);
        if self.lakes.len() != self.tiles.len() {
            self.lakes = vec![false; self.tiles.len()];
        }
        self.lakes[i] = lake;
    }

    /// The same grid with the given enchanted-road layer; ignored if its length is wrong.
    pub fn with_enchanted_roads(mut self, enchanted: Vec<bool>) -> Self {
        if enchanted.len() == self.tiles.len() {
            self.enchanted_roads = enchanted;
        }
        self
    }

    /// Enchanted-road flag per tile, row by row.
    pub fn enchanted_roads(&self) -> Vec<bool> {
        let mut v = self.enchanted_roads.clone();
        v.resize(self.tiles.len(), false);
        v
    }

    /// Whether the road on `(x, y)` is enchanted (false without a road).
    pub fn has_enchanted_road(&self, x: i32, y: i32) -> bool {
        if y < 0 || y >= self.height as i32 {
            return false;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        self.enchanted_roads.get(i).copied().unwrap_or(false) && self.has_road(x, y)
    }

    pub fn set_enchanted_road(&mut self, x: u32, y: u32, on: bool) {
        let i = self.index(x, y);
        if self.enchanted_roads.len() != self.tiles.len() {
            self.enchanted_roads = vec![false; self.tiles.len()];
        }
        self.enchanted_roads[i] = on;
    }

    /// River flag per tile, row by row (all false if rivers were never set).
    pub fn rivers(&self) -> Vec<bool> {
        let mut rivers = self.rivers.clone();
        rivers.resize(self.tiles.len(), false);
        rivers
    }

    /// Whether a river runs through the tile at `(x, y)` (x wraps, y outside the map has none).
    pub fn has_river(&self, x: i32, y: i32) -> bool {
        if y < 0 || y >= self.height as i32 {
            return false;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        self.rivers.get(i).copied().unwrap_or(false)
    }

    pub fn set_river(&mut self, x: u32, y: u32, river: bool) {
        let i = self.index(x, y);
        if self.rivers.len() != self.tiles.len() {
            self.rivers = vec![false; self.tiles.len()];
        }
        self.rivers[i] = river;
    }

    fn index(&self, x: u32, y: u32) -> usize {
        assert!(x < self.width && y < self.height, "tile out of bounds");
        (y * self.width + x) as usize
    }

    /// Squared distance between two tiles, taking the horizontal wrap into account.
    pub fn distance_sq(&self, a: (u32, u32), b: (u32, u32)) -> u32 {
        let dx = a.0.abs_diff(b.0);
        let dx = dx.min(self.width - dx);
        let dy = a.1.abs_diff(b.1);
        dx * dx + dy * dy
    }

    /// Chebyshev distance (king moves) between two tiles, with horizontal wrap.
    pub fn chebyshev(&self, a: (u32, u32), b: (u32, u32)) -> u32 {
        let dx = a.0.abs_diff(b.0);
        dx.min(self.width - dx).max(a.1.abs_diff(b.1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    planes: [Grid; 2],
    /// Finds the tower, node, encounter site or volcano entity on a square.
    features: FeatureIndex,
}

impl Map {
    pub fn new(arcanus: Grid, myrror: Grid) -> Self {
        assert!(
            arcanus.width() == myrror.width() && arcanus.height() == myrror.height(),
            "both planes must have the same size"
        );
        Self {
            planes: [arcanus, myrror],
            features: FeatureIndex::default(),
        }
    }

    /// The feature entity (tower, node, encounter site or volcano) on a square.
    pub fn feature_entity(&self, at: Position) -> Option<hecs::Entity> {
        self.features.get(at)
    }

    /// Enter a feature entity in the index; a tower stands on both planes.
    pub(crate) fn index_feature(&mut self, entity: hecs::Entity, at: Position, both_planes: bool) {
        self.features.insert(at, entity);
        if both_planes {
            self.features.insert(at.on_other_plane(), entity);
        }
    }

    pub(crate) fn unindex_feature(&mut self, at: Position, both_planes: bool) {
        self.features.remove(at);
        if both_planes {
            self.features.remove(at.on_other_plane());
        }
    }

    pub fn plane(&self, plane: Plane) -> &Grid {
        &self.planes[plane.index()]
    }

    pub fn plane_mut(&mut self, plane: Plane) -> &mut Grid {
        &mut self.planes[plane.index()]
    }

    pub fn width(&self) -> u32 {
        self.planes[0].width()
    }

    pub fn height(&self) -> u32 {
        self.planes[0].height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_wraps_y_does_not() {
        let mut g = Grid::new(4, 3, TerrainId(0));
        g.set(3, 1, TerrainId(7));
        assert_eq!(g.get(-1, 1), Some(TerrainId(7)));
        assert_eq!(g.get(7, 1), Some(TerrainId(7)));
        assert_eq!(g.get(0, -1), None);
        assert_eq!(g.get(0, 3), None);
    }

    #[test]
    fn minerals_overlay_terrain_and_distance_wraps() {
        let mut g = Grid::new(10, 10, TerrainId(0));
        g.set_mineral(9, 2, Some(FeatureId(3)));
        assert_eq!(g.mineral(-1, 2), Some(FeatureId(3)));
        assert_eq!(g.mineral(0, 2), None);
        assert_eq!(g.get(9, 2), Some(TerrainId(0)));
        assert_eq!(g.distance_sq((9, 2), (0, 2)), 1);
        assert_eq!(g.chebyshev((9, 2), (1, 5)), 3);
    }
}
