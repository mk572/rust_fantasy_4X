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
    /// Myrror's types follow Arcanus's.
    pub const MYRROR_OFFSET: usize = 762;
}

/// What the original's minimap is painted with (`extract-art` writes `minimap.tsv`): a colour
/// for each terrain type, and for the cities of each banner and of the neutral ones, the box
/// round the map window and the mark on the selected stack.
#[derive(Default)]
pub struct MinimapColours {
    terrain: Vec<Color>,
    banners: Vec<Color>,
    neutral: Option<Color>,
    box_: Option<Color>,
    mark: Option<Color>,
    /// Other colours by their row's name (`reserve_text`, `combat_text`...).
    named: HashMap<String, Color>,
}

impl MinimapColours {
    fn parse(text: &str) -> Self {
        let mut c = Self::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let [kind, _, r, g, b] = f[..] else { continue };
            let (Ok(r), Ok(g), Ok(b)) = (r.parse::<f32>(), g.parse::<f32>(), b.parse::<f32>())
            else {
                continue;
            };
            let colour = Color::new(r / 255.0, g / 255.0, b / 255.0, 1.0);
            match kind {
                "terrain" => c.terrain.push(colour),
                "banner" => c.banners.push(colour),
                "neutral" => c.neutral = Some(colour),
                "box" => c.box_ = Some(colour),
                "mark" => c.mark = Some(colour),
                other => {
                    c.named.insert(other.to_string(), colour);
                }
            }
        }
        c
    }

    /// The colour of a terrain type (Myrror's follow Arcanus's), black without the table.
    pub fn terrain(&self, terrain_type: usize) -> Color {
        self.terrain.get(terrain_type).copied().unwrap_or(BLACK)
    }

    /// The colour of a city of this banner (5 is the neutral banner).
    pub fn city(&self, banner: u8) -> Color {
        self.banners
            .get(usize::from(banner))
            .copied()
            .or(self.neutral)
            .unwrap_or(WHITE)
    }

    pub fn window_box(&self) -> Color {
        self.box_.unwrap_or(MAGENTA)
    }

    pub fn mark(&self) -> Color {
        self.mark.unwrap_or(WHITE)
    }

    /// A colour of the table by name, `fallback` without it.
    pub fn named(&self, kind: &str, fallback: Color) -> Color {
        self.named.get(kind).copied().unwrap_or(fallback)
    }
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

/// A combat figure: unit type, direction, animation frame and banner.
pub type FigureKey = (usize, u8, u8, u8);

/// A spell's change to the look of a unit's figures (`Combat_Figure_Effect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigureEffect {
    /// Black Sleep: in shades of grey.
    Asleep,
    /// Warp Creature: the shades of grey turned inside out, in red.
    Warped,
}

pub struct Art {
    dir: PathBuf,
    styles: Vec<Option<FontStyle>>,
    /// Terrain type -> first tile, from `map/tiles/types.tsv`.
    type_tiles: Vec<usize>,
    /// Terrain type -> the folder under `map/tiles/` its tiles are in (`arcanus/shore`).
    type_dirs: Vec<String>,
    textures: HashMap<PathBuf, Option<Texture2D>>,
    /// Pictures filed away from `art/<lbx>/`: lbx -> entry -> path from `assets-local/` without
    /// the frame (`screens/<screen>/<name>`, `art/units/<id>/<name>`; `index.tsv`, written by
    /// `extract-art`).
    placed: HashMap<String, HashMap<usize, String>>,
    /// Terrain type by neighbour mask, five tables of 256 (`map/tiles/autotile.tsv`): 0 for
    /// ocean shores, deserts and tundra, 1 for mountains and hills.
    autotile: Vec<Vec<usize>>,
    /// Whether a terrain type animates (its next three tiles are the other frames).
    type_anim: Vec<bool>,
    /// The minimap's colours (`map/tiles/minimap.tsv`).
    minimap: MinimapColours,
    /// The banners' five shades each (`map/banners.tsv`).
    banners: Vec<[Color; 5]>,
    /// Tiles of rivers with everything but the water cut out, to lay over any ground.
    river_overlays: HashMap<usize, Option<Texture2D>>,
    /// Combat figures painted in a banner, by (unit type, direction, frame, banner).
    figures: HashMap<FigureKey, Option<Texture2D>>,
    unit_pictures: HashMap<(String, usize, u8), Option<Texture2D>>,
    /// Figures with the glow of an enchantment: the figure's key, the realm and the step.
    glows: HashMap<(FigureKey, usize, usize), Option<Texture2D>>,
}

/// Banner names in the order the game numbers them.
pub const BANNER_NAMES: [&str; 6] = ["blue", "green", "purple", "red", "yellow", "neutral"];

impl Art {
    pub fn load(dir: PathBuf) -> Self {
        let types = std::fs::read_to_string(dir.join("map/tiles/types.tsv")).unwrap_or_default();
        let type_tiles = types
            .lines()
            .filter_map(|l| l.split('\t').nth(1)?.parse().ok())
            .collect();
        // The folder under `map/tiles/` of each type's tiles (`arcanus/shore`).
        let type_dirs = types
            .lines()
            .map(|l| l.split('\t').nth(3).unwrap_or("other").trim().to_string())
            .collect();
        let type_anim = types
            .lines()
            .map(|l| l.split('\t').nth(2).is_some_and(|v| v.trim() == "1"))
            .collect();
        let mut autotile = vec![vec![0usize; 256]; 5];
        if let Ok(text) = std::fs::read_to_string(dir.join("map/tiles/autotile.tsv")) {
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
        let minimap = MinimapColours::parse(
            &std::fs::read_to_string(dir.join("map/tiles/minimap.tsv")).unwrap_or_default(),
        );
        let styles = Self::load_font_metrics(&dir);
        let placed = parse_placed(
            &std::fs::read_to_string(dir.with_file_name("index.tsv")).unwrap_or_default(),
        );
        Self {
            dir,
            placed,
            styles,
            type_tiles,
            type_dirs,
            textures: HashMap::new(),
            autotile,
            type_anim,
            minimap,
            banners,
            river_overlays: HashMap::new(),
            figures: HashMap::new(),
            glows: HashMap::new(),
            unit_pictures: HashMap::new(),
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
        self.draw_bitmap_text_tinted(style, set, text, x, y, scale, WHITE)
    }

    /// Text in one flat colour, as the original draws a font whose colours are aliased to one
    /// palette entry: the lightest colour set, tinted so that its average is `colour`.
    pub fn draw_flat_text(
        &mut self,
        style: usize,
        text: &str,
        (x, y): (f32, f32),
        scale: f32,
        colour: Color,
    ) {
        let set = self.nearest_font_set(style, WHITE);
        let average = self
            .styles
            .get(style)
            .and_then(Option::as_ref)
            .and_then(|s| s.set_colors.get(set).copied())
            .unwrap_or(WHITE);
        let ratio = |c: f32, a: f32| (c / a.max(0.05)).min(1.0);
        let tint = Color::new(
            ratio(colour.r, average.r),
            ratio(colour.g, average.g),
            ratio(colour.b, average.b),
            1.0,
        );
        self.draw_bitmap_text_tinted(style, set, text, x, y, scale, tint);
    }

    /// [`Art::draw_bitmap_text`] multiplied by a colour.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_bitmap_text_tinted(
        &mut self,
        style: usize,
        set: usize,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        tint: Color,
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
                    tint,
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
    /// The terrain type the original's table `table` gives for a neighbour mask.
    pub fn autotile_type(&self, table: usize, mask: usize) -> usize {
        self.autotile
            .get(table)
            .and_then(|t| t.get(mask))
            .copied()
            .unwrap_or(0)
    }

    /// The minimap's colours: of a terrain type, and the other palette colours it uses.
    pub fn minimap(&self) -> &MinimapColours {
        &self.minimap
    }

    /// A banner's colour (its middle shade); brown for numbers beyond the table.
    pub fn banner_color(&self, banner: u8) -> Color {
        self.banners
            .get(usize::from(banner))
            .map_or_else(|| default_banner_color(banner), |s| s[2])
    }

    /// A picture recoloured for a banner by `extract-art` (`map/locations/<name>_<banner>_<frame>.png`).
    pub fn banner_picture(&mut self, name: &str, banner: u8, frame: usize) -> Option<&Texture2D> {
        let colour = BANNER_NAMES
            .get(usize::from(banner))
            .copied()
            .unwrap_or("neutral");
        self.texture(PathBuf::from(format!(
            "map/locations/{name}_{colour}_{frame:02}.png"
        )))
    }

    /// The ground tile for a terrain type at an animation frame (0 to 3; fixed tiles ignore it).
    pub fn terrain_frame(&mut self, terrain_type: usize, frame: usize) -> Option<&Texture2D> {
        let tile = *self.type_tiles.get(terrain_type)?;
        let animated = self.type_anim.get(terrain_type).copied().unwrap_or(false);
        let tile = if animated { tile + frame % 4 } else { tile };
        let path = self.tile_path(terrain_type, tile);
        self.texture(path)
    }

    /// A river tile with only its water kept, so it can be laid over forest, desert and the rest.
    pub fn river_overlay(&mut self, terrain_type: usize) -> Option<&Texture2D> {
        if !self.river_overlays.contains_key(&terrain_type) {
            let tex = self.type_tiles.get(terrain_type).and_then(|&tile| {
                let bytes =
                    std::fs::read(self.dir.join(self.tile_path(terrain_type, tile))).ok()?;
                let mut img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
                for px in img.bytes.as_chunks_mut::<4>().0.iter_mut() {
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

    /// A combat figure of a unit type facing one of eight directions (0 north, clockwise) at an
    /// animation frame, painted in a banner's shades. `None` without the extracted pictures.
    pub fn figure(
        &mut self,
        unit_type: usize,
        direction: u8,
        frame: u8,
        banner: u8,
    ) -> Option<&Texture2D> {
        let key = (unit_type, direction, frame, banner);
        if !self.figures.contains_key(&key) {
            let tex = self.load_figure(unit_type, direction, frame, banner);
            self.figures.insert(key, tex);
        }
        self.figures.get(&key).and_then(Option::as_ref)
    }

    /// A combat figure with the glow the original draws round an enchanted unit's figures
    /// (`Combat_Unit_Enchantment_Outline_Draw`): the transparent pixels next to the figure take
    /// the colours of the realm's ramp, which move with `step`. `realm` is 0 chaos, 1 death,
    /// 2 life, 3 nature, 4 sorcery.
    pub fn glowing_figure(
        &mut self,
        figure: FigureKey,
        realm: usize,
        step: usize,
    ) -> Option<&Texture2D> {
        const REALMS: [&str; 5] = ["chaos", "death", "life", "nature", "sorcery"];
        let key = (figure, realm, step % 8);
        if !self.glows.contains_key(&key) {
            let ramp: Vec<Color> = (0..8)
                .map(|i| {
                    self.minimap.named(
                        &format!("outline_{}_{}", REALMS[realm % 5], (i + step) % 8),
                        WHITE,
                    )
                })
                .collect();
            let glow = self
                .figure(figure.0, figure.1, figure.2, figure.3)
                .cloned()
                .map(|tex| {
                    let mut img = tex.get_texture_data();
                    outline_pixels(&mut img, &ramp);
                    let glowing = Texture2D::from_image(&img);
                    glowing.set_filter(FilterMode::Nearest);
                    glowing
                });
            self.glows.insert(key, glow);
        }
        self.glows.get(&key).and_then(Option::as_ref)
    }

    /// A figure under a spell's effect, recoloured pixel by pixel as the original does.
    pub fn affected_figure(
        &mut self,
        figure: FigureKey,
        effect: FigureEffect,
    ) -> Option<&Texture2D> {
        let key = (figure, 100 + effect as usize, 0);
        if !self.glows.contains_key(&key) {
            let changed = self
                .figure(figure.0, figure.1, figure.2, figure.3)
                .cloned()
                .map(|tex| {
                    let mut img = tex.get_texture_data();
                    for px in img
                        .bytes
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .filter(|p| p[3] > 0)
                    {
                        let grey =
                            (u32::from(px[0]) * 30 + u32::from(px[1]) * 59 + u32::from(px[2]) * 11)
                                / 100;
                        let grey = grey as u8;
                        match effect {
                            FigureEffect::Asleep => px[..3].fill(grey),
                            FigureEffect::Warped => {
                                let v = 255 - grey;
                                px[..3].copy_from_slice(&[v, v / 4, v / 4]);
                            }
                        }
                    }
                    let tex = Texture2D::from_image(&img);
                    tex.set_filter(FilterMode::Nearest);
                    tex
                });
            self.glows.insert(key, changed);
        }
        self.glows.get(&key).and_then(Option::as_ref)
    }

    fn load_figure(
        &self,
        unit_type: usize,
        direction: u8,
        frame: u8,
        banner: u8,
    ) -> Option<Texture2D> {
        let (lbx, entry) = figure_entry(unit_type, direction);
        self.load_painted(&lbx, entry, usize::from(frame), banner)
    }

    /// A picture painted in a banner's shades, where `extract-art` wrote a banner mask for it.
    fn load_painted(&self, lbx: &str, entry: usize, frame: usize, banner: u8) -> Option<Texture2D> {
        let base = picture_base(&self.placed, lbx, entry, frame);
        let bytes = std::fs::read(self.dir.join(format!("{base}.png"))).ok()?;
        let mut img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
        if let Some(shades) = self.banners.get(usize::from(banner)) {
            if let Ok(mask_bytes) = std::fs::read(self.dir.join(format!("{base}_banner.png"))) {
                if let Ok(mask) = Image::from_file_with_format(&mask_bytes, Some(ImageFormat::Png))
                {
                    let rgb = shades.map(|c| [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8));
                    paint_banner(&mut img.bytes, &mask.bytes, &rgb);
                }
            }
        }
        let tex = Texture2D::from_image(&img);
        tex.set_filter(FilterMode::Nearest);
        Some(tex)
    }

    /// A unit's overland picture (`units1`, `units2`) painted in a banner's shades.
    pub fn unit_picture(&mut self, lbx: &str, entry: usize, banner: u8) -> Option<&Texture2D> {
        let key = (lbx.to_string(), entry, banner);
        if !self.unit_pictures.contains_key(&key) {
            let tex = self.load_painted(lbx, entry, 0, banner);
            self.unit_pictures.insert(key.clone(), tex);
        }
        self.unit_pictures.get(&key).and_then(Option::as_ref)
    }

    /// A ground tile's file under the art directory (`map/tiles/<plane>/<kind>/tile_NNNN.png`,
    /// where `extract-art` put it).
    fn tile_path(&self, terrain_type: usize, tile: usize) -> PathBuf {
        let folder = self
            .type_dirs
            .get(terrain_type)
            .map_or("other", String::as_str);
        PathBuf::from(format!("map/tiles/{folder}/tile_{tile:04}.png"))
    }

    /// A river picture of a battlefield: one of the six shapes of Arcanus or of Myrror (see
    /// [`crate::river_art`]), from the embedded files.
    pub fn river(&mut self, shape: usize, frame: usize, myrror: bool) -> Option<&Texture2D> {
        let frame = frame % crate::river_art::FRAMES;
        let key = PathBuf::from(format!("river/{}_{shape}_{frame}", u8::from(myrror)));
        if !self.textures.contains_key(&key) {
            let tex = crate::river_art::png(shape, frame, myrror).and_then(|bytes| {
                let img = Image::from_file_with_format(bytes, Some(ImageFormat::Png)).ok()?;
                let tex = Texture2D::from_image(&img);
                tex.set_filter(FilterMode::Nearest);
                Some(tex)
            });
            self.textures.insert(key.clone(), tex);
        }
        self.textures.get(&key)?.as_ref()
    }

    /// The pixels of a picture that use the cycling palette index (`cycle.rs`), if it has any.
    pub fn cycle_mask(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Texture2D> {
        let base = picture_base(&self.placed, lbx, entry, frame);
        self.texture(PathBuf::from(format!("{base}_cycle.png")))
    }

    /// One of the original's mouse pointers, numbered from 1 (`crsr_Finger`).
    pub fn cursor(&mut self, number: usize) -> Option<&Texture2D> {
        self.texture(PathBuf::from(format!("cursors/{number:02}.png")))
    }

    /// A picture from an original `.lbx` file: lower-case name, entry, frame.
    pub fn picture(&mut self, lbx: &str, entry: usize, frame: usize) -> Option<&Texture2D> {
        let path = picture_path(&self.placed, lbx, entry, frame);
        self.texture(path)
    }
}

/// Paint the outline of a picture (`Outline_Bitmap_Pixels_No_Glass`, `Bitmap_Aura_Pixels`): every
/// transparent pixel with a figure pixel above, below or beside it takes the next colour of the
/// ramp, which advances as the picture is read a column at a time.
fn outline_pixels(img: &mut Image, ramp: &[Color]) {
    let (w, h) = (usize::from(img.width), usize::from(img.height));
    let inside: Vec<bool> = (0..w * h).map(|i| img.bytes[i * 4 + 3] > 0).collect();
    let (mut at, mut step) = (0usize, 0usize);
    for x in 0..w {
        for y in 0..h {
            let near = |dx: isize, dy: isize| {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                (0..w as isize).contains(&nx)
                    && (0..h as isize).contains(&ny)
                    && inside[ny as usize * w + nx as usize]
            };
            if !inside[y * w + x] && (near(-1, 0) || near(1, 0) || near(0, -1) || near(0, 1)) {
                let c = ramp[at % ramp.len()];
                let i = (y * w + x) * 4;
                img.bytes[i..i + 4].copy_from_slice(&[
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8,
                    255,
                ]);
            }
            at = (at + step) & 7;
            step = (step + 1) & 7;
        }
    }
}

/// Read `screens/index.tsv`: `lbx  entry  <screen>/<name>` a row.
fn parse_placed(text: &str) -> HashMap<String, HashMap<usize, String>> {
    let mut placed: HashMap<String, HashMap<usize, String>> = HashMap::new();
    for line in text.lines() {
        let mut f = line.split('\t');
        if let (Some(lbx), Some(Ok(entry)), Some(place)) =
            (f.next(), f.next().map(str::parse::<usize>), f.next())
        {
            placed
                .entry(lbx.to_string())
                .or_default()
                .insert(entry, place.to_string());
        }
    }
    placed
}

/// Where a picture's file is, relative to the art directory and without `.png`: in
/// `art/<lbx>/`, or in the folder `extract-art` filed it under (a screen's, a unit's).
fn picture_base(
    placed: &HashMap<String, HashMap<usize, String>>,
    lbx: &str,
    entry: usize,
    frame: usize,
) -> String {
    match placed.get(lbx).and_then(|e| e.get(&entry)) {
        Some(place) => format!("../{place}_{frame:02}"),
        None => format!("{lbx}/{entry:03}_{frame:02}"),
    }
}

fn picture_path(
    placed: &HashMap<String, HashMap<usize, String>>,
    lbx: &str,
    entry: usize,
    frame: usize,
) -> PathBuf {
    PathBuf::from(format!("{}.png", picture_base(placed, lbx, entry, frame)))
}

/// The picture file and entry of a unit type's figure facing `direction`: fifteen unit
/// types to a file, eight directions each (`Combat_Figure_Load`).
pub fn figure_entry(unit_type: usize, direction: u8) -> (String, usize) {
    let file = unit_type / 15 + 1;
    let name = if file < 10 {
        format!("figures{file}")
    } else {
        format!("figure{file}")
    };
    (name, (unit_type % 15) * 8 + usize::from(direction))
}

/// The unit type number the original's tables use for a unit whose overland picture is entry
/// `entry` of `units1` (types 0 to 119) or `units2` (120 on).
pub fn unit_type_number(lbx: &str, entry: usize) -> Option<usize> {
    match lbx {
        "units1" => Some(entry),
        "units2" => Some(120 + entry),
        _ => None,
    }
}

/// Paint the pixels a banner mask marks (red channel 1 to 5, alpha 255) in the banner's
/// shades. Both slices are RGBA of the same picture.
pub fn paint_banner(rgba: &mut [u8], mask: &[u8], shades: &[[u8; 3]; 5]) {
    for (px, m) in rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(mask.as_chunks::<4>().0)
    {
        if m[3] == 255 && (1..=5).contains(&m[0]) {
            let c = shades[usize::from(m[0]) - 1];
            px[..3].copy_from_slice(&c);
        }
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

#[cfg(test)]
mod figure_tests {
    use super::*;

    #[test]
    fn a_picture_of_one_screen_is_found_in_that_screens_folder() {
        let placed = parse_placed("main\t8\tscreens/main_screen/button_done\nbad line\n");
        assert_eq!(
            picture_path(&placed, "main", 8, 1),
            PathBuf::from("../screens/main_screen/button_done_01.png")
        );
        assert_eq!(
            picture_path(&placed, "main", 9, 0),
            PathBuf::from("main/009_00.png")
        );
        assert_eq!(
            picture_path(&placed, "backgrnd", 24, 0),
            PathBuf::from("backgrnd/024_00.png")
        );
    }

    #[test]
    fn figures_are_fifteen_types_to_a_file_with_eight_directions() {
        assert_eq!(figure_entry(0, 0), ("figures1".to_string(), 0));
        assert_eq!(figure_entry(14, 7), ("figures1".to_string(), 119));
        assert_eq!(figure_entry(15, 2), ("figures2".to_string(), 2));
        assert_eq!(figure_entry(135, 0), ("figure10".to_string(), 0));
        assert_eq!(figure_entry(197, 7), ("figure14".to_string(), 2 * 8 + 7));
    }

    #[test]
    fn unit_types_follow_the_two_unit_picture_files() {
        assert_eq!(unit_type_number("units1", 5), Some(5));
        assert_eq!(unit_type_number("units2", 5), Some(125));
        assert_eq!(unit_type_number("mainscrn", 5), None);
    }

    #[test]
    fn banner_pixels_take_the_banners_shades() {
        let mut px = [9u8, 9, 9, 255, 7, 7, 7, 255, 1, 2, 3, 255];
        let mask = [0u8, 0, 0, 0, 2, 0, 0, 255, 5, 0, 0, 255];
        let shades = [[10; 3], [20; 3], [30; 3], [40; 3], [50; 3]];
        paint_banner(&mut px, &mask, &shades);
        assert_eq!(px, [9, 9, 9, 255, 20, 20, 20, 255, 50, 50, 50, 255]);
    }
}
