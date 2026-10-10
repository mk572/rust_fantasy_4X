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

/// One of the original's bitmap fonts: an atlas with the glyph strip once per colour set.
struct FontStyle {
    height: u32,
    hspacing: i32,
    vspacing: i32,
    /// `(x in the strip, width)` for ASCII 32..=127.
    glyphs: Vec<(u32, u32)>,
    /// Average colour of each of the 16 colour sets, to pick the nearest one to a request.
    set_colors: Vec<Color>,
}

pub struct Art {
    dir: PathBuf,
    styles: Vec<Option<FontStyle>>,
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
        let styles = Self::load_font_metrics(&dir);
        Self {
            dir,
            styles,
            type_tiles,
            textures: HashMap::new(),
            shore_types,
        }
    }

    /// Read `fonts/metrics.tsv` (see `f4x-tools` `extract-art`); the atlases load on demand.
    fn load_font_metrics(dir: &std::path::Path) -> Vec<Option<FontStyle>> {
        let mut styles: Vec<Option<FontStyle>> = (0..8).map(|_| None).collect();
        let Ok(text) = std::fs::read_to_string(dir.join("fonts/metrics.tsv")) else {
            return styles;
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<i64>().ok());
            match f.first().copied() {
                Some("S") => {
                    if let (Some(s), Some(h), Some(hs), Some(vs)) = (num(1), num(2), num(3), num(4))
                    {
                        if let Some(slot) = styles.get_mut(s as usize) {
                            *slot = Some(FontStyle {
                                height: h as u32,
                                hspacing: hs as i32,
                                vspacing: vs as i32,
                                glyphs: vec![(0, 0); 96],
                                set_colors: Vec::new(),
                            });
                        }
                    }
                }
                Some("G") => {
                    if let (Some(s), Some(code), Some(x), Some(w)) =
                        (num(1), num(2), num(3), num(4))
                    {
                        if let Some(Some(style)) = styles.get_mut(s as usize) {
                            if (32..128).contains(&code) {
                                style.glyphs[(code - 32) as usize] = (x as u32, w as u32);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        styles
    }

    /// Whether the original fonts were extracted.
    pub fn has_fonts(&self) -> bool {
        self.styles.iter().any(Option::is_some)
    }

    /// Width in the original's pixels of `text` in a font style.
    pub fn text_width(&self, style: usize, text: &str) -> f32 {
        let Some(Some(st)) = self.styles.get(style) else {
            return text.len() as f32 * 4.0;
        };
        text.chars()
            .filter_map(|c| {
                (32..128)
                    .contains(&(c as u32))
                    .then(|| st.glyphs[(c as u32 - 32) as usize].1 as i32 + st.hspacing)
            })
            .sum::<i32>() as f32
    }

    /// Height of a font style in the original's pixels.
    pub fn text_height(&self, style: usize) -> f32 {
        self.styles
            .get(style)
            .and_then(Option::as_ref)
            .map_or(8.0, |s| s.height as f32)
    }

    /// The colour set whose average colour is closest to `color`.
    pub fn nearest_font_set(&mut self, style: usize, color: Color) -> usize {
        self.ensure_set_colors(style);
        let Some(Some(st)) = self.styles.get(style) else {
            return 0;
        };
        st.set_colors
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let d = |c: &Color| {
                    (c.r - color.r).powi(2) + (c.g - color.g).powi(2) + (c.b - color.b).powi(2)
                };
                d(a).total_cmp(&d(b))
            })
            .map_or(0, |(i, _)| i)
    }

    fn ensure_set_colors(&mut self, style: usize) {
        let needs = matches!(self.styles.get(style), Some(Some(s)) if s.set_colors.is_empty());
        if !needs {
            return;
        }
        let Some(tex) = self
            .texture(PathBuf::from(format!("fonts/style{style}.png")))
            .cloned()
        else {
            return;
        };
        let img = tex.get_texture_data();
        let (w, h) = (img.width(), img.height());
        let fh = self.styles[style]
            .as_ref()
            .map_or(1, |s| s.height as usize)
            .max(1);
        let mut colors = Vec::new();
        for set in 0..16 {
            let (mut r, mut g, mut b, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for y in set * fh..((set + 1) * fh).min(h) {
                for x in 0..w {
                    let p = img.bytes[(y * w + x) * 4..(y * w + x) * 4 + 4].to_vec();
                    if p[3] > 0 {
                        r += f32::from(p[0]);
                        g += f32::from(p[1]);
                        b += f32::from(p[2]);
                        n += 1.0;
                    }
                }
            }
            colors.push(if n > 0.0 {
                Color::new(r / n / 255.0, g / n / 255.0, b / n / 255.0, 1.0)
            } else {
                Color::new(0.0, 0.0, 0.0, 1.0)
            });
        }
        if let Some(Some(st)) = self.styles.get_mut(style) {
            st.set_colors = colors;
        }
    }

    /// Draw `text` in the original's font: `(x, y)` is the top-left in window pixels and
    /// `scale` the window pixels per original pixel. Returns the width drawn.
    pub fn draw_bitmap_text(
        &mut self,
        style: usize,
        set: usize,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
    ) -> f32 {
        let Some(tex) = self
            .texture(PathBuf::from(format!("fonts/style{style}.png")))
            .cloned()
        else {
            return 0.0;
        };
        let Some(Some(st)) = self.styles.get(style) else {
            return 0.0;
        };
        let (height, hspacing) = (st.height as f32, st.hspacing as f32);
        let mut at = x;
        for c in text.chars() {
            if !(32..128).contains(&(c as u32)) {
                continue;
            }
            let (gx, gw) = st.glyphs[(c as u32 - 32) as usize];
            if gw > 0 {
                draw_texture_ex(
                    &tex,
                    at,
                    y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(gw as f32 * scale, height * scale)),
                        source: Some(Rect::new(
                            gx as f32,
                            set.min(15) as f32 * height,
                            gw as f32,
                            height,
                        )),
                        ..Default::default()
                    },
                );
            }
            at += (gw as f32 + hspacing) * scale;
        }
        at - x
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
