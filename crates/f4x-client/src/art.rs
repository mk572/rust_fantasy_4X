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
    /// Terrain type by neighbour mask, five tables of 256 (`terrain/autotile.tsv`): 0 for
    /// ocean shores, deserts and tundra, 1 for mountains and hills.
    autotile: Vec<Vec<usize>>,
    /// Whether a terrain type animates (its next three tiles are the other frames).
    type_anim: Vec<bool>,
    /// The banners' five shades each (`map/banners.tsv`).
    banners: Vec<[Color; 5]>,
    /// Tiles of rivers with everything but the water cut out, to lay over any ground.
    river_overlays: HashMap<usize, Option<Texture2D>>,
}

/// Banner names in the order the game numbers them.
pub const BANNER_NAMES: [&str; 6] = ["blue", "green", "purple", "red", "yellow", "neutral"];

impl Art {
    pub fn load(dir: PathBuf) -> Self {
        let types = std::fs::read_to_string(dir.join("terrain/types.tsv")).unwrap_or_default();
        let type_tiles = types
            .lines()
            .filter_map(|l| l.split('\t').nth(1)?.parse().ok())
            .collect();
        let type_anim = types
            .lines()
            .map(|l| l.split('\t').nth(2).is_some_and(|v| v.trim() == "1"))
            .collect();
        let mut autotile = vec![vec![0usize; 256]; 5];
        if let Ok(text) = std::fs::read_to_string(dir.join("terrain/autotile.tsv")) {
            for line in text.lines() {
                let mut f = line.split('\t').map(|v| v.parse::<usize>().ok());
                if let (Some(Some(table)), Some(Some(mask)), Some(Some(t))) =
                    (f.next(), f.next(), f.next())
                {
                    if table < 5 && mask < 256 {
                        autotile[table][mask] = t;
                    }
                }
            }
        }
        let mut banners = Vec::new();
        if let Ok(text) = std::fs::read_to_string(dir.join("map/banners.tsv")) {
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let shades: Vec<Color> = line
                    .split('\t')
                    .skip(2)
                    .filter_map(|c| {
                        let v: Vec<f32> = c.split(',').filter_map(|n| n.parse().ok()).collect();
                        (v.len() == 3)
                            .then(|| Color::new(v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, 1.0))
                    })
                    .collect();
                if let Ok(five) = <[Color; 5]>::try_from(shades) {
                    banners.push(five);
                }
            }
        }
        let styles = Self::load_font_metrics(&dir);
        Self {
            dir,
            styles,
            type_tiles,
            textures: HashMap::new(),
            autotile,
            type_anim,
            banners,
            river_overlays: HashMap::new(),
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
                    if let (Some(s), Some(h), Some(hs)) = (num(1), num(2), num(3)) {
                        if let Some(slot) = styles.get_mut(s as usize) {
                            *slot = Some(FontStyle {
                                height: h as u32,
                                hspacing: hs as i32,
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
            .filter(|c| (32..128).contains(&(*c as u32)))
            .map(|c| st.glyphs[(c as u32 - 32) as usize].1 as i32 + st.hspacing)
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
        self.autotile_type(0, mask)
    }

    /// The terrain type the original's table `table` gives for a neighbour mask.
    pub fn autotile_type(&self, table: usize, mask: usize) -> usize {
        self.autotile
            .get(table)
            .and_then(|t| t.get(mask))
            .copied()
            .unwrap_or(0)
    }

    /// A banner's colour (its middle shade); brown for numbers beyond the table.
    pub fn banner_color(&self, banner: u8) -> Color {
        self.banners
            .get(usize::from(banner))
            .map_or_else(|| default_banner_color(banner), |s| s[2])
    }

    /// All five shades of a banner.
    pub fn banner_shades(&self, banner: u8) -> [Color; 5] {
        self.banners
            .get(usize::from(banner))
            .copied()
            .unwrap_or([default_banner_color(banner); 5])
    }

    /// A picture recoloured for a banner by `extract-art` (`map/<name>_<banner>_<frame>.png`).
    pub fn banner_picture(&mut self, name: &str, banner: u8, frame: usize) -> Option<&Texture2D> {
        let colour = BANNER_NAMES
            .get(usize::from(banner))
            .copied()
            .unwrap_or("neutral");
        self.texture(PathBuf::from(format!("map/{name}_{colour}_{frame:02}.png")))
    }

    /// The ground tile for a terrain type at an animation frame (0 to 3; fixed tiles ignore it).
    pub fn terrain_frame(&mut self, terrain_type: usize, frame: usize) -> Option<&Texture2D> {
        let tile = *self.type_tiles.get(terrain_type)?;
        let animated = self.type_anim.get(terrain_type).copied().unwrap_or(false);
        let tile = if animated { tile + frame % 4 } else { tile };
        self.texture(PathBuf::from(format!("terrain/tile_{tile:04}.png")))
    }

    /// A river tile with only its water kept, so it can be laid over forest, desert and the rest.
    pub fn river_overlay(&mut self, terrain_type: usize) -> Option<&Texture2D> {
        if !self.river_overlays.contains_key(&terrain_type) {
            let tex = self.type_tiles.get(terrain_type).and_then(|&tile| {
                let bytes =
                    std::fs::read(self.dir.join(format!("terrain/tile_{tile:04}.png"))).ok()?;
                let mut img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
                for px in img.bytes.chunks_exact_mut(4) {
                    let water = i32::from(px[2]) > i32::from(px[0]) + 40
                        && i32::from(px[2]) > i32::from(px[1]) + 25;
                    if !water {
                        px[3] = 0;
                    }
                }
                let tex = Texture2D::from_image(&img);
                tex.set_filter(FilterMode::Nearest);
                Some(tex)
            });
            self.river_overlays.insert(terrain_type, tex);
        }
        self.river_overlays
            .get(&terrain_type)
            .and_then(Option::as_ref)
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

/// Colours to use before the original's banners are extracted.
fn default_banner_color(banner: u8) -> Color {
    match banner {
        0 => Color::new(0.33, 0.33, 0.65, 1.0),
        1 => Color::new(0.33, 0.65, 0.33, 1.0),
        2 => Color::new(0.65, 0.33, 0.65, 1.0),
        3 => Color::new(0.68, 0.0, 0.0, 1.0),
        4 => Color::new(1.0, 0.8, 0.14, 1.0),
        _ => Color::new(0.75, 0.6, 0.46, 1.0),
    }
}
