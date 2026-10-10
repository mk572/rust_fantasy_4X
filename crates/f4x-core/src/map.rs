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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    width: u32,
    height: u32,
    tiles: Vec<TerrainId>,
}

impl Grid {
    pub fn new(width: u32, height: u32, fill: TerrainId) -> Self {
        assert!(width > 0 && height > 0, "grid must be non-empty");
        Self {
            width,
            height,
            tiles: vec![fill; (width * height) as usize],
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
        assert!(x < self.width && y < self.height, "tile out of bounds");
        self.tiles[(y * self.width + x) as usize] = terrain;
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
}
