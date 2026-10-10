//! Loads the locally extracted original art (`assets-local/art`) on demand.
//! Everything is optional: a missing file just means the caller draws a flat
//! placeholder, so the game runs and CI passes without the original assets.

use std::collections::HashMap;
use std::path::PathBuf;

use macroquad::prelude::*;

pub const TILE_W: f32 = 20.0;
pub const TILE_H: f32 = 18.0;

/// Terrain type numbers in the original's tile table.
pub mod terrain_type {
    pub const OCEAN: usize = 0x00;
    pub const GRASSLAND: usize = 0xA2;
    pub const FOREST: usize = 0xA3;
    pub const MOUNTAINS: usize = 0xA4;
    pub const DESERT: usize = 0xA5;
    pub const SWAMP: usize = 0xA6;
    pub const TUNDRA: usize = 0xA7;
    pub const SORCERY_NODE: usize = 0xA8;
    pub const NATURE_NODE: usize = 0xA9;
    pub const CHAOS_NODE: usize = 0xAA;
    pub const HILLS: usize = 0xAB;
    /// Myrror's types follow Arcanus's.
    pub const MYRROR_OFFSET: usize = 762;
}

pub struct Art {
    dir: PathBuf,
    /// Terrain type -> first tile, from `terrain/types.tsv`.
    type_tiles: Vec<usize>,
    textures: HashMap<PathBuf, Option<Texture2D>>,
    /// Shore type by neighbour mask (table 0 of `terrain/autotile.tsv`); empty if absent.
    shore_types: Vec<usize>,
}

impl Art {
    pub fn load(dir: PathBuf) -> Self {
        let type_tiles = std::fs::read_to_string(dir.join("terrain/types.tsv"))
            .map(|text| {
                text.lines()
                    .filter_map(|l| l.split('\t').nth(1)?.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        let mut shore_types = vec![0; 256];
        if let Ok(text) = std::fs::read_to_string(dir.join("terrain/autotile.tsv")) {
            for line in text.lines() {
                let mut f = line.split('\t').map(|v| v.parse::<usize>().ok());
                if let (Some(Some(0)), Some(Some(mask)), Some(Some(t))) =
                    (f.next(), f.next(), f.next())
                {
                    if mask < 256 {
                        shore_types[mask] = t;
                    }
                }
            }
        }
        Self {
            dir,
            type_tiles,
            textures: HashMap::new(),
            shore_types,
        }
    }

    pub fn available(&self) -> bool {
        !self.type_tiles.is_empty()
    }

    fn texture(&mut self, relative: PathBuf) -> Option<&Texture2D> {
        let path = self.dir.join(&relative);
        self.textures
            .entry(relative)
            .or_insert_with(|| {
                let bytes = std::fs::read(&path).ok()?;
                let tex = Texture2D::from_file_with_format(&bytes, Some(ImageFormat::Png));
                tex.set_filter(FilterMode::Nearest);
                Some(tex)
            })
            .as_ref()
    }

    /// The ocean tile type for a mask of land neighbours (NW 1, N 2, NE 4, E 8, SE 16, S 32,
    /// SW 64, W 128): plain ocean when there is no land or no table.
    pub fn shore_type(&self, mask: usize) -> usize {
        self.shore_types.get(mask).copied().unwrap_or(0)
    }

    /// The ground tile for a terrain type number (see [`terrain_type`]).
    pub fn terrain(&mut self, terrain_type: usize) -> Option<&Texture2D> {
        let tile = *self.type_tiles.get(terrain_type)?;
        self.texture(PathBuf::from(format!("terrain/tile_{tile:04}.png")))
    }

    /// A picture from an original `.lbx` file: lower-case name, entry, frame.
    pub fn picture(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Texture2D> {
        self.texture(PathBuf::from(format!("{lbx}/{entry:03}_{frame:02}.png")))
    }
}
