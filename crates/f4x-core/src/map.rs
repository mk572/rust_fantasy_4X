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
    /// A river runs through the tile: a gold bonus for a city there, nothing else.
    #[serde(default)]
    rivers: Vec<bool>,
    /// Enchanted roads cost walking units no movement at all.
    #[serde(default)]
    enchanted_roads: Vec<bool>,
    /// Lake squares: ocean-class water that is not part of any sea.
    #[serde(default)]
    lakes: Vec<bool>,
    /// Volcano owner per tile: 0 for none, else the owner's player number plus one.
    #[serde(default)]
    volcanoes: Vec<u8>,
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
            rivers: vec![false; len],
            volcanoes: vec![0; len],
            enchanted_roads: vec![false; len],
            lakes: vec![false; len],
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

    /// Rebuild a grid from saved parts. Returns `None` if the lengths do not match.
    pub fn from_parts(
        width: u32,
        height: u32,
        tiles: Vec<TerrainId>,
        features: Vec<Option<FeatureId>>,
        roads: Vec<bool>,
    ) -> Option<Self> {
        let len = (width as usize).checked_mul(height as usize)?;
        let ok = width > 0
            && height > 0
            && tiles.len() == len
            && features.len() == len
            && roads.len() == len;
        ok.then_some(Self {
            width,
            height,
            tiles,
            features,
            roads,
            rivers: vec![false; len],
            volcanoes: vec![0; len],
            enchanted_roads: vec![false; len],
            lakes: vec![false; len],
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

    /// Feature per tile, row by row.
    pub fn features(&self) -> &[Option<FeatureId>] {
        &self.features
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

    /// The same grid with the given volcano layer (owner plus one, 0 for none); ignored if
    /// its length is wrong.
    pub fn with_volcanoes(mut self, volcanoes: Vec<u8>) -> Self {
        if volcanoes.len() == self.tiles.len() {
            self.volcanoes = volcanoes;
        }
        self
    }

    /// Volcano owner per tile, row by row (owner plus one, 0 for none).
    pub fn volcanoes(&self) -> Vec<u8> {
        let mut v = self.volcanoes.clone();
        v.resize(self.tiles.len(), 0);
        v
    }

    /// Player number of the wizard whose volcano stands on `(x, y)`, if there is one.
    pub fn volcano(&self, x: i32, y: i32) -> Option<u8> {
        if y < 0 || y >= self.height as i32 {
            return None;
        }
        let i = (y as u32 * self.width + self.wrap_x(x)) as usize;
        match self.volcanoes.get(i).copied().unwrap_or(0) {
            0 => None,
            n => Some(n - 1),
        }
    }

    pub fn set_volcano(&mut self, x: u32, y: u32, owner: Option<u8>) {
        let i = self.index(x, y);
        if self.volcanoes.len() != self.tiles.len() {
            self.volcanoes = vec![0; self.tiles.len()];
        }
        self.volcanoes[i] = owner.map_or(0, |o| o + 1);
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

/// A magic node: a feature tile with an aura of squares around it. A wizard who melds a
/// spirit into it owns it and draws power from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub plane: Plane,
    pub x: u32,
    pub y: u32,
    /// The realm of the node's magic: "sorcery", "nature" or "chaos".
    pub realm: String,
    /// Strength: how many squares the aura covers, and so the power the node yields.
    pub power: u8,
    /// The squares of the aura, the node's own tile first.
    pub aura: Vec<(u32, u32)>,
    /// Player number of the wizard who owns it.
    pub owner: Option<u8>,
    /// A Guardian Spirit holds the node: rivals who meld there only succeed one time in four.
    #[serde(default)]
    pub guardian: bool,
    /// A warped node drains its owner of five power instead of giving any.
    #[serde(default)]
    pub warped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    planes: [Grid; 2],
    #[serde(default)]
    nodes: Vec<Node>,
}

impl Map {
    pub fn new(arcanus: Grid, myrror: Grid) -> Self {
        assert!(
            arcanus.width() == myrror.width() && arcanus.height() == myrror.height(),
            "both planes must have the same size"
        );
        Self {
            planes: [arcanus, myrror],
            nodes: Vec::new(),
        }
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn set_nodes(&mut self, nodes: Vec<Node>) {
        self.nodes = nodes;
    }

    pub fn node_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.nodes.get_mut(index)
    }

    /// Index of the node standing on this tile.
    pub fn node_at(&self, plane: Plane, x: u32, y: u32) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.plane == plane && n.x == x && n.y == y)
    }

    /// Index of the node whose aura covers this tile (the node's own tile included).
    pub fn node_covering(&self, plane: Plane, x: u32, y: u32) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.plane == plane && n.aura.contains(&(x, y)))
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
