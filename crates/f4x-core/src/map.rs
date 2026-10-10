//! Two-plane tile map. The world wraps horizontally and is bounded vertically.

use serde::{Deserialize, Serialize};

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

/// Index into the loaded [`crate::content::Terrains`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerrainId(pub u16);

/// Index into the loaded [`crate::content::Features`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureId(pub u16);

/// One plane: a base terrain layer plus an optional feature overlay per tile
/// (minerals, encounter sites, towers, nodes). Cities are entities, not tiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    width: u32,
    height: u32,
    tiles: Vec<TerrainId>,
    features: Vec<Option<FeatureId>>,
    /// Roads make movement between two road tiles cost half a move.
    #[serde(default)]
    roads: Vec<bool>,
}

impl Grid {
    pub fn new(width: u32, height: u32, fill: TerrainId) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        let len = (width * height) as usize;
        Self {
            width,
            height,
            tiles: vec![fill; len],
            features: vec![None; len],
            roads: vec![false; len],
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

    /// Feature on the tile at `(x, y)` (x wraps, y outside the map yields `None`).
    pub fn feature(&self, x: i32, y: i32) -> Option<FeatureId> {
        if y < 0 || y >= self.height as i32 {
            return None;
        }
        self.features[(y as u32 * self.width + self.wrap_x(x)) as usize]
    }

    pub fn set_feature(&mut self, x: u32, y: u32, feature: Option<FeatureId>) {
        let i = self.index(x, y);
        self.features[i] = feature;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    planes: [Grid; 2],
}

impl Map {
    pub fn new(arcanus: Grid, myrror: Grid) -> Self {
        assert!(
            arcanus.width() == myrror.width() && arcanus.height() == myrror.height(),
            "both planes must have the same size"
        );
        Self {
            planes: [arcanus, myrror],
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
    fn features_overlay_terrain_and_distance_wraps() {
        let mut g = Grid::new(10, 10, TerrainId(0));
        g.set_feature(9, 2, Some(FeatureId(3)));
        assert_eq!(g.feature(-1, 2), Some(FeatureId(3)));
        assert_eq!(g.feature(0, 2), None);
        assert_eq!(g.get(9, 2), Some(TerrainId(0)));
        assert_eq!(g.distance_sq((9, 2), (0, 2)), 1);
        assert_eq!(g.chebyshev((9, 2), (1, 5)), 3);
    }
}
